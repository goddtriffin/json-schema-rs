//! Code generation: schema → source in a target language.
//!
//! A [`CodeGenBackend`] takes the intermediate [`JsonSchema`] and returns generated
//! source as bytes. The CLI matches on the language argument and calls the
//! appropriate backend (e.g. the Rust backend).

mod error;
mod input;
mod rust_backend;
mod settings;
mod type_graph;

pub use error::{CodeGenError, CodeGenResult};
pub use input::CodeGenInput;
pub use rust_backend::{RustBackend, generate_rust};
pub use settings::{
    CodeGenSettings, CodeGenSettingsBuilder, DedupeMode, ModelNameSource, UnknownProperties,
};

/// Output of Rust code generation: optional shared buffer plus one buffer per input schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateRustOutput {
    /// When dedupe is enabled and at least one struct is shared across schemas, contains the shared struct definitions. Otherwise `None`.
    pub shared: Option<Vec<u8>>,
    /// One UTF-8 Rust source buffer per input schema; length equals number of schemas.
    pub per_schema: Vec<Vec<u8>>,
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
