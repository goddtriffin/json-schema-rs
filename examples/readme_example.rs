//! Full-featured example matching the README: every supported schema feature.
//! See [README Examples](../README.md#examples).
//!
//! Contains only the input JSON Schema and the conversion logic; generated
//! Rust is written to stdout.

use std::io;

const SCHEMA_JSON: &str = r#"{
  "type": "object",
  "title": "Record",
  "description": "A record with id and optional fields.",
  "required": ["id"],
  "additionalProperties": { "type": "string" },
  "properties": {
    "active": { "type": "boolean" },
    "count": { "type": "integer", "minimum": 0, "maximum": 255 },
    "id": { "type": "string", "format": "uuid", "description": "Unique identifier." },
    "name": { "type": "string" },
    "score": { "type": "number", "minimum": 0, "maximum": 1 },
    "status": { "type": "string", "enum": ["active", "inactive"], "default": "active", "description": "Current status." },
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
    let settings = json_schema_rs::GenerateSettings::default();
    json_schema_rs::generate_to_writer(SCHEMA_JSON, &mut stdout, &settings)?;
    Ok(())
}
