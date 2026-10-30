//! Fixture discovery shared by the in-process tier (criterion bench and heap profiler).
//!
//! A *fixture root* is a directory laid out as `<group>/<name>/schema.json`, with optional
//! instance documents under `<group>/<name>/instances/valid/*.json` and
//! `<group>/<name>/instances/invalid/*.json`. The fixture id is `<group>/<name>`
//! (e.g. `small/config`, `generated/wide-object`).
//!
//! A fixture may also vendor the documents its schema references under
//! `<group>/<name>/resources/*.json`. Each resource declares an absolute root `$id`, and every
//! tool registers it under that URI, so a `$ref` to another document resolves without the
//! network. A fixture is measured only when every `$ref` in its schema and resources resolves to
//! one of its documents (see [`Fixture::unresolved_refs`]).
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

use fluent_uri::{Iri, IriRef};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Environment variable holding `:`-separated fixture root directories.
pub const FIXTURE_ROOTS_ENV: &str = "BENCH_FIXTURE_ROOTS";

/// File name of a fixture's schema document.
pub const SCHEMA_FILE_NAME: &str = "schema.json";

/// Name of the directory holding a fixture's vendored resource documents.
pub const RESOURCES_DIR_NAME: &str = "resources";

/// The base URI of a document without a root `$id` (the default base URI `json-schema-rs` uses
/// for a schema it was given without a retrieval URI). A relative `$ref` in such a document
/// cannot reach a resource, because resources are registered under their absolute `$id`.
const DEFAULT_BASE_URI: &str = "json-schema:///";

/// A document a fixture's schema references, vendored with the fixture so the run stays offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    /// The document's root `$id`, absolute, without fragment and normalised (RFC 3986 §6.2.2):
    /// the URI every tool registers the document under.
    pub uri: String,
    /// Path of the document's file.
    pub path: PathBuf,
    /// Full text of the document.
    pub text: String,
}

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
    /// Every `*.json` document in `resources/`, sorted by file name.
    pub resources: Vec<Resource>,
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
    /// A resource document is not JSON, or has no absolute root `$id` to register it under.
    InvalidResource {
        /// The resource's file.
        path: PathBuf,
        /// Why it cannot be registered.
        reason: String,
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
            FixtureError::InvalidResource { path, reason } => {
                write!(f, "invalid resource {}: {reason}", path.display())
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
            FixtureError::NonUtf8Name { .. }
            | FixtureError::InvalidResource { .. }
            | FixtureError::DuplicateId { .. } => None,
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

/// Load one fixture's schema, first valid/invalid instance texts, and resources.
///
/// # Errors
///
/// Returns [`FixtureError::Io`] when a file or directory cannot be read, and
/// [`FixtureError::InvalidResource`] when a resource is not JSON or has no absolute root `$id`.
pub fn load_fixture(id: String, dir: PathBuf) -> Result<Fixture, FixtureError> {
    let schema_text: String = read_text(&dir.join(SCHEMA_FILE_NAME))?;
    let instances_dir: PathBuf = dir.join("instances");
    let valid_instance: Option<String> = first_instance(&instances_dir.join("valid"))?;
    let invalid_instance: Option<String> = first_instance(&instances_dir.join("invalid"))?;
    let resources: Vec<Resource> = sorted_json_files(&dir.join(RESOURCES_DIR_NAME))?
        .into_iter()
        .map(load_resource)
        .collect::<Result<Vec<Resource>, FixtureError>>()?;
    Ok(Fixture {
        id,
        dir,
        schema_text,
        valid_instance,
        invalid_instance,
        resources,
    })
}

/// Load one resource document and the normalised root `$id` it is registered under.
fn load_resource(path: PathBuf) -> Result<Resource, FixtureError> {
    let text: String = read_text(&path)?;
    let invalid = |reason: String| FixtureError::InvalidResource {
        path: path.clone(),
        reason,
    };
    let document: Value = serde_json::from_str(&text)
        .map_err(|e: serde_json::Error| invalid(format!("not JSON: {e}")))?;
    let id: &str = document
        .get("$id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("no root \"$id\" string".to_owned()))?;
    let uri: Iri<String> = Iri::parse(id)
        .map(|iri: Iri<&str>| iri.normalize())
        .map_err(|e| invalid(format!("root \"$id\" {id:?} is not an absolute URI: {e}")))?;
    Ok(Resource {
        uri: document_uri(&uri),
        path,
        text,
    })
}

impl Fixture {
    /// Every `$ref` in the schema or its resources whose target document is none of the
    /// fixture's documents, as written, sorted and deduplicated. Empty when the fixture is
    /// self-contained. Documents that are not valid JSON contribute nothing.
    ///
    /// The benchmark runs offline, so a fixture with an unresolved reference is never timed: a
    /// tool would either fail to resolve it or spend the measurement fetching it.
    #[must_use]
    pub fn unresolved_refs(&self) -> Vec<String> {
        let documents: Vec<Value> = std::iter::once(self.schema_text.as_str())
            .chain(
                self.resources
                    .iter()
                    .map(|resource: &Resource| resource.text.as_str()),
            )
            .filter_map(|text: &str| serde_json::from_str::<Value>(text).ok())
            .collect();
        let borrowed: Vec<&Value> = documents.iter().collect();
        unresolved_refs(&borrowed)
    }
}

/// Every `$ref` in `documents` whose target document is none of `documents` (nor a schema
/// resource embedded in one), as written, sorted and deduplicated.
///
/// Each document's base URI is its root `$id` (or [`DEFAULT_BASE_URI`] without one), and a
/// nested `$id` sets the base for its subschema (RFC 3986 §5.2 resolution, §6.2.2
/// normalisation). A `$ref` resolves when its target URI, without fragment, is one of those
/// bases. A `$ref` that is not a URI reference never resolves. Draft-4 `id` is not read: no
/// fixture relies on it for a cross-document reference. Walks with an explicit stack.
#[must_use]
pub fn unresolved_refs(documents: &[&Value]) -> Vec<String> {
    let default_base: Iri<String> = Iri::parse(DEFAULT_BASE_URI)
        .map_or_else(|_| unreachable!(), |iri: Iri<&str>| iri.normalize());
    let mut known: BTreeSet<String> = BTreeSet::new();
    // Each `$ref` as written, with its target document URI (`None` when it does not resolve).
    let mut refs: Vec<(String, Option<String>)> = Vec::new();
    for document in documents {
        let mut stack: Vec<(&Value, Iri<String>)> = vec![(*document, default_base.clone())];
        while let Some((node, base)) = stack.pop() {
            match node {
                Value::Object(map) => {
                    let base: Iri<String> = match map.get("$id").and_then(Value::as_str) {
                        Some(id) => resolve(&base, id).unwrap_or(base),
                        None => base,
                    };
                    known.insert(document_uri(&base));
                    if let Some(Value::String(target)) = map.get("$ref") {
                        let resolved: Option<String> =
                            resolve(&base, target).map(|uri: Iri<String>| document_uri(&uri));
                        refs.push((target.clone(), resolved));
                    }
                    for value in map.values() {
                        stack.push((value, base.clone()));
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        stack.push((item, base.clone()));
                    }
                }
                _ => {}
            }
        }
    }
    refs.into_iter()
        .filter(|(_, resolved): &(String, Option<String>)| {
            resolved
                .as_ref()
                .is_none_or(|uri: &String| !known.contains(uri))
        })
        .map(|(written, _): (String, Option<String>)| written)
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

/// Resolve the URI reference `reference` against `base`, normalised; `None` when `reference` is
/// not a URI reference or cannot be resolved against `base`.
fn resolve(base: &Iri<String>, reference: &str) -> Option<Iri<String>> {
    let reference: IriRef<&str> = IriRef::parse(reference).ok()?;
    reference
        .resolve_against(&base.strip_fragment())
        .ok()
        .map(|resolved: Iri<String>| resolved.normalize())
}

/// `uri` without its fragment, as text.
fn document_uri(uri: &Iri<String>) -> String {
    uri.strip_fragment().normalize().as_str().to_owned()
}

/// Text of the first `*.json` file (sorted by file name) in `dir`; `None` when the directory
/// is absent or holds no JSON file.
fn first_instance(dir: &Path) -> Result<Option<String>, FixtureError> {
    match sorted_json_files(dir)?.first() {
        Some(path) => read_text(path).map(Some),
        None => Ok(None),
    }
}

/// Every `*.json` file in `dir`, sorted by path; empty when the directory is absent.
fn sorted_json_files(dir: &Path) -> Result<Vec<PathBuf>, FixtureError> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<PathBuf> = read_dir_paths(dir)?
        .into_iter()
        .filter(|path: &PathBuf| {
            path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("json")
        })
        .collect();
    files.sort();
    Ok(files)
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
    use super::{
        Fixture, FixtureError, Resource, SCHEMA_FILE_NAME, default_roots, discover, parse_roots,
        unresolved_refs,
    };
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
                resources: Vec::new(),
            },
            Fixture {
                id: "small/zeta".to_owned(),
                dir: root.join("small").join("zeta"),
                schema_text: "{\"z\":1}".to_owned(),
                valid_instance: None,
                invalid_instance: None,
                resources: Vec::new(),
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
    fn discover_loads_resources_sorted_under_their_normalised_ids() {
        let root: PathBuf = scratch_dir("resources");
        let dir: PathBuf = root.join("small").join("refs");
        let zeta: &str = r#"{"$id":"HTTPS://Example.COM/zeta.json#"}"#;
        let alpha: &str = r#"{"$id":"https://example.com/a/../alpha.json"}"#;
        write(&dir.join(SCHEMA_FILE_NAME), "{}");
        write(&dir.join("resources/zeta.json"), zeta);
        write(&dir.join("resources/alpha.json"), alpha);
        write(&dir.join("resources/notes.txt"), "ignored");
        let expected: Vec<Fixture> = vec![Fixture {
            id: "small/refs".to_owned(),
            dir: dir.clone(),
            schema_text: "{}".to_owned(),
            valid_instance: None,
            invalid_instance: None,
            resources: vec![
                Resource {
                    uri: "https://example.com/alpha.json".to_owned(),
                    path: dir.join("resources").join("alpha.json"),
                    text: alpha.to_owned(),
                },
                Resource {
                    uri: "https://example.com/zeta.json".to_owned(),
                    path: dir.join("resources").join("zeta.json"),
                    text: zeta.to_owned(),
                },
            ],
        }];
        let actual: Vec<Fixture> = discover(std::slice::from_ref(&root)).expect("discover");
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_rejects_resource_without_id() {
        let root: PathBuf = scratch_dir("resource-no-id");
        write(&root.join("small/a").join(SCHEMA_FILE_NAME), "{}");
        write(
            &root.join("small/a/resources/b.json"),
            r#"{"type":"string"}"#,
        );
        let expected: String = format!(
            "invalid resource {}: no root \"$id\" string",
            root.join("small/a/resources/b.json").display()
        );
        let actual: String = discover(std::slice::from_ref(&root))
            .expect_err("resource without $id")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn discover_rejects_resource_with_relative_id() {
        let root: PathBuf = scratch_dir("resource-relative-id");
        write(&root.join("small/a").join(SCHEMA_FILE_NAME), "{}");
        write(
            &root.join("small/a/resources/b.json"),
            r#"{"$id":"b.json"}"#,
        );
        let actual: Result<Vec<Fixture>, FixtureError> = discover(std::slice::from_ref(&root));
        assert!(matches!(actual, Err(FixtureError::InvalidResource { .. })));
    }

    #[test]
    fn discover_rejects_resource_that_is_not_json() {
        let root: PathBuf = scratch_dir("resource-not-json");
        write(&root.join("small/a").join(SCHEMA_FILE_NAME), "{}");
        write(&root.join("small/a/resources/b.json"), "{");
        let actual: Result<Vec<Fixture>, FixtureError> = discover(std::slice::from_ref(&root));
        assert!(matches!(actual, Err(FixtureError::InvalidResource { .. })));
    }

    #[test]
    fn unresolved_refs_lists_refs_to_documents_not_given_sorted_and_deduplicated() {
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
        let actual: Vec<String> = unresolved_refs(&[&schema]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolved_refs_is_empty_for_a_self_contained_schema() {
        let schema: serde_json::Value = serde_json::json!({
            "$defs": { "a": { "type": "string" } },
            "properties": { "local": { "$ref": "#/$defs/a" } }
        });
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = unresolved_refs(&[&schema]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolved_refs_resolves_relative_refs_against_the_root_id_to_given_documents() {
        let schema: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/root.json",
            "properties": {
                "a": { "$ref": "a.json" },
                "b": { "$ref": "HTTPS://example.com/b.json#/definitions/x" }
            }
        });
        let a: serde_json::Value = serde_json::json!({ "$id": "https://example.com/a.json" });
        let b: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/b.json",
            "definitions": { "x": { "type": "string" } }
        });
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = unresolved_refs(&[&schema, &a, &b]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolved_refs_follows_refs_inside_given_documents() {
        let schema: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/root.json",
            "$ref": "a.json"
        });
        let a: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/a.json",
            "$ref": "missing.json"
        });
        let expected: Vec<String> = vec!["missing.json".to_owned()];
        let actual: Vec<String> = unresolved_refs(&[&schema, &a]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolved_refs_resolves_refs_to_embedded_resources_against_nested_ids() {
        let schema: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/root.json",
            "$defs": {
                "inner": {
                    "$id": "nested/inner.json",
                    "properties": { "sibling": { "$ref": "sibling.json" } }
                },
                "sibling": { "$id": "nested/sibling.json" }
            },
            "$ref": "nested/inner.json"
        });
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = unresolved_refs(&[&schema]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolved_refs_keeps_a_relative_ref_from_a_schema_without_id() {
        let schema: serde_json::Value = serde_json::json!({ "$ref": "a.json" });
        let a: serde_json::Value = serde_json::json!({ "$id": "https://example.com/a.json" });
        let expected: Vec<String> = vec!["a.json".to_owned()];
        let actual: Vec<String> = unresolved_refs(&[&schema, &a]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn committed_fixtures_resolve_every_ref() {
        let committed: PathBuf = default_roots().into_iter().next().expect("committed root");
        let expected: Vec<(String, Vec<String>)> = Vec::new();
        let actual: Vec<(String, Vec<String>)> = discover(&[committed])
            .expect("discover")
            .into_iter()
            .map(|fixture: Fixture| {
                let unresolved: Vec<String> = fixture.unresolved_refs();
                (fixture.id, unresolved)
            })
            .filter(|(_, unresolved): &(String, Vec<String>)| !unresolved.is_empty())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn committed_package_json_fixture_vendors_every_referenced_document() {
        let committed: PathBuf = default_roots().into_iter().next().expect("committed root");
        let expected: Vec<String> = vec![
            "https://json.schemastore.org/ava.json".to_owned(),
            "https://json.schemastore.org/eslintrc.json".to_owned(),
            "https://json.schemastore.org/jscpd.json".to_owned(),
            "https://json.schemastore.org/nodemon.json".to_owned(),
            "https://json.schemastore.org/partial-eslint-plugins.json".to_owned(),
            "https://www.schemastore.org/prettierrc.json".to_owned(),
            "https://www.schemastore.org/quikrun.json".to_owned(),
            "https://json.schemastore.org/semantic-release.json".to_owned(),
            "https://json.schemastore.org/stylelintrc.json".to_owned(),
        ];
        let actual: Vec<String> = discover(&[committed])
            .expect("discover")
            .into_iter()
            .find(|fixture: &Fixture| fixture.id == "medium/package-json")
            .expect("package-json fixture")
            .resources
            .into_iter()
            .map(|resource: Resource| resource.uri)
            .collect();
        assert_eq!(expected, actual);
    }
}
