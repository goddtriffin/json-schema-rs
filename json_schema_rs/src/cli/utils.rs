//! CLI helpers: schema/payload I/O, schema file discovery, mod.rs emission.

use json_schema_rs::sanitizers::{sanitize_output_relative, sanitize_path_component};
use json_schema_rs::{DialectWarning, JsonSchemaSettings, Schema, resolve_dialect};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub(crate) const STDIN_OUTPUT_NAME: &str = "stdin.rs";

pub(crate) fn read_schema_from_reader<R: Read>(
    mut r: R,
    schema_settings: &JsonSchemaSettings,
) -> Result<Schema, String> {
    let mut buf: Vec<u8> = Vec::new();
    r.read_to_end(&mut buf)
        .map_err(|e| format!("failed to read schema: {e}"))?;
    Schema::new_from_slice(&buf, schema_settings).map_err(|e| e.to_string())
}

/// Reads and parses a schema document (`-` is stdin), which may be a boolean schema, printing
/// its dialect warnings to stderr.
pub(crate) fn read_schema_from_path(
    path: &PathBuf,
    schema_settings: &JsonSchemaSettings,
) -> Result<Schema, String> {
    let schema: Schema = if path.as_os_str() == "-" {
        read_schema_from_reader(io::stdin(), schema_settings)?
    } else {
        let f = File::open(path).map_err(|e| format!("failed to open schema file: {e}"))?;
        read_schema_from_reader(f, schema_settings)?
    };
    if let Some(object) = schema.as_object() {
        let warnings: Vec<DialectWarning> = resolve_dialect(object, schema_settings)
            .map_err(|e| e.to_string())?
            .warnings;
        for warning in warnings {
            eprintln!("warning: {}: {warning}", display_schema_path(path));
        }
    }
    Ok(schema)
}

/// How a schema path is shown in messages (`stdin` for `-`).
pub(crate) fn display_schema_path(path: &Path) -> String {
    if path.as_os_str() == "-" {
        "stdin".to_string()
    } else {
        path.display().to_string()
    }
}

pub(crate) fn read_payload_from_reader<R: Read>(mut r: R) -> Result<serde_json::Value, String> {
    let mut buf: Vec<u8> = Vec::new();
    r.read_to_end(&mut buf)
        .map_err(|e| format!("failed to read payload: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("invalid JSON payload: {e}"))
}

pub(crate) fn read_payload_from_path(path: &PathBuf) -> Result<serde_json::Value, String> {
    let f = File::open(path).map_err(|e| format!("failed to open payload file: {e}"))?;
    read_payload_from_reader(f)
}

/// Recursively find all `.json` files under `dir`. Returns (`input_path`, `output_relative`) for each,
/// where `output_relative` is the path under the output dir with `.rs` extension (e.g. `a/b/c.json` -> `a/b/c.rs`).
/// Uses an explicit stack to avoid recursion.
fn find_schema_files_under(dir: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut out: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries = fs::read_dir(&current)
            .map_err(|e| format!("failed to read dir {}: {e}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read entry: {e}"))?;
            let path: PathBuf = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                let relative = path
                    .strip_prefix(dir)
                    .map_err(|e| format!("schema path not under input dir: {e}"))?;
                let rs_relative = sanitize_output_relative(relative);
                out.push((path, rs_relative.clone()));
            }
        }
    }
    Ok(out)
}

/// Expand INPUTs (file paths, dir paths, or "-") into a deduplicated list of (`input_path`, `output_relative`).
/// For "-", `input_path` is `PathBuf::from("-")` and `output_relative` is `STDIN_OUTPUT_NAME`.
pub(crate) fn collect_schema_entries(inputs: &[String]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut entries: Vec<(PathBuf, PathBuf)> = Vec::new();
    for input in inputs {
        let path = PathBuf::from(input);
        if path.as_os_str() == "-" {
            entries.push((PathBuf::from("-"), PathBuf::from(STDIN_OUTPUT_NAME)));
            continue;
        }
        if path.is_file() {
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if seen.insert(canonical.clone()) {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("schema");
                let rs = PathBuf::from(format!("{}.rs", sanitize_path_component(stem)));
                entries.push((canonical, rs));
            }
        } else if path.is_dir() {
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let files = find_schema_files_under(&canonical)?;
            for (file_path, relative_rs) in files {
                let canonical_file = file_path
                    .canonicalize()
                    .map_err(|e| format!("{}: {e}", file_path.display()))?;
                if seen.insert(canonical_file.clone()) {
                    entries.push((canonical_file, relative_rs));
                }
            }
        } else {
            return Err(format!("not a file or directory: {}", path.display()));
        }
    }
    Ok(entries)
}

/// Builds a map: directory path (relative to `output_dir`) -> set of module names (direct
/// children: file stems and subdirectory names). Root is `""`. Any depth; iterative.
pub(crate) fn mod_rs_content_by_dir(
    output_relatives: &[PathBuf],
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut by_dir: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for rel in output_relatives {
        let components: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        let Some((file, dirs)) = components.split_last() else {
            continue;
        };
        let module_name: String = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("schema")
            .to_string();
        let mut dir: PathBuf = PathBuf::new();
        for sub in dirs {
            by_dir.entry(dir.clone()).or_default().insert(sub.clone());
            dir.push(sub);
        }
        by_dir.entry(dir).or_default().insert(module_name);
    }
    by_dir
}

/// Writes a `mod.rs` in `output_dir` and in every subdirectory, declaring its modules. When
/// `has_shared`, each subdirectory's `mod.rs` also re-exports its parent's `shared` module, so
/// generated files at any depth reach the shared items as `super::shared::Name`.
pub(crate) fn write_mod_rs_files(
    output_dir: &Path,
    output_relatives: &[PathBuf],
    has_shared: bool,
) -> Result<(), String> {
    let by_dir = mod_rs_content_by_dir(output_relatives);
    for (dir_rel, modules) in by_dir {
        let is_root: bool = dir_rel.as_os_str().is_empty();
        let mod_rs_path = if is_root {
            output_dir.join("mod.rs")
        } else {
            output_dir.join(&dir_rel).join("mod.rs")
        };
        if let Some(parent) = mod_rs_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
        let mut content: String = String::new();
        for name in &modules {
            content.push_str("pub mod ");
            content.push_str(name);
            content.push_str(";\n");
        }
        if has_shared && !is_root {
            content.push_str("\npub use super::shared;\n");
        }
        let mut f = File::create(&mod_rs_path)
            .map_err(|e| format!("failed to create {}: {e}", mod_rs_path.display()))?;
        f.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {e}", mod_rs_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", mod_rs_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_rs_content_by_dir_single_root_file() {
        let paths = vec![PathBuf::from("a.rs")];
        let actual = mod_rs_content_by_dir(&paths);
        let mut expected: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
        expected
            .entry(PathBuf::new())
            .or_default()
            .insert("a".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn mod_rs_content_by_dir_root_and_subdir() {
        let paths = vec![
            PathBuf::from("schema_1.rs"),
            PathBuf::from("sub_dir/schema_2.rs"),
        ];
        let actual: BTreeMap<PathBuf, BTreeSet<String>> = mod_rs_content_by_dir(&paths);
        let expected: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::from([
            (
                PathBuf::new(),
                BTreeSet::from(["schema_1".to_string(), "sub_dir".to_string()]),
            ),
            (
                PathBuf::from("sub_dir"),
                BTreeSet::from(["schema_2".to_string()]),
            ),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn mod_rs_content_by_dir_two_levels_deep() {
        let paths: Vec<PathBuf> = vec![PathBuf::from("a/b/c.rs")];
        let actual: BTreeMap<PathBuf, BTreeSet<String>> = mod_rs_content_by_dir(&paths);
        let expected: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::from([
            (PathBuf::new(), BTreeSet::from(["a".to_string()])),
            (PathBuf::from("a"), BTreeSet::from(["b".to_string()])),
            (PathBuf::from("a/b"), BTreeSet::from(["c".to_string()])),
        ]);
        assert_eq!(expected, actual);
    }
}
