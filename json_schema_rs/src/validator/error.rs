use crate::json_pointer::JsonPointer;
use std::fmt;

pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// A single validation failure: kind and instance location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Schema had `type: "object"` but the instance was not an object.
    ExpectedObject {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// Schema had `type: "string"` but the instance was not a string.
    ExpectedString {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// A property listed in `required` was absent.
    MissingRequired {
        /// JSON Pointer to the object (parent of the missing property).
        instance_path: JsonPointer,
        /// The required property name that was missing.
        property: String,
    },
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    #[must_use]
    pub fn instance_path(&self) -> &JsonPointer {
        match self {
            ValidationError::ExpectedObject { instance_path }
            | ValidationError::ExpectedString { instance_path }
            | ValidationError::MissingRequired { instance_path, .. } => instance_path,
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let location = self.instance_path().display_root_or_path();
        match self {
            ValidationError::ExpectedObject { .. } => {
                write!(f, "{location}: expected object")
            }
            ValidationError::ExpectedString { .. } => {
                write!(f, "{location}: expected string")
            }
            ValidationError::MissingRequired { property, .. } => {
                write!(f, "{location}: missing required property \"{property}\"")
            }
        }
    }
}
