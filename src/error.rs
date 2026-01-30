use std::error;
use std::fmt;

/// Error type for JSON Schema code generation operations.
#[derive(Debug)]
pub enum JsonSchemaGenError {
    /// Generic error with a message.
    GenericError(String),

    /// I/O error (e.g., reading schema file, writing output file).
    IoError(std::io::Error),

    /// JSON parsing error.
    JsonError(serde_json::Error),
}

impl error::Error for JsonSchemaGenError {}

impl fmt::Display for JsonSchemaGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenericError(message) => write!(f, "{message}"),
            Self::IoError(io_error) => fmt::Display::fmt(io_error, f),
            Self::JsonError(json_error) => fmt::Display::fmt(json_error, f),
        }
    }
}

impl From<&str> for JsonSchemaGenError {
    fn from(message: &str) -> Self {
        Self::GenericError(message.to_string())
    }
}

impl From<String> for JsonSchemaGenError {
    fn from(message: String) -> Self {
        Self::GenericError(message)
    }
}

impl From<std::io::Error> for JsonSchemaGenError {
    fn from(io_error: std::io::Error) -> Self {
        Self::IoError(io_error)
    }
}

impl From<serde_json::Error> for JsonSchemaGenError {
    fn from(json_error: serde_json::Error) -> Self {
        Self::JsonError(json_error)
    }
}
