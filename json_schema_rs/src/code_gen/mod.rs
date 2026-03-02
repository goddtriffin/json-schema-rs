//! Code generation: schema → source in a target language.
//!
//! A [`CodeGenBackend`] takes the intermediate [`JsonSchema`] and returns generated
//! source as bytes. The CLI matches on the language argument and calls the
//! appropriate backend (e.g. the Rust backend).

mod error;
mod rust_backend;
mod settings;

pub use error::{CodeGenError, CodeGenResult};
pub use rust_backend::{RustBackend, generate_rust};
pub use settings::{CodeGenSettings, CodeGenSettingsBuilder, DedupeMode, ModelNameSource};

use crate::json_schema::JsonSchema;

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
    /// Generate model source for each schema. Returns shared buffer (if any) and one buffer per schema.
    ///
    /// # Errors
    ///
    /// Returns [`CodeGenError::RootNotObject`] if a root schema is not an object with properties.
    /// Returns [`CodeGenError::Io`] on write failure.
    /// Returns [`CodeGenError::Batch`] with index when one schema in the batch fails.
    fn generate(
        &self,
        schemas: &[JsonSchema],
        settings: &CodeGenSettings,
    ) -> CodeGenResult<GenerateRustOutput>;
}
