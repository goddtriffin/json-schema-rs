//! Integration test: a crate that invokes `json_schema_to_rust!` is rebuilt when a schema file
//! the macro read changes, both a path input and a file one of its `$ref`s names.
//!
//! A temp crate is built and run after each edit; its program reports whether one fixed
//! instance deserializes into the generated type, which every edit flips.

use std::path::{Path, PathBuf};
use std::process::Command;

const ROOT_OPEN: &str =
    r#"{"title":"Root","type":"object","properties":{"a":{"$ref":"child.json"}},"required":["a"]}"#;
const ROOT_REQUIRES_B: &str = r#"{"title":"Root","type":"object","properties":{"a":{"$ref":"child.json"},"b":{"type":"string"}},"required":["a","b"]}"#;
const CHILD_STRING: &str =
    r#"{"title":"Child","type":"object","properties":{"x":{"type":"string"}},"required":["x"]}"#;
const CHILD_INTEGER: &str =
    r#"{"title":"Child","type":"object","properties":{"x":{"type":"integer"}},"required":["x"]}"#;

const MAIN_RS: &str = r##"json_schema_rs_macro::json_schema_to_rust!("schemas/root.json");

fn main() {
    let parsed: Result<root::Root, serde_json::Error> = serde_json::from_str(r#"{"a":{"x":"s"}}"#);
    print!("{}", if parsed.is_ok() { "ok" } else { "err" });
}
"##;

/// A target directory shared across runs so the path dependencies are built once.
fn target_dir() -> PathBuf {
    let manifest_dir: String =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo");
    let target: PathBuf = std::env::var("CARGO_TARGET_DIR").map_or_else(
        |_| PathBuf::from(&manifest_dir).join("..").join("target"),
        PathBuf::from,
    );
    target.join("macro_rebuild")
}

fn cargo_toml() -> String {
    let manifest_dir: String =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo");
    let macro_path: &Path = Path::new(&manifest_dir);
    let library_path: PathBuf = macro_path
        .parent()
        .expect("workspace root")
        .join("json_schema_rs");
    format!(
        r#"[package]
name = "macro_rebuild"
version = "0.0.1"
edition = "2024"

[workspace]

[dependencies]
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
json-schema-rs = {{ path = {library:?} }}
json-schema-rs-macro = {{ path = {macro_crate:?} }}
"#,
        library = library_path.display().to_string(),
        macro_crate = macro_path.display().to_string(),
    )
}

fn cargo_run(crate_dir: &Path) -> String {
    let run = Command::new("cargo")
        .args(["run", "--quiet"])
        .current_dir(crate_dir)
        .env("CARGO_TARGET_DIR", target_dir())
        .output()
        .expect("run cargo run");
    assert!(
        run.status.success(),
        "cargo run failed: stderr={}",
        String::from_utf8_lossy(&run.stderr)
    );
    String::from_utf8(run.stdout).expect("stdout is UTF-8")
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write file");
}

#[test]
fn editing_input_or_referenced_schema_file_rebuilds() {
    let crate_dir: tempfile::TempDir = tempfile::tempdir().expect("temp dir");
    let dir: &Path = crate_dir.path();
    std::fs::create_dir_all(dir.join("src")).expect("create src");
    std::fs::create_dir_all(dir.join("schemas")).expect("create schemas");
    write(&dir.join("Cargo.toml"), &cargo_toml());
    write(&dir.join("src/main.rs"), MAIN_RS);
    let root: PathBuf = dir.join("schemas/root.json");
    let child: PathBuf = dir.join("schemas/child.json");
    write(&root, ROOT_OPEN);
    write(&child, CHILD_STRING);

    let mut actual: Vec<String> = vec![cargo_run(dir)];
    // The `$ref`'d file changes: `x` must now be an integer.
    write(&child, CHILD_INTEGER);
    actual.push(cargo_run(dir));
    // And changes back.
    write(&child, CHILD_STRING);
    actual.push(cargo_run(dir));
    // The input file changes: `b` is now required.
    write(&root, ROOT_REQUIRES_B);
    actual.push(cargo_run(dir));

    let expected: Vec<String> = ["ok", "err", "ok", "err"].map(String::from).to_vec();
    assert_eq!(expected, actual);
}
