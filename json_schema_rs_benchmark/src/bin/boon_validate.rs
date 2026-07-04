//! Thin CLI wrapper around the `boon` validator so Hyperfine can time it uniformly with the
//! other tools. Mirrors `jsonschemars validate`: takes a schema file and a payload file,
//! compiles the schema, validates the payload, and exits 0 on valid / non-zero on invalid.
//!
//! OWNER: competitor-wiring leaf.
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.
//!
//! boon 0.6.1 API used (see docs.rs/boon/0.6.1):
//!   - `Schemas::new()`               -> collection of compiled schemas.
//!   - `Compiler::new()`              -> schema compiler.
//!   - `Compiler::add_resource(loc, Value)` -> register a parsed schema under a URL so the
//!     compiler never touches the filesystem or network (keeps the benchmark hermetic).
//!   - `Compiler::compile(loc, &mut Schemas) -> Result<SchemaIndex, CompileError>`.
//!   - `Schemas::validate(&Value, SchemaIndex) -> Result<(), ValidationError>`.
//!
//! If the schema has no `$schema`, boon assumes the latest draft.

use boon::{Compiler, Schemas};
use clap::{Arg, Command};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A synthetic URL under which the schema document is registered with boon. Using a fixed
/// in-memory resource loc avoids file-URL / working-directory portability issues and keeps
/// the compiler from hitting the filesystem for the root document.
const SCHEMA_LOC: &str = "mem://benchmark/schema.json";

fn main() -> ExitCode {
    let matches = Command::new("boon_validate")
        .about("Validate a JSON payload against a schema via boon")
        .arg(
            Arg::new("schema")
                .short('s')
                .long("schema")
                .value_name("FILE")
                .required(true)
                .help("Path to the JSON Schema file"),
        )
        .arg(
            Arg::new("payload")
                .short('p')
                .long("payload")
                .value_name("FILE")
                .required(true)
                .help("Path to the JSON payload to validate"),
        )
        .get_matches();

    let schema: PathBuf = matches
        .get_one::<String>("schema")
        .map(PathBuf::from)
        .expect("required --schema");
    let payload: PathBuf = matches
        .get_one::<String>("payload")
        .map(PathBuf::from)
        .expect("required --payload");

    match run(&schema, &payload) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("boon_validate: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Read + parse a JSON document from `path`, mapping I/O and parse failures to a message.
fn read_json(path: &Path) -> Result<Value, String> {
    let file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("cannot parse {}: {e}", path.display()))
}

/// Compile `schema_path` with boon and validate `payload_path` against it.
///
/// Returns `Ok(())` when the payload is valid; an `Err` message otherwise (compile error,
/// I/O error, or a validation failure). The caller maps `Err` to a non-zero exit code.
fn run(schema_path: &Path, payload_path: &Path) -> Result<(), String> {
    let schema_doc: Value = read_json(schema_path)?;
    let instance: Value = read_json(payload_path)?;

    let mut schemas = Schemas::new();
    let mut compiler = Compiler::new();
    compiler
        .add_resource(SCHEMA_LOC, schema_doc)
        .map_err(|e| format!("cannot register schema: {e}"))?;
    let sch_index = compiler
        .compile(SCHEMA_LOC, &mut schemas)
        .map_err(|e| format!("cannot compile schema: {e}"))?;

    schemas
        .validate(&instance, sch_index)
        .map_err(|e| format!("invalid: {e}"))
}
