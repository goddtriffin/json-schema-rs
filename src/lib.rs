//! Generate Rust structs from JSON Schema.

mod codegen;
mod error;
mod schema;

pub use error::JsonSchemaGenError;
pub use schema::JsonSchema;

use std::io::Write;
use std::path::Path;

/// Generate Rust structs from a JSON Schema string and write to `writer`.
///
/// The writer can be any type implementing `Write`, such as `File`, `Vec<u8>`, or
/// `Cursor<Vec<u8>>`, enabling easy unit testing without file system interaction.
///
/// # Errors
///
/// Returns `JsonSchemaGenError` if the schema JSON is invalid, the root is not an object,
/// or writing to the writer fails.
pub fn generate_to_writer<W: Write>(
    schema_json: &str,
    writer: &mut W,
) -> Result<(), JsonSchemaGenError> {
    codegen::generate_to_writer(schema_json, writer)
}

/// Generate Rust structs from a JSON Schema file and write to an output file.
///
/// # Errors
///
/// Returns `JsonSchemaGenError` if reading the input file fails, the schema JSON is invalid,
/// the root is not an object, or writing to the output file fails.
pub fn generate_from_file(
    input_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<(), JsonSchemaGenError> {
    let schema_json: String = std::fs::read_to_string(input_path)?;
    let mut output_file: std::fs::File = std::fs::File::create(output_path)?;
    generate_to_writer(&schema_json, &mut output_file)
}
