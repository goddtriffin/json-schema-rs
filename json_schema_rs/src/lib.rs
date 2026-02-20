//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod error;
pub mod json_pointer;
pub mod json_schema;
pub mod json_schema_parser;
pub mod json_schema_settings;
pub mod json_schema_spec_version;
pub mod sanitize;
pub mod validation;

pub use code_gen::{
    CodeGenBackend, CodeGenSettings, CodeGenSettingsBuilder, ModelNameSource, RustBackend,
    generate_rust,
};
pub use error::Error;
pub use json_pointer::{JsonPointer, JsonPointerError};
pub use json_schema::{JsonSchema, SchemaIngestionError};
pub use json_schema_parser::{parse_schema, parse_schema_from_slice};
pub use json_schema_settings::{JsonSchemaSettings, JsonSchemaSettingsBuilder};
pub use json_schema_spec_version::JsonSchemaSpecVersion;
pub use validation::{ValidationError, ValidationResult, validate};
