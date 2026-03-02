//! Parse a JSON Schema and print generated Rust to stdout.

use json_schema_rs::{CodeGenSettings, JsonSchema, generate_rust};
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = r#"{
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "email": { "type": "string" }
        }
    }"#;
    let schema = JsonSchema::try_from(json)?;
    let code_gen_settings: CodeGenSettings = CodeGenSettings::builder().build();
    let output = generate_rust(&[schema], &code_gen_settings)?;
    io::stdout().write_all(&output.per_schema[0])?;
    Ok(())
}
