//! Parse a JSON Schema and print generated Rust to stdout.

use json_schema_rs::{JsonSchema, generate_rust};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "email": { "type": "string" }
        }
    }"#;
    let schema: JsonSchema = serde_json::from_str(json)?;
    let bytes = generate_rust(&[schema])?;
    io::stdout().write_all(&bytes[0])?;
    Ok(())
}
