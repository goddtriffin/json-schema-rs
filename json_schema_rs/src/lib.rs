//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod json_pointer;
pub mod json_schema;
pub mod reverse_code_gen;
pub mod sanitizers;
pub mod validator;

pub use code_gen::{
    CodeGenBackend, CodeGenError, CodeGenResult, CodeGenSettings, CodeGenSettingsBuilder,
    DedupeMode, GenerateRustOutput, ModelNameSource, RustBackend, generate_rust,
};
pub use json_pointer::{JsonPointer, JsonPointerError};
pub use json_schema::{
    JsonSchema, JsonSchemaParseError, JsonSchemaParseResult, JsonSchemaSettings,
    JsonSchemaSettingsBuilder, SpecVersion, parse_schema_from_path, parse_schema_from_reader,
    parse_schema_from_serde_value, parse_schema_from_slice, parse_schema_from_str,
    resolved_spec_version,
};
pub use reverse_code_gen::ToJsonSchema;
pub use validator::{OrderedF64, ValidationError, ValidationResult, validate};
