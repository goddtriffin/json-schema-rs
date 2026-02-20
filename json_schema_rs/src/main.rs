//! CLI for json-schema-rs: generate Rust from JSON Schema, validate JSON against a schema.

use clap::{Arg, Command};
use json_schema_rs::{JsonSchema, generate_rust, validate};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const STDIN_OUTPUT_NAME: &str = "stdin.rs";

/// Maps a path component (file stem or dir name) to a Rust-valid identifier.
/// Replaces `-` with `_` and any character not in `[a-zA-Z0-9_]` with `_`.
/// If the result is empty or starts with a digit, returns a valid fallback.
fn sanitize_path_component(component: &str) -> String {
    let s: String = component
        .chars()
        .map(|c| {
            if c == '-' || c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .replace('-', "_");
    if s.is_empty() {
        return "schema".to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("_{s}");
    }
    s
}

/// Builds the sanitized output relative path (e.g. `sub_dir/schema_2.rs`) from a relative path (e.g. `sub-dir/schema-2.json`).
fn sanitize_output_relative(relative: &Path) -> PathBuf {
    let components: Vec<_> = relative.components().collect();
    let mut out = PathBuf::new();
    for (i, comp) in components.iter().enumerate() {
        let os = comp.as_os_str();
        let s = os.to_string_lossy();
        let is_last = i == components.len() - 1;
        if is_last {
            let stem = Path::new(s.as_ref())
                .file_stem()
                .and_then(|st| st.to_str())
                .unwrap_or("schema");
            let name = format!("{}.rs", sanitize_path_component(stem));
            out.push(name);
        } else {
            out.push(sanitize_path_component(s.as_ref()));
        }
    }
    out
}

fn read_schema_from_reader<R: Read>(mut r: R) -> Result<JsonSchema, String> {
    let mut buf: Vec<u8> = Vec::new();
    r.read_to_end(&mut buf)
        .map_err(|e| format!("failed to read schema: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("invalid JSON Schema: {e}"))
}

fn read_schema_from_path(path: &PathBuf) -> Result<JsonSchema, String> {
    if path.as_os_str() == "-" {
        read_schema_from_reader(io::stdin())
    } else {
        let f = File::open(path).map_err(|e| format!("failed to open schema file: {e}"))?;
        read_schema_from_reader(f)
    }
}

fn read_payload_from_reader<R: Read>(mut r: R) -> Result<serde_json::Value, String> {
    let mut buf: Vec<u8> = Vec::new();
    r.read_to_end(&mut buf)
        .map_err(|e| format!("failed to read payload: {e}"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("invalid JSON payload: {e}"))
}

fn read_payload_from_path(path: &PathBuf) -> Result<serde_json::Value, String> {
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
fn collect_schema_entries(inputs: &[String]) -> Result<Vec<(PathBuf, PathBuf)>, String> {
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

/// Builds a map: directory path (relative to `output_dir`) -> set of module names (direct children).
/// Root is ""; subdirs are e.g. `sub_dir`. Uses iterative logic (no recursion).
fn mod_rs_content_by_dir(output_relatives: &[PathBuf]) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut by_dir: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
    for rel in output_relatives {
        let path = Path::new(rel);
        let components: Vec<_> = path.components().collect();
        if components.is_empty() {
            continue;
        }
        let module_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("schema")
            .to_string();
        if components.len() == 1 {
            by_dir
                .entry(PathBuf::new())
                .or_default()
                .insert(module_name);
        } else {
            let subdir_name = components[0].as_os_str().to_string_lossy().to_string();
            by_dir
                .entry(PathBuf::new())
                .or_default()
                .insert(subdir_name.clone());
            by_dir
                .entry(PathBuf::from(&subdir_name))
                .or_default()
                .insert(module_name);
        }
    }
    by_dir
}

/// Writes a `mod.rs` in each output directory that has generated .rs files or subdirs.
fn write_mod_rs_files(output_dir: &Path, output_relatives: &[PathBuf]) -> Result<(), String> {
    let by_dir = mod_rs_content_by_dir(output_relatives);
    for (dir_rel, modules) in by_dir {
        let mod_rs_path = if dir_rel.as_os_str().is_empty() {
            output_dir.join("mod.rs")
        } else {
            output_dir.join(&dir_rel).join("mod.rs")
        };
        if let Some(parent) = mod_rs_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
        let content: String = modules
            .iter()
            .map(|name| format!("pub mod {name};"))
            .collect::<Vec<_>>()
            .join("\n");
        let content = content + "\n";
        let mut f = File::create(&mod_rs_path)
            .map_err(|e| format!("failed to create {}: {e}", mod_rs_path.display()))?;
        f.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {e}", mod_rs_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", mod_rs_path.display()))?;
    }
    Ok(())
}

fn run_generate(lang: &str, inputs: &[String], output_dir: &Path) -> Result<(), String> {
    if !lang.eq_ignore_ascii_case("rust") {
        return Err(format!("unsupported language: {lang}; supported: rust"));
    }
    if inputs.is_empty() {
        return Err(
            "at least one INPUT (file, directory, or \"-\" for stdin) is required".to_string(),
        );
    }
    let entries = collect_schema_entries(inputs)?;
    if entries.is_empty() {
        return Err("no JSON Schema files found (look for .json in directories)".to_string());
    }

    // Standalone ingestion step: try every file, log each failure to stderr, do not short-circuit.
    let mut successful: Vec<(JsonSchema, PathBuf)> = Vec::with_capacity(entries.len());
    let mut had_errors = false;
    for (input_path, output_relative) in &entries {
        match read_schema_from_path(input_path) {
            Ok(schema) => successful.push((schema, output_relative.clone())),
            Err(e) => {
                let path_display = if input_path.as_os_str() == "-" {
                    "stdin".to_string()
                } else {
                    input_path.display().to_string()
                };
                eprintln!("error: {path_display}: {e}");
                had_errors = true;
            }
        }
    }
    if had_errors {
        let count = entries.len() - successful.len();
        return Err(format!(
            "schema ingestion failed with {count} error(s); no output written"
        ));
    }

    let (schemas, output_relatives): (Vec<JsonSchema>, Vec<PathBuf>) =
        successful.into_iter().unzip();
    let bytes_list = generate_rust(&schemas).map_err(|e| e.to_string())?;
    assert_eq!(
        bytes_list.len(),
        output_relatives.len(),
        "codegen output count"
    );

    for (output_relative, bytes) in output_relatives.iter().zip(bytes_list.iter()) {
        let out_path = output_dir.join(output_relative);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
        let mut f = File::create(&out_path)
            .map_err(|e| format!("failed to create {}: {e}", out_path.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", out_path.display()))?;
    }

    write_mod_rs_files(output_dir, &output_relatives)?;
    Ok(())
}

fn run_validate(schema_path: &PathBuf, payload_path: Option<PathBuf>) -> Result<(), String> {
    let schema: JsonSchema = read_schema_from_path(schema_path)?;
    let instance: serde_json::Value = match payload_path {
        Some(p) => read_payload_from_path(&p)?,
        None => read_payload_from_reader(io::stdin())?,
    };
    match validate(&schema, &instance) {
        Ok(()) => Ok(()),
        Err(errors) => {
            for e in &errors {
                eprintln!("{e}");
            }
            Err(format!("validation failed with {} error(s)", errors.len()))
        }
    }
}

fn main() {
    let cmd = Command::new("jsonschemars")
        .about("JSON Schema tooling: generate Rust types, validate JSON")
        .subcommand(
            Command::new("generate")
                .about("Generate Rust from one or more JSON Schema files or directories")
                .arg(
                    Arg::new("lang")
                        .required(true)
                        .value_name("LANG")
                        .help("Target language (e.g. rust)"),
                )
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .value_name("DIR")
                        .required(true)
                        .help("Output directory; generated .rs files use sanitized paths and each dir has a mod.rs"),
                )
                .arg(
                    Arg::new("inputs")
                        .required(true)
                        .value_name("INPUT")
                        .num_args(1..)
                        .help("JSON Schema file(s), directory(ies) to search for .json, or \"-\" for stdin"),
                ),
        )
        .subcommand(
            Command::new("validate")
                .about("Validate a JSON instance against a JSON Schema (one schema, one payload)")
                .arg(
                    Arg::new("schema")
                        .short('s')
                        .long("schema")
                        .value_name("FILE")
                        .required(true)
                        .help("Path to the JSON Schema file. Use \"-\" for stdin."),
                )
                .arg(
                    Arg::new("payload")
                        .short('p')
                        .long("payload")
                        .value_name("FILE")
                        .help("Path to the JSON payload to validate. If omitted, read from stdin."),
                ),
        );
    let matches = cmd.get_matches();

    let result = match matches.subcommand() {
        Some(("generate", gen_m)) => {
            let lang: &str = gen_m
                .get_one::<String>("lang")
                .map(String::as_str)
                .expect("required LANG");
            let output_dir: PathBuf = gen_m
                .get_one::<String>("output")
                .map(|s| PathBuf::from(s.as_str()))
                .expect("required --output");
            let inputs: Vec<String> = gen_m
                .get_many::<String>("inputs")
                .map(|it| it.map(String::from).collect())
                .unwrap_or_default();
            run_generate(lang, &inputs, &output_dir)
        }
        Some(("validate", val_m)) => {
            let schema: PathBuf = val_m
                .get_one::<String>("schema")
                .map(|s| PathBuf::from(s.as_str()))
                .expect("required --schema");
            let payload: Option<PathBuf> = val_m
                .get_one::<String>("payload")
                .map(|s| PathBuf::from(s.as_str()));
            run_validate(&schema, payload)
        }
        _ => {
            eprintln!("expected subcommand: generate or validate");
            std::process::exit(1);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sanitize_path_component_hyphen_to_underscore() {
        let expected = "schema_1";
        let actual = sanitize_path_component("schema-1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_unchanged_valid() {
        let expected = "sub_dir";
        let actual = sanitize_path_component("sub_dir");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_empty_fallback() {
        let expected = "schema";
        let actual = sanitize_path_component("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_only_hyphens_becomes_underscores() {
        let expected = "___";
        let actual = sanitize_path_component("---");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_path_component_starts_with_digit_prefixed() {
        let expected = "_123";
        let actual = sanitize_path_component("123");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_output_relative_single_file() {
        let relative = Path::new("schema-1.json");
        let actual = sanitize_output_relative(relative);
        let expected = PathBuf::from("schema_1.rs");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_output_relative_nested() {
        let relative = Path::new("sub-dir/schema-2.json");
        let actual = sanitize_output_relative(relative);
        let expected = PathBuf::from("sub_dir/schema_2.rs");
        assert_eq!(expected, actual);
    }

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
        let actual = mod_rs_content_by_dir(&paths);
        let root_modules = actual.get(&PathBuf::new()).expect("root entry");
        assert!(root_modules.contains("schema_1"));
        assert!(root_modules.contains("sub_dir"));
        let sub_modules = actual
            .get(&PathBuf::from("sub_dir"))
            .expect("sub_dir entry");
        assert!(sub_modules.contains("schema_2"));
    }
}
