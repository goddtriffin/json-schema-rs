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
    /// allOf is present but empty (no subschemas to merge).
    AllOfMergeEmpty,
    /// At least one subschema in allOf is not object-like (no type "object" and no non-empty properties).
    AllOfMergeNonObjectSubschema { index: usize },
    /// Same property appears in multiple subschemas with incompatible types (e.g. string vs integer).
    AllOfMergeConflictingPropertyType {
        property_key: String,
        subschema_indices: Vec<usize>,
    },
    /// Same property has conflicting minimum/maximum (or minLength/maxLength, minItems/maxItems) across subschemas that cannot be merged.
    AllOfMergeConflictingNumericBounds {
        property_key: String,
        keyword: String,
    },
    /// Same property has enum in more than one subschema with incompatible value sets.
    AllOfMergeConflictingEnum { property_key: String },
    /// Same property has const in more than one subschema with different values.
    AllOfMergeConflictingConst { property_key: String },
    /// Subschema uses unsupported features for merge (e.g. $ref, non-object type).
    AllOfMergeUnsupportedSubschema { index: usize, reason: String },
    /// anyOf is present but empty (no subschemas).
    AnyOfEmpty,
    /// oneOf is present but empty (no subschemas).
    OneOfEmpty,
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
            CodeGenError::AllOfMergeEmpty => {
                write!(f, "allOf is present but empty (no subschemas to merge)")
            }
            CodeGenError::AllOfMergeNonObjectSubschema { index } => write!(
                f,
                "allOf subschema at index {index} is not object-like (need type \"object\" or non-empty properties)"
            ),
            CodeGenError::AllOfMergeConflictingPropertyType {
                property_key,
                subschema_indices,
            } => write!(
                f,
                "allOf merge: property \"{property_key}\" has conflicting types in subschemas at indices {subschema_indices:?}"
            ),
            CodeGenError::AllOfMergeConflictingNumericBounds {
                property_key,
                keyword,
            } => write!(
                f,
                "allOf merge: property \"{property_key}\" has conflicting {keyword} across subschemas"
            ),
            CodeGenError::AllOfMergeConflictingEnum { property_key } => write!(
                f,
                "allOf merge: property \"{property_key}\" has conflicting enum values across subschemas"
            ),
            CodeGenError::AllOfMergeConflictingConst { property_key } => write!(
                f,
                "allOf merge: property \"{property_key}\" has conflicting const values across subschemas"
            ),
            CodeGenError::AllOfMergeUnsupportedSubschema { index, reason } => {
                write!(
                    f,
                    "allOf subschema at index {index} is unsupported for merge: {reason}"
                )
            }
            CodeGenError::AnyOfEmpty => {
                write!(f, "anyOf is present but empty (no subschemas)")
            }
            CodeGenError::OneOfEmpty => {
                write!(f, "oneOf is present but empty (no subschemas)")
            }
        }
    }
}

impl std::error::Error for CodeGenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CodeGenError::Io(e) => Some(e),
            CodeGenError::RootNotObject
            | CodeGenError::AllOfMergeEmpty
            | CodeGenError::AllOfMergeNonObjectSubschema { .. }
            | CodeGenError::AllOfMergeConflictingPropertyType { .. }
            | CodeGenError::AllOfMergeConflictingNumericBounds { .. }
            | CodeGenError::AllOfMergeConflictingEnum { .. }
            | CodeGenError::AllOfMergeConflictingConst { .. }
            | CodeGenError::AllOfMergeUnsupportedSubschema { .. }
            | CodeGenError::AnyOfEmpty
            | CodeGenError::OneOfEmpty => None,
            CodeGenError::Batch { source, .. } => Some(source.as_ref()),
        }
    }
}

impl From<std::io::Error> for CodeGenError {
    fn from(e: std::io::Error) -> Self {
        CodeGenError::Io(e)
    }
}
