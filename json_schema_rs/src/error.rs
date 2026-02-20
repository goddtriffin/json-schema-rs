//! Error type for schema validation and code generation.

use std::fmt;

/// Errors that can occur during code generation.
#[derive(Debug)]
pub enum Error {
    /// Root schema is not an object with properties.
    RootNotObject,
    /// I/O error while writing output.
    Io(std::io::Error),
    /// One schema in a batch failed; index is the 0-based schema index.
    Batch { index: usize, source: Box<Error> },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::RootNotObject => write!(
                f,
                "root schema must have type \"object\" and non-empty properties"
            ),
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Batch { index, source } => {
                write!(f, "schema at index {index}: {source}")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::RootNotObject => None,
            Error::Batch { source, .. } => Some(source.as_ref()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
