//! JSON Schema parsing, model, and settings.

pub mod dialect;
pub mod error;
#[expect(clippy::module_inception)]
pub mod json_schema;
pub mod registry;
pub mod retrievers;
pub mod settings;
pub mod spec_version;
pub mod type_set;

pub use dialect::{
    Dialect, DialectWarning, ExclusiveBounds, FormatMode, IntegerSemantics, Keyword, RefSiblings,
    ResolvedDialect, SubschemaPosition, Vocabulary, VocabularyKind, VocabularySet,
};
pub use error::{JsonSchemaParseError, JsonSchemaParseErrors, JsonSchemaParseResult};
pub use json_schema::{JsonSchema, Schema, SchemaRef};
pub use registry::{
    DEFAULT_BASE_URI, OfflineRetriever, Registry, RegistryBuilder, RegistryError, RegistryErrors,
    Resolved, Retrieve, RetrieveError, Scope, UnknownRef,
};
#[cfg(feature = "http")]
pub use retrievers::HttpRetriever;
pub use retrievers::{FileRetriever, file_uri};
pub use settings::{
    DialectSettings, JsonSchemaSettings, JsonSchemaSettingsBuilder, UnknownSchemaUri,
    resolve_dialect,
};
pub use spec_version::SpecVersion;
pub use type_set::{JsonType, TypeSet, UnknownJsonType};
