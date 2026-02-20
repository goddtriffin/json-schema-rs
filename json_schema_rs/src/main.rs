//! CLI for json-schema-rs: generate Rust from JSON Schema, validate JSON against a schema.

use clap::{Arg, Command};
use json_schema_rs::{JsonSchema, generate_rust, validate};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const STDIN_OUTPUT_NAME: &str = "stdin.rs";

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
                let rs_relative = relative.with_extension("rs");
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
                let rs: PathBuf = if path.is_absolute() {
                    path.file_name().map_or_else(
                        || PathBuf::from("schema.rs"),
                        |n| PathBuf::from(n).with_extension("rs"),
                    )
                } else {
                    path.with_extension("rs")
                };
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
    let mut schemas: Vec<JsonSchema> = Vec::with_capacity(entries.len());
    for (input_path, _) in &entries {
        let schema = read_schema_from_path(input_path)?;
        schemas.push(schema);
    }
    let bytes_list = generate_rust(&schemas).map_err(|e| e.to_string())?;
    assert_eq!(bytes_list.len(), entries.len(), "codegen output count");
    for ((_, output_relative), bytes) in entries.into_iter().zip(bytes_list) {
        let out_path = output_dir.join(&output_relative);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
        let mut f = File::create(&out_path)
            .map_err(|e| format!("failed to create {}: {e}", out_path.display()))?;
        f.write_all(&bytes)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", out_path.display()))?;
    }
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
                        .help("Output directory; generated .rs files mirror input paths"),
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
