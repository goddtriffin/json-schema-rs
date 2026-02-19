//! Integration test: public API for schema parsing and code generation.

use json_schema_rs::{Schema, generate_rust};
use std::io::Cursor;

#[test]
fn integration_parse_and_generate() {
    let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
    let schema: Schema = serde_json::from_str(json).expect("parse schema");
    let mut out = Cursor::new(Vec::new());
    generate_rust(&schema, &mut out).expect("generate");
    let rust = String::from_utf8(out.into_inner()).expect("utf8");
    assert!(rust.contains("pub struct Root"));
    assert!(rust.contains("pub id: String,"));
}
