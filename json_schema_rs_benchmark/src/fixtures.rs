//! Fixture discovery shared by the in-process tier (criterion bench and heap profiler).
//!
//! A *fixture root* is a directory laid out as `<group>/<name>/schema.json`, with optional
//! instance documents under `<group>/<name>/instances/valid/*.json` and
//! `<group>/<name>/instances/invalid/*.json`. The fixture id is `<group>/<name>`
//! (e.g. `small/config`, `generated/wide-object`).
//!
//! Roots come from the `BENCH_FIXTURE_ROOTS` environment variable (`:`-separated absolute
//! directories). When it is unset or empty, the roots are the committed fixtures directory
//! (`<CARGO_MANIFEST_DIR>/fixtures`) and the generated fixtures directory
//! (`<CARGO_MANIFEST_DIR>/../target/benchmark-fixtures`). A root that does not exist is
//! skipped, so the generated tier is optional.
//!
//! Every fixture's text is loaded eagerly so measured code never touches the filesystem.
//! Discovery walks directories with plain loops (no recursion) and returns fixtures sorted
//! by id.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Environment variable holding `:`-separated fixture root directories.
pub const FIXTURE_ROOTS_ENV: &str = "BENCH_FIXTURE_ROOTS";

/// File name of a fixture's schema document.
pub const SCHEMA_FILE_NAME: &str = "schema.json";

/// One benchmark fixture: a schema plus the instances used by the validation operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    /// Fixture id, `<group>/<name>` (always `/`-separated).
    pub id: String,
    /// Directory holding `schema.json` and `instances/`.
    pub dir: PathBuf,
    /// Full text of `schema.json`.
    pub schema_text: String,
    /// Text of the first file (sorted by file name) in `instances/valid`, if any.
    pub valid_instance: Option<String>,
    /// Text of the first file (sorted by file name) in `instances/invalid`, if any.
    pub invalid_instance: Option<String>,
}

/// Failure while discovering or loading fixtures.
#[derive(Debug)]
pub enum FixtureError {
    /// A directory or file could not be read.
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// A directory name on a fixture path is not valid UTF-8, so it cannot form an id.
    NonUtf8Name {
        /// Offending path.
        path: PathBuf,
    },
    /// Two roots provide a fixture with the same id.
    DuplicateId {
        /// The duplicated fixture id.
        id: String,
        /// Directory of the fixture discovered first.
        first: PathBuf,
        /// Directory of the fixture discovered second.
        second: PathBuf,
    },
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixtureError::Io { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            FixtureError::NonUtf8Name { path } => {
                write!(f, "fixture path is not valid UTF-8: {}", path.display())
            }
            FixtureError::DuplicateId { id, first, second } => write!(
                f,
                "duplicate fixture id {id}: {} and {}",
                first.display(),
                second.display()
            ),
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FixtureError::Io { source, .. } => Some(source),
            FixtureError::NonUtf8Name { .. } | FixtureError::DuplicateId { .. } => None,
        }
    }
}

/// The default fixture roots: committed fixtures, then generated fixtures.
#[must_use]
pub fn default_roots() -> Vec<PathBuf> {
    let manifest_dir: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        manifest_dir.join("fixtures"),
        manifest_dir
            .join("..")
            .join("target")
            .join("benchmark-fixtures"),
    ]
}

/// Parse a `:`-separated root list, dropping empty segments.
#[must_use]
pub fn parse_roots(value: &str) -> Vec<PathBuf> {
    value
        .split(':')
        .filter(|segment: &&str| !segment.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Fixture roots from [`FIXTURE_ROOTS_ENV`], or [`default_roots`] when it is unset or empty.
#[must_use]
pub fn configured_roots() -> Vec<PathBuf> {
    let from_env: Vec<PathBuf> = std::env::var(FIXTURE_ROOTS_ENV)
        .map(|value: String| parse_roots(&value))
        .unwrap_or_default();
    if from_env.is_empty() {
        default_roots()
    } else {
        from_env
    }
}

/// Discover and load every fixture under the [`configured_roots`].
///
/// # Errors
///
/// Returns a [`FixtureError`] when a directory or file cannot be read, a path is not UTF-8,
/// or two roots provide the same fixture id.
pub fn discover_configured() -> Result<Vec<Fixture>, FixtureError> {
    discover(&configured_roots())
}

/// Discover and load every `<group>/<name>/schema.json` fixture under `roots`, sorted by id.
///
/// Roots that do not exist are skipped.
///
/// # Errors
///
/// Returns a [`FixtureError`] when a directory or file cannot be read, a path is not UTF-8,
/// or two roots provide the same fixture id.
pub fn discover(roots: &[PathBuf]) -> Result<Vec<Fixture>, FixtureError> {
    let mut found: BTreeMap<String, PathBuf> = BTreeMap::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for group_dir in sorted_subdirs(root)? {
            let group: String = utf8_file_name(&group_dir)?;
            for fixture_dir in sorted_subdirs(&group_dir)? {
                if !fixture_dir.join(SCHEMA_FILE_NAME).is_file() {
                    continue;
                }
                let name: String = utf8_file_name(&fixture_dir)?;
                let id: String = format!("{group}/{name}");
                if let Some(first) = found.get(&id) {
                    return Err(FixtureError::DuplicateId {
                        id,
                        first: first.clone(),
                        second: fixture_dir,
                    });
                }
                found.insert(id, fixture_dir);
            }
        }
    }
    found
        .into_iter()
        .map(|(id, dir): (String, PathBuf)| load_fixture(id, dir))
        .collect()
}

/// Load one fixture's schema and first valid/invalid instance texts.
///
/// # Errors
///
/// Returns [`FixtureError::Io`] when a file or instance directory cannot be read.
pub fn load_fixture(id: String, dir: PathBuf) -> Result<Fixture, FixtureError> {
    let schema_text: String = read_text(&dir.join(SCHEMA_FILE_NAME))?;
    let instances_dir: PathBuf = dir.join("instances");
    let valid_instance: Option<String> = first_instance(&instances_dir.join("valid"))?;
    let invalid_instance: Option<String> = first_instance(&instances_dir.join("invalid"))?;
    Ok(Fixture {
        id,
        dir,
        schema_text,
        valid_instance,
        invalid_instance,
    })
}

impl Fixture {
    /// Every `$ref` in the schema that points outside the document (does not start with `#`),
    /// sorted and deduplicated. Empty when the schema is self-contained or is not valid JSON.
    ///
    /// The benchmark runs offline, so a fixture with external references is never timed: a
    /// tool would either fail to resolve them or spend the measurement fetching them.
    #[must_use]
    pub fn external_refs(&self) -> Vec<String> {
        serde_json::from_str::<serde_json::Value>(&self.schema_text)
            .map(|schema: serde_json::Value| external_refs(&schema))
            .unwrap_or_default()
    }
}

/// Every `$ref` string in `schema` that does not start with `#`, sorted and deduplicated. Walks
/// the document with an explicit stack.
#[must_use]
pub fn external_refs(schema: &serde_json::Value) -> Vec<String> {
    let mut refs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut stack: Vec<&serde_json::Value> = vec![schema];
    while let Some(node) = stack.pop() {
        match node {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    if key == "$ref"
                        && let serde_json::Value::String(target) = value
                        && !target.starts_with('#')
                    {
                        refs.insert(target.clone());
                    }
                    stack.push(value);
                }
            }
            serde_json::Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    refs.into_iter().collect()
}

/// Text of the first `*.json` file (sorted by file name) in `dir`; `None` when the directory
/// is absent or holds no JSON file.
fn first_instance(dir: &Path) -> Result<Option<String>, FixtureError> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut files: Vec<PathBuf> = read_dir_paths(dir)?
        .into_iter()
        .filter(|path: &PathBuf| {
            path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json")
        })
        .collect();
    files.sort();
    match files.first() {
        Some(path) => read_text(path).map(Some),
        None => Ok(None),
    }
}

/// Immediate subdirectories of `dir`, sorted by path.
fn sorted_subdirs(dir: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let mut dirs: Vec<PathBuf> = read_dir_paths(dir)?
        .into_iter()
        .filter(|path: &PathBuf| path.is_dir())
        .collect();
    dirs.sort();
    Ok(dirs)
}

/// Every entry path of `dir`.
fn read_dir_paths(dir: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    let io_err = |source: io::Error| FixtureError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_err)? {
        let entry: fs::DirEntry = entry.map_err(io_err)?;
        paths.push(entry.path());
    }
    Ok(paths)
}

fn read_text(path: &Path) -> Result<String, FixtureError> {
    fs::read_to_string(path).map_err(|source: io::Error| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn utf8_file_name(path: &Path) -> Result<String, FixtureError> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| FixtureError::NonUtf8Name {
            path: path.to_path_buf(),
        })
}

#[cfg(test)]
mod tests {
    use super::{Fixture, FixtureError, SCHEMA_FILE_NAME, discover, external_refs, parse_roots};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// A fresh, empty scratch directory unique to `name` and this process.
    fn scratch_dir(name: &str) -> PathBuf {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "json-schema-rs-benchmark-fixtures-{}-{name}",
            std::process::id()
        ));
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("remove scratch dir");
        }
        fs::create_dir_all(&dir).expect("create scratch dir");
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(path, text).expect("write file");
    }

    #[test]
    fn parse_roots_splits_and_drops_empty_segments() {
        let expected: Vec<PathBuf> = vec![PathBuf::from("/a"), PathBuf::from("/b/c")];
        let actual: Vec<PathBuf> = parse_roots(":/a::/b/c:");
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_loads_sorted_fixtures_with_first_instances() {
        let root: PathBuf = scratch_dir("sorted");
        write(&root.join("small/zeta").join(SCHEMA_FILE_NAME), "{\"z\":1}");
        write(&root.join("small/alpha").join(SCHEMA_FILE_NAME), "{}");
        write(&root.join("small/alpha/instances/valid/b.json"), "2");
        write(&root.join("small/alpha/instances/valid/a.json"), "1");
        write(&root.join("small/alpha/instances/valid/0.txt"), "ignored");
        write(&root.join("small/alpha/instances/invalid/x.json"), "3");
        fs::create_dir_all(root.join("small/no-schema")).expect("create dir");
        let expected: Vec<Fixture> = vec![
            Fixture {
                id: "small/alpha".to_owned(),
                dir: root.join("small").join("alpha"),
                schema_text: "{}".to_owned(),
                valid_instance: Some("1".to_owned()),
                invalid_instance: Some("3".to_owned()),
            },
            Fixture {
                id: "small/zeta".to_owned(),
                dir: root.join("small").join("zeta"),
                schema_text: "{\"z\":1}".to_owned(),
                valid_instance: None,
                invalid_instance: None,
            },
        ];
        let actual: Vec<Fixture> = discover(std::slice::from_ref(&root)).expect("discover");
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_skips_missing_root() {
        let missing: PathBuf = scratch_dir("missing").join("does-not-exist");
        let expected: Vec<Fixture> = Vec::new();
        let actual: Vec<Fixture> = discover(&[missing]).expect("discover");
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_merges_roots_sorted_by_id() {
        let first: PathBuf = scratch_dir("merge-first");
        let second: PathBuf = scratch_dir("merge-second");
        write(&first.join("small/b").join(SCHEMA_FILE_NAME), "{}");
        write(&second.join("generated/a").join(SCHEMA_FILE_NAME), "{}");
        let expected: Vec<String> = vec!["generated/a".to_owned(), "small/b".to_owned()];
        let actual: Vec<String> = discover(&[first, second])
            .expect("discover")
            .into_iter()
            .map(|fixture: Fixture| fixture.id)
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_rejects_duplicate_ids() {
        let first: PathBuf = scratch_dir("dup-first");
        let second: PathBuf = scratch_dir("dup-second");
        write(&first.join("small/a").join(SCHEMA_FILE_NAME), "{}");
        write(&second.join("small/a").join(SCHEMA_FILE_NAME), "{}");
        let actual: Result<Vec<Fixture>, FixtureError> = discover(&[first, second]);
        assert!(matches!(actual, Err(FixtureError::DuplicateId { ref id, .. }) if id == "small/a"));
    }

    #[test]
    fn external_refs_collects_non_fragment_refs_sorted_and_deduplicated() {
        let schema: serde_json::Value = serde_json::json!({
            "$defs": { "a": { "type": "string" } },
            "properties": {
                "local": { "$ref": "#/$defs/a" },
                "remote": { "$ref": "https://example.com/b.json" },
                "list": { "items": [{ "$ref": "ava.json" }, { "$ref": "ava.json" }] }
            }
        });
        let expected: Vec<String> = vec![
            "ava.json".to_owned(),
            "https://example.com/b.json".to_owned(),
        ];
        let actual: Vec<String> = external_refs(&schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn external_refs_is_empty_for_a_self_contained_schema() {
        let schema: serde_json::Value = serde_json::json!({
            "$defs": { "a": { "type": "string" } },
            "properties": { "local": { "$ref": "#/$defs/a" } }
        });
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = external_refs(&schema);
        assert_eq!(expected, actual);
    }
}
