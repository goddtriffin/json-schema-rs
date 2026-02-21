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
pub use settings::{CodeGenSettings, CodeGenSettingsBuilder, ModelNameSource};

use crate::json_schema::JsonSchema;

/// Contract for a codegen backend: schemas in, one generated source buffer per schema out.
pub trait CodeGenBackend {
    /// Generate model source for each schema. Returns one UTF-8 encoded byte buffer per schema.
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
    ) -> CodeGenResult<Vec<Vec<u8>>>;
}
