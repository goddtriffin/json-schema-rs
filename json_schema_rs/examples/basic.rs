//! Parse a JSON Schema and print generated Rust to stdout.

use json_schema_rs::{JsonSchema, generate_rust};
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "email": { "type": "string" }
        }
    }"#;
    let schema: JsonSchema = serde_json::from_str(json)?;
    generate_rust(&schema, &mut io::stdout())?;
    Ok(())
}
