//! Errors that can occur during code generation.

use crate::json_schema::RegistryErrors;
use std::fmt;

/// Result type for code generation operations.
pub type CodeGenResult<T> = Result<T, CodeGenError>;

/// Errors that can occur during code generation.
///
/// Every schema the registry accepts gets a type: an unsatisfiable one (conflicting `allOf`
/// branches, an empty `anyOf`, `"type": []`) becomes a type with no values, so the only errors
/// are references that do not resolve and failures to write.
#[derive(Debug)]
pub enum CodeGenError {
    /// I/O error while writing output.
    Io(std::io::Error),
    /// One schema in a batch failed; index is the 0-based schema index.
    Batch {
        index: usize,
        source: Box<CodeGenError>,
    },
    /// `$ref` could not be resolved.
    RefResolution { ref_str: String, reason: String },
    /// A `then` / `else` subschema (kept raw by the schema model) does not parse as a schema.
    InvalidSubschema { reason: String },
    /// The schema's `$ref` registry could not be built: every problem found (a `$ref` that does
    /// not resolve, a duplicate `$id` or anchor, an unsupported vocabulary, ...), one per line
    /// when displayed.
    Registry(RegistryErrors),
}

impl fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeGenError::Io(e) => write!(f, "io error: {e}"),
            CodeGenError::Batch { index, source } => {
                write!(f, "schema at index {index}: {source}")
            }
            CodeGenError::RefResolution { ref_str, reason } => {
                write!(f, "could not resolve $ref \"{ref_str}\": {reason}")
            }
            CodeGenError::InvalidSubschema { reason } => {
                write!(f, "invalid then / else subschema: {reason}")
            }
            CodeGenError::Registry(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CodeGenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodeGenError::Io(e) => Some(e),
            CodeGenError::RefResolution { .. } | CodeGenError::InvalidSubschema { .. } => None,
            CodeGenError::Batch { source, .. } => Some(source.as_ref()),
            CodeGenError::Registry(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for CodeGenError {
    fn from(e: std::io::Error) -> Self {
        CodeGenError::Io(e)
    }
}
