use crate::json_pointer::JsonPointer;
use std::fmt;

/// Wraps f64 so that `ValidationError` can derive Eq (f64 is not Eq; comparison is by bits).
#[derive(Debug, Clone, Copy)]
pub struct OrderedF64(pub f64);

impl PartialEq for OrderedF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for OrderedF64 {}

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
    /// Schema had `type: "integer"` but the instance was not an integer (e.g. float, string, or non-number).
    ExpectedInteger {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// Schema had `type: "number"` but the instance was not a number (e.g. string, null, or non-number).
    ExpectedNumber {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// Schema had `type: "array"` but the instance was not an array.
    ExpectedArray {
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
    /// Schema had `enum` but the instance value was not one of the allowed values.
    NotInEnum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// Instance was below the schema's `minimum` (inclusive lower bound).
    BelowMinimum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's minimum value.
        minimum: OrderedF64,
    },
    /// Instance was above the schema's `maximum` (inclusive upper bound).
    AboveMaximum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's maximum value.
        maximum: OrderedF64,
    },
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    #[must_use]
    pub fn instance_path(&self) -> &JsonPointer {
        match self {
            ValidationError::ExpectedObject { instance_path }
            | ValidationError::ExpectedString { instance_path }
            | ValidationError::ExpectedInteger { instance_path }
            | ValidationError::ExpectedNumber { instance_path }
            | ValidationError::ExpectedArray { instance_path }
            | ValidationError::MissingRequired { instance_path, .. }
            | ValidationError::NotInEnum { instance_path }
            | ValidationError::BelowMinimum { instance_path, .. }
            | ValidationError::AboveMaximum { instance_path, .. } => instance_path,
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
            ValidationError::ExpectedInteger { .. } => {
                write!(f, "{location}: expected integer")
            }
            ValidationError::ExpectedNumber { .. } => {
                write!(f, "{location}: expected number")
            }
            ValidationError::ExpectedArray { .. } => {
                write!(f, "{location}: expected array")
            }
            ValidationError::MissingRequired { property, .. } => {
                write!(f, "{location}: missing required property \"{property}\"")
            }
            ValidationError::NotInEnum { .. } => {
                write!(f, "{location}: value not in enum")
            }
            ValidationError::BelowMinimum { minimum, .. } => {
                write!(f, "{location}: value below minimum {}", minimum.0)
            }
            ValidationError::AboveMaximum { maximum, .. } => {
                write!(f, "{location}: value above maximum {}", maximum.0)
            }
        }
    }
}
