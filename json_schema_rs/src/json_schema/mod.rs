//! JSON Schema parsing, model, and settings.

pub mod error;
pub mod parser;
pub mod schema;
pub mod settings;
pub mod spec_version;

pub use error::SchemaIngestionError;
pub use parser::{parse_schema, parse_schema_from_slice};
pub use schema::JsonSchema;
pub use settings::{JsonSchemaSettings, JsonSchemaSettingsBuilder};
pub use spec_version::SpecVersion;
