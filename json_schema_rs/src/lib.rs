//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod json_pointer;
pub mod json_schema;
pub mod sanitize;
pub mod validator;

pub use code_gen::{
    CodeGenBackend, CodeGenError, CodeGenResult, CodeGenSettings, CodeGenSettingsBuilder,
    ModelNameSource, RustBackend, generate_rust,
};
pub use json_pointer::{JsonPointer, JsonPointerError};
pub use json_schema::{
    JsonSchema, JsonSchemaParseError, JsonSchemaParseResult, JsonSchemaSettings,
    JsonSchemaSettingsBuilder, SpecVersion, parse_schema, parse_schema_from_slice,
};
pub use validator::{ValidationError, ValidationResult, validate};
