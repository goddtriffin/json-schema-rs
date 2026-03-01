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
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "string"` but the instance was not a string.
    ExpectedString {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "integer"` but the instance was not an integer (e.g. float, string, or non-number).
    ExpectedInteger {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "number"` but the instance was not a number (e.g. string, null, or non-number).
    ExpectedNumber {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "array"` but the instance was not an array.
    ExpectedArray {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `uniqueItems: true` but the array contained duplicate elements.
    DuplicateArrayItems {
        /// JSON Pointer to the array instance that failed.
        instance_path: JsonPointer,
        /// Serialized duplicate value (for user-facing context).
        duplicate_value: String,
    },
    /// Schema had `minItems` but the array had fewer elements.
    TooFewItems {
        /// JSON Pointer to the array instance that failed.
        instance_path: JsonPointer,
        /// The schema's minItems value.
        min_items: u64,
        /// Actual number of items in the array (for user-facing context).
        actual_count: u64,
    },
    /// Schema had `maxItems` but the array had more elements.
    TooManyItems {
        /// JSON Pointer to the array instance that failed.
        instance_path: JsonPointer,
        /// The schema's maxItems value.
        max_items: u64,
        /// Actual number of items in the array (for user-facing context).
        actual_count: u64,
    },
    /// A property listed in `required` was absent.
    MissingRequired {
        /// JSON Pointer to the object (parent of the missing property).
        instance_path: JsonPointer,
        /// The required property name that was missing.
        property: String,
    },
    /// Schema had `additionalProperties: false` but the instance contained a property not in `properties`.
    DisallowedAdditionalProperty {
        /// JSON Pointer to the instance (the additional property).
        instance_path: JsonPointer,
        /// The property name that is not allowed.
        property: String,
    },
    /// Schema had `enum` but the instance value was not one of the allowed values.
    NotInEnum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// Serialized invalid value (for user-facing context).
        invalid_value: String,
        /// Serialized allowed enum values (for user-facing context).
        allowed: Vec<String>,
    },
    /// Schema had `const` but the instance value was not equal to the const value.
    NotConst {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// Serialized expected (const) value (for user-facing context).
        expected: String,
        /// Serialized actual instance value (for user-facing context).
        actual: String,
    },
    /// Instance was below the schema's `minimum` (inclusive lower bound).
    BelowMinimum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's minimum value.
        minimum: OrderedF64,
        /// Actual instance value (for user-facing context).
        actual: OrderedF64,
    },
    /// Instance was above the schema's `maximum` (inclusive upper bound).
    AboveMaximum {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's maximum value.
        maximum: OrderedF64,
        /// Actual instance value (for user-facing context).
        actual: OrderedF64,
    },
    /// Schema had `minLength` but the string had fewer Unicode code points.
    TooShort {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's minLength value.
        min_length: u64,
        /// Actual Unicode code point count (for user-facing context).
        actual_length: u64,
    },
    /// Schema had `maxLength` but the string had more Unicode code points.
    TooLong {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The schema's maxLength value.
        max_length: u64,
        /// Actual Unicode code point count (for user-facing context).
        actual_length: u64,
    },
    /// The string instance does not parse as a valid UUID (only emitted when the `uuid` feature is enabled).
    #[cfg(feature = "uuid")]
    InvalidUuidFormat {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// The invalid string value (for user-facing context).
        value: String,
    },
    /// Schema had `anyOf` but the instance did not validate against any of the subschemas.
    NoSubschemaMatched {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// Number of subschemas in the anyOf (for user-facing context).
        subschema_count: usize,
    },
    /// Schema had `oneOf` but the instance validated against more than one subschema.
    MultipleSubschemasMatched {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
        /// Number of subschemas in the oneOf (for user-facing context).
        subschema_count: usize,
        /// Number of subschemas that passed validation (must be >= 2 for this error).
        match_count: usize,
    },
}

impl std::error::Error for ValidationError {}

impl ValidationError {
    #[must_use]
    pub fn instance_path(&self) -> &JsonPointer {
        match self {
            ValidationError::ExpectedObject { instance_path, .. }
            | ValidationError::ExpectedString { instance_path, .. }
            | ValidationError::ExpectedInteger { instance_path, .. }
            | ValidationError::ExpectedNumber { instance_path, .. }
            | ValidationError::ExpectedArray { instance_path, .. }
            | ValidationError::DuplicateArrayItems { instance_path, .. }
            | ValidationError::TooFewItems { instance_path, .. }
            | ValidationError::TooManyItems { instance_path, .. }
            | ValidationError::MissingRequired { instance_path, .. }
            | ValidationError::DisallowedAdditionalProperty { instance_path, .. }
            | ValidationError::NotInEnum { instance_path, .. }
            | ValidationError::NotConst { instance_path, .. }
            | ValidationError::BelowMinimum { instance_path, .. }
            | ValidationError::AboveMaximum { instance_path, .. }
            | ValidationError::TooShort { instance_path, .. }
            | ValidationError::TooLong { instance_path, .. }
            | ValidationError::NoSubschemaMatched { instance_path, .. }
            | ValidationError::MultipleSubschemasMatched { instance_path, .. } => instance_path,
            #[cfg(feature = "uuid")]
            ValidationError::InvalidUuidFormat { instance_path, .. } => instance_path,
        }
    }
}

impl fmt::Display for ValidationError {
    #[expect(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let location = self.instance_path().display_root_or_path();
        match self {
            ValidationError::ExpectedObject { got, .. } => {
                write!(f, "{location}: expected object, got {got}")
            }
            ValidationError::ExpectedString { got, .. } => {
                write!(f, "{location}: expected string, got {got}")
            }
            ValidationError::ExpectedInteger { got, .. } => {
                write!(f, "{location}: expected integer, got {got}")
            }
            ValidationError::ExpectedNumber { got, .. } => {
                write!(f, "{location}: expected number, got {got}")
            }
            ValidationError::ExpectedArray { got, .. } => {
                write!(f, "{location}: expected array, got {got}")
            }
            ValidationError::DuplicateArrayItems {
                duplicate_value, ..
            } => {
                write!(
                    f,
                    "{location}: array has duplicate items (value: {duplicate_value})"
                )
            }
            ValidationError::TooFewItems {
                min_items,
                actual_count,
                ..
            } => {
                write!(
                    f,
                    "{location}: array has {actual_count} item(s), minimum is {min_items}"
                )
            }
            ValidationError::TooManyItems {
                max_items,
                actual_count,
                ..
            } => {
                write!(
                    f,
                    "{location}: array has {actual_count} item(s), maximum is {max_items}"
                )
            }
            ValidationError::MissingRequired { property, .. } => {
                write!(f, "{location}: missing required property \"{property}\"")
            }
            ValidationError::DisallowedAdditionalProperty { property, .. } => {
                write!(
                    f,
                    "{location}: additional property \"{property}\" not allowed"
                )
            }
            ValidationError::NotInEnum {
                invalid_value,
                allowed,
                ..
            } => {
                let allowed_str: String = allowed.join(", ");
                write!(
                    f,
                    "{location}: value {invalid_value} not in enum (allowed: {allowed_str})"
                )
            }
            ValidationError::NotConst {
                expected, actual, ..
            } => {
                write!(
                    f,
                    "{location}: value {actual} does not match const (expected: {expected})"
                )
            }
            ValidationError::BelowMinimum {
                minimum, actual, ..
            } => {
                write!(
                    f,
                    "{location}: value {} is below minimum {}",
                    actual.0, minimum.0
                )
            }
            ValidationError::AboveMaximum {
                maximum, actual, ..
            } => {
                write!(
                    f,
                    "{location}: value {} is above maximum {}",
                    actual.0, maximum.0
                )
            }
            ValidationError::TooShort {
                min_length,
                actual_length,
                ..
            } => {
                write!(
                    f,
                    "{location}: string has {actual_length} code points, minLength is {min_length}"
                )
            }
            ValidationError::TooLong {
                max_length,
                actual_length,
                ..
            } => {
                write!(
                    f,
                    "{location}: string has {actual_length} code points, maxLength is {max_length}"
                )
            }
            #[cfg(feature = "uuid")]
            ValidationError::InvalidUuidFormat { value, .. } => {
                write!(f, "{location}: string \"{value}\" is not a valid UUID")
            }
            ValidationError::NoSubschemaMatched {
                subschema_count, ..
            } => {
                write!(
                    f,
                    "{location}: instance does not match any of the {subschema_count} subschema(s)"
                )
            }
            ValidationError::MultipleSubschemasMatched {
                subschema_count,
                match_count,
                ..
            } => {
                write!(
                    f,
                    "{location}: instance matches {match_count} of the {subschema_count} oneOf subschema(s), exactly one required"
                )
            }
        }
    }
}
