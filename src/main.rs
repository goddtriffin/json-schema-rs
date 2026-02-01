//! Binary to generate Rust structs from a JSON Schema.
//!
//! Usage: `json-schema-gen < input.json > output.rs`
//!
//! Reads a JSON Schema from stdin and writes generated Rust structs to stdout.

use std::io::{read_to_string, stdin, stdout};
use std::process;

use json_schema_rs::{GenerateSettings, generate_to_writer};

fn main() {
    let schema_json: String = match read_to_string(stdin()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading stdin: {e}");
            process::exit(1);
        }
    };

    let settings = GenerateSettings::default();
    if let Err(e) = generate_to_writer(&schema_json, &mut stdout(), &settings) {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}
