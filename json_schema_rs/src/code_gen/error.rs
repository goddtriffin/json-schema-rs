//! Errors that can occur during code generation.

use std::fmt;

/// Result type for code generation operations.
pub type CodeGenResult<T> = Result<T, CodeGenError>;

/// Errors that can occur during code generation.
#[derive(Debug)]
pub enum CodeGenError {
    /// Root schema is not an object with properties.
    RootNotObject,
    /// I/O error while writing output.
    Io(std::io::Error),
    /// One schema in a batch failed; index is the 0-based schema index.
    Batch {
        index: usize,
        source: Box<CodeGenError>,
    },
}

impl fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeGenError::RootNotObject => write!(
                f,
                "root schema must have type \"object\" and non-empty properties"
            ),
            CodeGenError::Io(e) => write!(f, "io error: {e}"),
            CodeGenError::Batch { index, source } => {
                write!(f, "schema at index {index}: {source}")
            }
        }
    }
}

impl std::error::Error for CodeGenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodeGenError::Io(e) => Some(e),
            CodeGenError::RootNotObject => None,
            CodeGenError::Batch { source, .. } => Some(source.as_ref()),
        }
    }
}

impl From<std::io::Error> for CodeGenError {
    fn from(e: std::io::Error) -> Self {
        CodeGenError::Io(e)
    }
}
