//! JSON Schema parsing, model, and settings.

pub mod error;
#[expect(clippy::module_inception)]
pub mod json_schema;
pub mod parser;
pub mod settings;
pub mod spec_version;

pub use error::{JsonSchemaParseError, JsonSchemaParseResult};
pub use json_schema::JsonSchema;
pub use parser::{
    parse_schema_from_path, parse_schema_from_reader, parse_schema_from_serde_value,
    parse_schema_from_slice, parse_schema_from_str,
};
pub use settings::{JsonSchemaSettings, JsonSchemaSettingsBuilder, resolved_spec_version};
pub use spec_version::SpecVersion;
