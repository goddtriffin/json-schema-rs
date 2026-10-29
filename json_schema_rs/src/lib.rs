//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod json_pointer;
pub mod json_schema;
mod number;
pub mod reverse_code_gen;
pub mod sanitizers;
pub mod validator;

pub use code_gen::{
    CodeGenBackend, CodeGenError, CodeGenResult, CodeGenSettings, CodeGenSettingsBuilder,
    DedupeMode, GenerateRustOutput, ModelNameSource, RustBackend, generate_rust,
};
pub use json_pointer::{JsonPointer, JsonPointerError};
pub use json_schema::{
    Dialect, DialectWarning, ExclusiveBounds, FormatMode, IntegerSemantics, JsonSchema,
    JsonSchemaParseError, JsonSchemaParseResult, JsonSchemaSettings, JsonSchemaSettingsBuilder,
    JsonType, Keyword, OfflineRetriever, RefSiblings, Registry, RegistryBuilder, RegistryError,
    Resolved, ResolvedDialect, Retrieve, RetrieveError, Schema, SchemaRef, Scope, SpecVersion,
    SubschemaPosition, TypeSet, UnknownJsonType, UnknownRef, UnknownSchemaUri, Vocabulary,
    VocabularyKind, VocabularySet, resolve_dialect,
};
pub use reverse_code_gen::{SchemaGenerator, ToJsonSchema};
pub use validator::{ValidationError, ValidationResult, validate, validate_with_registry};
