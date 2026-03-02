//! JSON Schema parsing, model, and settings.

pub mod error;
#[expect(clippy::module_inception)]
pub mod json_schema;
pub mod ref_resolver;
pub mod settings;
pub mod spec_version;

pub use error::{JsonSchemaParseError, JsonSchemaParseResult};
pub use json_schema::JsonSchema;
pub use settings::{JsonSchemaSettings, JsonSchemaSettingsBuilder, resolved_spec_version};
pub use spec_version::SpecVersion;
