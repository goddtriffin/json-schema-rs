//! Errors when parsing (ingesting) JSON Schema.

use std::fmt;

/// Error when parsing (ingesting) a JSON Schema with the given settings.
#[derive(Debug)]
pub enum SchemaIngestionError {
    /// JSON or serde error (invalid JSON, wrong types, etc.).
    Serde(serde_json::Error),
    /// An unknown key was present and strict ingestion was enabled.
    UnknownField {
        /// The unknown key name.
        key: String,
        /// JSON Pointer or path to the schema object that contained the key.
        path: String,
    },
}

impl fmt::Display for SchemaIngestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaIngestionError::Serde(e) => write!(f, "invalid JSON Schema: {e}"),
            SchemaIngestionError::UnknownField { key, path } => {
                write!(f, "unknown schema key \"{key}\" at {path}")
            }
        }
    }
}

impl std::error::Error for SchemaIngestionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SchemaIngestionError::Serde(e) => Some(e),
            SchemaIngestionError::UnknownField { .. } => None,
        }
    }
}

impl From<serde_json::Error> for SchemaIngestionError {
    fn from(e: serde_json::Error) -> Self {
        SchemaIngestionError::Serde(e)
    }
}
