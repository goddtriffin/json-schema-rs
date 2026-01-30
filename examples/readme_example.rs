//! Full-featured example matching the README: every supported schema feature.
//! See [README Examples](../README.md#examples).
//!
//! Contains only the input JSON Schema and the conversion logic; generated
//! Rust is written to stdout.

use std::io;

const SCHEMA_JSON: &str = r#"{
  "type": "object",
  "title": "Record",
  "required": ["id"],
  "properties": {
    "active": { "type": "boolean" },
    "count": { "type": "integer" },
    "id": { "type": "string" },
    "name": { "type": "string" },
    "score": { "type": "number" },
    "status": { "type": "string", "enum": ["active", "inactive"] },
    "nested": {
      "type": "object",
      "title": "NestedInfo",
      "required": ["value"],
      "properties": {
        "value": { "type": "string" },
        "kind": { "type": "string", "enum": ["A", "a"] }
      }
    },
    "foo-bar": { "type": "string" },
    "tags": { "type": "array", "items": { "type": "string" } }
  }
}"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout: io::Stdout = io::stdout();
    json_schema_rs::generate_to_writer(SCHEMA_JSON, &mut stdout)?;
    Ok(())
}
