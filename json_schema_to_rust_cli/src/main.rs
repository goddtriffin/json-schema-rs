//! CLI: read JSON Schema from stdin, write generated Rust to stdout.

use json_schema_rs::{Schema, generate_rust};
use std::io::{self, Read, Write};

fn main() {
    let mut input: Vec<u8> = Vec::new();
    if io::stdin().read_to_end(&mut input).is_err() {
        eprintln!("error: failed to read stdin");
        std::process::exit(1);
    }
    let schema: Schema = match serde_json::from_slice(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: invalid JSON Schema: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = generate_rust(&schema, &mut io::stdout()) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    if io::stdout().flush().is_err() {
        std::process::exit(1);
    }
}
