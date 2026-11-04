//! Code generation: schema → source in a target language.
//!
//! A [`CodeGenBackend`] takes parsed [`Schema`](crate::Schema) documents (a schema object or a
//! boolean schema) and returns generated source as bytes. The CLI matches on the language
//! argument and calls the appropriate backend (e.g. the Rust backend).

mod embedded_prune;
mod error;
mod input;
mod rust_backend;
mod rust_validators;
mod settings;
mod type_graph;

pub use error::{CodeGenError, CodeGenResult};
pub use input::CodeGenInput;
pub use rust_backend::{RustBackend, generate_rust};
pub use settings::{
    CodeGenSettings, CodeGenSettingsBuilder, DedupeMode, ModelNameSource, UniqueItemsType,
    UnknownProperties,
};

/// Output of Rust code generation: optional shared buffer plus one buffer per input schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateRustOutput {
    /// When dedupe is enabled and at least one struct is shared across schemas, contains the shared struct definitions. Otherwise `None`.
    pub shared: Option<Vec<u8>>,
    /// One UTF-8 Rust source buffer per input schema; length equals number of schemas.
    pub per_schema: Vec<Vec<u8>>,
    /// The name of each input schema's root type (a struct, an enum, or a type alias), in input
    /// order. It is in scope in the schema's buffer, also when the type itself lives in the
    /// shared buffer.
    pub root_types: Vec<String>,
}

/// Contract for a codegen backend: schemas in, [`GenerateRustOutput`] with optional shared buffer and per-schema buffers.
pub trait CodeGenBackend {
    /// Generate model source for each input schema. Returns shared buffer (if any) and one
    /// buffer per input schema.
    ///
    /// # Errors
    ///
    /// Returns [`CodeGenError::Batch`] with the schema's index when one schema fails (an
    /// unresolvable `$ref`, a document that cannot be retrieved, a registry problem), and
    /// [`CodeGenError::Io`] on write failure.
    fn generate(
        &self,
        input: &CodeGenInput<'_>,
        settings: &CodeGenSettings,
    ) -> CodeGenResult<GenerateRustOutput>;
}
