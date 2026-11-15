//! Generate Rust structs from JSON Schema.
//!
//! Accepts a JSON Schema, parses it into an in-memory model, and emits Rust source
//! to a writer. Supported keywords and types are documented in the README.

pub mod code_gen;
pub mod error_list;
#[doc(hidden)]
pub mod generated_serde;
pub mod integer;
pub mod json_pointer;
pub mod json_schema;
pub mod meta;
mod nullable;
mod number;
pub mod reverse_code_gen;
pub mod sanitizers;
mod validated;
pub mod validator;

pub use code_gen::{
    CodeGenBackend, CodeGenError, CodeGenInput, CodeGenResult, CodeGenSettings,
    CodeGenSettingsBuilder, DedupeMode, FormatTypes, GenerateRustOutput, ModelNameSource,
    RustBackend, TypeConversion, TypePatch, UniqueItemsType, UnknownProperties, generate_rust,
};
pub use error_list::ErrorList;
pub use json_pointer::{JsonPointer, JsonPointerError};
#[cfg(feature = "http")]
pub use json_schema::HttpRetriever;
#[cfg(feature = "async")]
pub use json_schema::{AsyncRetrieve, RetrieveFuture};
pub use json_schema::{
    ContentMode, DereferenceError, Dialect, DialectSettings, DialectWarning, ExclusiveBounds,
    FileRetriever, FormatMode, IntegerSemantics, JsonSchema, JsonSchemaParseError,
    JsonSchemaParseErrors, JsonSchemaParseResult, JsonSchemaSettings, JsonSchemaSettingsBuilder,
    JsonType, Keyword, MetaschemaCheck, OfflineRetriever, RefSiblings, Registry, RegistryBuilder,
    RegistryError, RegistryErrors, Resolved, ResolvedDialect, Retrieve, RetrieveError, Schema,
    SchemaRef, Scope, SpecVersion, SubschemaPosition, TypeSet, UnknownFormats, UnknownJsonType,
    UnknownRef, UnknownSchemaUri, Vocabulary, VocabularyKind, VocabularySet, file_path, file_uri,
    resolve_dialect,
};
pub use nullable::Nullable;
pub use number::json_equal;
pub use reverse_code_gen::{
    ReverseCodeGenError, ReverseCodeGenResult, SchemaGenerator, ToJsonSchema,
};
pub use validated::Validated;
#[cfg(feature = "linear-regex")]
pub use validator::RegexEngine;
pub use validator::{
    Annotation, BasicOutput, CompileError, CompileErrors, CompiledSchema, CompiledSchemaBuilder,
    CustomKeyword, EmbeddedDocument, EmbeddedDocumentError, EmbeddedDocumentKind,
    EmbeddedSchemaError, ErrorIter, FlagOutput, Instance, InstanceObject, InstanceRef,
    MaskedValidationError, MaskedValidationErrorKind, NewtypeValue, Output, OutputFormat,
    OutputUnit, SchemaError, TypeValidationError, TypeValidators, UnionMatch, UnionVariants,
    ValidateError, ValidationError, ValidationErrorKind, ValidationResult, validate,
};
