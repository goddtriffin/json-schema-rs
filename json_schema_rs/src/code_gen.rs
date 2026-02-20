//! Code generation: schema → source in a target language.
//!
//! A [`CodeGenBackend`] takes the intermediate [`JsonSchema`] and returns generated
//! source as bytes. The CLI matches on the language argument and calls the
//! appropriate backend (e.g. the Rust backend).

use crate::code_gen_settings::CodeGenSettings;
use crate::error::Error;
use crate::json_schema::JsonSchema;

/// Contract for a codegen backend: schemas in, one generated source buffer per schema out.
pub trait CodeGenBackend {
    /// Generate model source for each schema. Returns one UTF-8 encoded byte buffer per schema.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RootNotObject`] if a root schema is not an object with properties.
    /// Returns [`Error::Io`] on write failure.
    /// Returns [`Error::Batch`] with index when one schema in the batch fails.
    fn generate(
        &self,
        schemas: &[JsonSchema],
        settings: &CodeGenSettings,
    ) -> Result<Vec<Vec<u8>>, Error>;
}
