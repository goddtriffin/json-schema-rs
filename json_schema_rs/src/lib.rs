//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod error_list;
pub mod json_pointer;
pub mod json_schema;
mod number;
pub mod reverse_code_gen;
pub mod sanitizers;
pub mod validator;

pub use code_gen::{
    CodeGenBackend, CodeGenError, CodeGenInput, CodeGenResult, CodeGenSettings,
    CodeGenSettingsBuilder, DedupeMode, GenerateRustOutput, ModelNameSource, RustBackend,
    UnknownProperties, generate_rust,
};
pub use error_list::ErrorList;
pub use json_pointer::{JsonPointer, JsonPointerError};
#[cfg(feature = "http")]
pub use json_schema::HttpRetriever;
pub use json_schema::{
    Dialect, DialectSettings, DialectWarning, ExclusiveBounds, FileRetriever, FormatMode,
    IntegerSemantics, JsonSchema, JsonSchemaParseError, JsonSchemaParseErrors,
    JsonSchemaParseResult, JsonSchemaSettings, JsonSchemaSettingsBuilder, JsonType, Keyword,
    OfflineRetriever, RefSiblings, Registry, RegistryBuilder, RegistryError, RegistryErrors,
    Resolved, ResolvedDialect, Retrieve, RetrieveError, Schema, SchemaRef, Scope, SpecVersion,
    SubschemaPosition, TypeSet, UnknownJsonType, UnknownRef, UnknownSchemaUri, Vocabulary,
    VocabularyKind, VocabularySet, file_uri, resolve_dialect,
};
pub use number::json_equal;
pub use reverse_code_gen::{
    ReverseCodeGenError, ReverseCodeGenResult, SchemaGenerator, ToJsonSchema,
};
pub use validator::{
    BasicOutput, CompileError, CompileErrors, CompiledSchema, CompiledSchemaBuilder, FlagOutput,
    FormatCheck, Output, OutputFormat, OutputUnit, SchemaError, ValidateError, ValidationError,
    ValidationErrorKind, ValidationResult, validate,
};
