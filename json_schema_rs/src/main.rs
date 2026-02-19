//! CLI for json-schema-rs: generate Rust from JSON Schema, validate JSON against a schema.

use clap::{Arg, Command};
use json_schema_rs::{JsonSchema, generate_rust, validate};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;

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

fn run_generate_rust(output: Option<PathBuf>) -> Result<(), String> {
    let schema: JsonSchema = read_schema_from_reader(io::stdin())?;
    if let Some(p) = output {
        let mut f = File::create(&p).map_err(|e| format!("failed to create output file: {e}"))?;
        generate_rust(&schema, &mut f).map_err(|e| e.to_string())?;
        f.flush()
            .map_err(|e| format!("failed to flush output: {e}"))?;
    } else {
        let mut out = io::stdout();
        generate_rust(&schema, &mut out).map_err(|e| e.to_string())?;
        out.flush()
            .map_err(|e| format!("failed to flush output: {e}"))?;
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
                .about("Generate code from a JSON Schema")
                .subcommand(
                    Command::new("rust")
                        .about("Generate Rust structs from a JSON Schema (schema from stdin, output to stdout or -o file)")
                        .arg(
                            Arg::new("output")
                                .short('o')
                                .long("output")
                                .value_name("FILE")
                                .help("Write output to FILE instead of stdout"),
                        ),
                ),
        )
        .subcommand(
            Command::new("validate")
                .about("Validate a JSON instance against a JSON Schema")
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
            if let Some(("rust", rust_m)) = gen_m.subcommand() {
                let output: Option<PathBuf> = rust_m
                    .get_one::<String>("output")
                    .map(|s| PathBuf::from(s.as_str()));
                run_generate_rust(output)
            } else {
                eprintln!("expected subcommand: rust");
                std::process::exit(1);
            }
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
