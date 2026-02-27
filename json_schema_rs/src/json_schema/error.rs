//! Errors when parsing JSON Schema.

use std::fmt;

/// Error when parsing a JSON Schema with the given settings.
#[derive(Debug)]
pub enum JsonSchemaParseError {
    /// JSON or serde error (invalid JSON, wrong types, etc.).
    Serde(serde_json::Error),
    /// An unknown key was present and strict ingestion was enabled.
    UnknownField {
        /// The unknown key name.
        key: String,
        /// JSON Pointer or path to the schema object that contained the key.
        path: String,
    },
    /// I/O error when reading from a reader or file.
    Io(std::io::Error),
}

/// Result type for JSON Schema parsing operations.
pub type JsonSchemaParseResult<T> = Result<T, JsonSchemaParseError>;

impl fmt::Display for JsonSchemaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonSchemaParseError::Serde(e) => write!(f, "invalid JSON Schema: {e}"),
            JsonSchemaParseError::UnknownField { key, path } => {
                write!(f, "unknown schema key \"{key}\" at {path}")
            }
            JsonSchemaParseError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for JsonSchemaParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            JsonSchemaParseError::Serde(e) => Some(e),
            JsonSchemaParseError::UnknownField { .. } => None,
            JsonSchemaParseError::Io(e) => Some(e),
        }
    }
}

impl From<serde_json::Error> for JsonSchemaParseError {
    fn from(e: serde_json::Error) -> Self {
        JsonSchemaParseError::Serde(e)
    }
}

impl From<std::io::Error> for JsonSchemaParseError {
    fn from(e: std::io::Error) -> Self {
        JsonSchemaParseError::Io(e)
    }
}
