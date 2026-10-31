use crate::json_pointer::JsonPointer;
use crate::json_schema::TypeSet;
use serde_json::Number;
use std::fmt;

pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// A single validation failure: what is wrong ([`ValidationErrorKind`]), where in the instance,
/// and which schema keyword found it.
///
/// The locations follow the JSON Schema output format (2020-12 core §12.3): `keyword_location`
/// is the path through the schema as evaluated, `$ref` and the dynamic references included
/// (`/properties/width/$ref/minimum`); `absolute_keyword_location` is the same keyword as a full
/// URI with a JSON Pointer fragment, after the last reference followed
/// (`https://example.com/schemas/common#/$defs/count/minimum`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// JSON Pointer to the instance value that failed.
    pub instance_location: JsonPointer,
    /// JSON Pointer to the failing keyword through the schema as evaluated, including every
    /// `$ref`, `$dynamicRef` and `$recursiveRef` crossed (2020-12 core §12.3.1,
    /// `keywordLocation`). For a boolean schema `false` it is the location of that subschema.
    pub keyword_location: JsonPointer,
    /// The failing keyword as an absolute URI with a JSON Pointer fragment: the canonical URI of
    /// the last reference target crossed, followed by the path from it (2020-12 core §12.3.2,
    /// `absoluteKeywordLocation`). `None` when evaluation crossed no reference, or when that
    /// target's resource has no absolute URI of its own (no `$id` and no base URI set on the
    /// registry), the two cases the specification allows to omit it.
    pub absolute_keyword_location: Option<String>,
    /// What was wrong.
    pub kind: ValidationErrorKind,
}

impl ValidationError {
    /// An error of `kind` at `instance_location`, found by the keyword at `keyword_location`, with no
    /// absolute location.
    #[must_use]
    pub fn new(
        kind: ValidationErrorKind,
        instance_location: JsonPointer,
        keyword_location: JsonPointer,
    ) -> ValidationError {
        ValidationError {
            instance_location,
            keyword_location,
            absolute_keyword_location: None,
            kind,
        }
    }

    /// The same error with `absolute_keyword_location` set.
    #[must_use]
    pub fn with_absolute_keyword_location(mut self, uri: impl Into<String>) -> ValidationError {
        self.absolute_keyword_location = Some(uri.into());
        self
    }
}

impl std::error::Error for ValidationError {}

/// One line: the instance location and the message (`/age: value 15 is above maximum 10`).
impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}",
            self.instance_location.display_root_or_path(),
            self.kind
        )
    }
}

/// What a [`ValidationError`] found: one variant per kind of problem, with the instance
/// context (actual value, count, length, or "got" type) and the schema constraint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationErrorKind {
    /// Schema had an invalid or unsupported `$ref`, or the reference could not be resolved.
    InvalidRef {
        /// The `$ref` string from the schema.
        ref_str: String,
        /// Human-readable reason (for user-facing context).
        reason: String,
    },
    /// Schema had `type: "object"` but the instance was not an object.
    ExpectedObject {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "string"` but the instance was not a string.
    ExpectedString {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "integer"` but the instance was not an integer (e.g. float, string, or non-number).
    ExpectedInteger {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "number"` but the instance was not a number (e.g. string, null, or non-number).
    ExpectedNumber {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "array"` but the instance was not an array.
    ExpectedArray {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "boolean"` but the instance was not a boolean.
    ExpectedBoolean {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had `type: "null"` but the instance was not null.
    ExpectedNull {
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// Schema had a `type` array (or an empty one) and the instance matched none of its types.
    ExpectedType {
        /// The schema's allowed types.
        expected: TypeSet,
        /// JSON type of the instance (for user-facing context).
        got: String,
    },
    /// The schema at this location was the boolean schema `false`, which no value satisfies.
    FalseSchema,
    /// The schema cannot be used: its `$ref` registry could not be built for a reason other
    /// than one `$ref` (a duplicate `$id` or anchor, a metaschema requiring an unsupported
    /// vocabulary, ...). Reported once, at the instance root.
    InvalidSchema {
        /// Why the schema cannot be used.
        reason: String,
    },
    /// Schema had `uniqueItems: true` but the array contained duplicate elements.
    DuplicateArrayItems {
        /// Serialized duplicate value (for user-facing context).
        duplicate_value: String,
    },
    /// Schema had `minItems` but the array had fewer elements.
    TooFewItems {
        /// The schema's minItems value.
        min_items: u64,
        /// Actual number of items in the array (for user-facing context).
        actual_count: u64,
    },
    /// Schema had `maxItems` but the array had more elements.
    TooManyItems {
        /// The schema's maxItems value.
        max_items: u64,
        /// Actual number of items in the array (for user-facing context).
        actual_count: u64,
    },
    /// A property listed in `required` was absent.
    MissingRequired {
        /// The required property name that was missing.
        property: String,
    },
    /// Schema had `additionalProperties: false` but the instance contained a property not in `properties`.
    DisallowedAdditionalProperty {
        /// The property name that is not allowed.
        property: String,
    },
    /// Schema had `enum` but the instance value was not one of the allowed values.
    NotInEnum {
        /// Serialized invalid value (for user-facing context).
        invalid_value: String,
        /// Serialized allowed enum values (for user-facing context).
        allowed: Vec<String>,
    },
    /// Schema had `const` but the instance value was not equal to the const value.
    NotConst {
        /// Serialized expected (const) value (for user-facing context).
        expected: String,
        /// Serialized actual instance value (for user-facing context).
        actual: String,
    },
    /// Instance was below the schema's `minimum` (inclusive lower bound).
    BelowMinimum {
        /// The schema's minimum value, exactly as written.
        minimum: Number,
        /// Actual instance value (for user-facing context).
        actual: Number,
    },
    /// Instance was above the schema's `maximum` (inclusive upper bound).
    AboveMaximum {
        /// The schema's maximum value, exactly as written.
        maximum: Number,
        /// Actual instance value (for user-facing context).
        actual: Number,
    },
    /// Instance was not above the schema's `exclusiveMinimum` (exclusive lower bound).
    NotAboveExclusiveMinimum {
        /// The schema's exclusive minimum, exactly as written.
        exclusive_minimum: Number,
        /// Actual instance value (for user-facing context).
        actual: Number,
    },
    /// Instance was not below the schema's `exclusiveMaximum` (exclusive upper bound).
    NotBelowExclusiveMaximum {
        /// The schema's exclusive maximum, exactly as written.
        exclusive_maximum: Number,
        /// Actual instance value (for user-facing context).
        actual: Number,
    },
    /// Instance was not an integer multiple of the schema's `multipleOf`.
    NotMultipleOf {
        /// The schema's `multipleOf`, exactly as written.
        multiple_of: Number,
        /// Actual instance value (for user-facing context).
        actual: Number,
    },
    /// Schema had `minLength` but the string had fewer Unicode code points.
    TooShort {
        /// The schema's minLength value.
        min_length: u64,
        /// Actual Unicode code point count (for user-facing context).
        actual_length: u64,
    },
    /// Schema had `maxLength` but the string had more Unicode code points.
    TooLong {
        /// The schema's maxLength value.
        max_length: u64,
        /// Actual Unicode code point count (for user-facing context).
        actual_length: u64,
    },
    /// Schema had `pattern` but the string did not match the ECMA 262 regex.
    PatternMismatch {
        /// The schema's pattern value (ECMA 262 regex string).
        pattern: String,
        /// The instance string value (for user-facing context).
        value: String,
    },
    /// Schema had an invalid `pattern` (not a valid ECMA 262 regex); compilation failed.
    InvalidPatternInSchema {
        /// The invalid pattern string from the schema.
        pattern: String,
    },
    /// The string instance is not in the `format` the schema asserts (only when formats are
    /// asserted: see `FormatMode`).
    InvalidFormat {
        /// The format name (`"date-time"`, `"uuid"`, ...).
        format: String,
        /// The invalid string value (for user-facing context).
        value: String,
    },
    /// The instance matched the schema of `not`.
    NotSchemaMatched,
    /// The object had fewer members than `minProperties`.
    TooFewProperties {
        /// The schema's `minProperties`.
        min_properties: u64,
        /// Number of members in the instance.
        actual_count: u64,
    },
    /// The object had more members than `maxProperties`.
    TooManyProperties {
        /// The schema's `maxProperties`.
        max_properties: u64,
        /// Number of members in the instance.
        actual_count: u64,
    },
    /// A property that `dependentRequired` (or the array form of `dependencies`) requires,
    /// because another property is present, was absent.
    MissingDependentProperty {
        /// The missing property.
        property: String,
        /// The present property that requires it.
        required_by: String,
    },
    /// A member name failed the `propertyNames` schema.
    InvalidPropertyName {
        /// The member name.
        property: String,
    },
    /// Fewer items matched `contains` than `minContains` (1 when absent) requires.
    TooFewContains {
        /// The required minimum.
        min_contains: u64,
        /// Number of items that matched.
        match_count: u64,
    },
    /// More items matched `contains` than `maxContains` allows.
    TooManyContains {
        /// The schema's `maxContains`.
        max_contains: u64,
        /// Number of items that matched.
        match_count: u64,
    },
    /// Schema had `unevaluatedProperties: false` and no other keyword evaluated this member.
    DisallowedUnevaluatedProperty {
        /// The member name.
        property: String,
    },
    /// Evaluation re-entered the same `$ref` target at the same instance location without
    /// consuming any of the instance (an in-place loop such as
    /// `{"anyOf": [{"type": "string"}, {"$ref": "#"}]}`), so that branch fails.
    RefCycle {
        /// The `$ref` (or `$dynamicRef` / `$recursiveRef`) that closed the loop.
        ref_str: String,
    },
    /// Evaluation nested deeper than the validator's depth guard (deeply nested instances under
    /// a recursive schema), which stops before the native stack can overflow.
    DepthLimitExceeded {
        /// The depth limit.
        limit: usize,
    },
    /// Schema had `anyOf` or `oneOf` (the keyword location says which) but the instance did not
    /// validate against any of the subschemas.
    NoSubschemaMatched {
        /// Number of subschemas in the anyOf / oneOf (for user-facing context).
        subschema_count: usize,
        /// Why each subschema failed: every error of every branch, in branch order (each with
        /// its own keyword location, under `/anyOf/<i>` or `/oneOf/<i>`).
        causes: Vec<ValidationError>,
    },
    /// Schema had `oneOf` but the instance validated against more than one subschema.
    MultipleSubschemasMatched {
        /// Number of subschemas in the oneOf (for user-facing context).
        subschema_count: usize,
        /// Number of subschemas that passed validation (must be >= 2 for this error).
        match_count: usize,
    },
}

/// The message alone, without a location.
impl fmt::Display for ValidationErrorKind {
    #[expect(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationErrorKind::InvalidRef {
                ref_str, reason, ..
            } => {
                write!(f, "could not resolve $ref \"{ref_str}\": {reason}")
            }
            ValidationErrorKind::ExpectedObject { got, .. } => {
                write!(f, "expected object, got {got}")
            }
            ValidationErrorKind::ExpectedString { got, .. } => {
                write!(f, "expected string, got {got}")
            }
            ValidationErrorKind::ExpectedInteger { got, .. } => {
                write!(f, "expected integer, got {got}")
            }
            ValidationErrorKind::ExpectedNumber { got, .. } => {
                write!(f, "expected number, got {got}")
            }
            ValidationErrorKind::ExpectedArray { got, .. } => {
                write!(f, "expected array, got {got}")
            }
            ValidationErrorKind::ExpectedBoolean { got, .. } => {
                write!(f, "expected boolean, got {got}")
            }
            ValidationErrorKind::ExpectedNull { got, .. } => {
                write!(f, "expected null, got {got}")
            }
            ValidationErrorKind::ExpectedType { expected, got, .. } => {
                write!(f, "expected one of {expected}, got {got}")
            }
            ValidationErrorKind::FalseSchema => {
                write!(f, "no value is allowed here (schema is false)")
            }
            ValidationErrorKind::InvalidSchema { reason, .. } => {
                write!(f, "the schema cannot be used: {reason}")
            }
            ValidationErrorKind::DuplicateArrayItems {
                duplicate_value, ..
            } => {
                write!(f, "array has duplicate items (value: {duplicate_value})")
            }
            ValidationErrorKind::TooFewItems {
                min_items,
                actual_count,
                ..
            } => {
                write!(
                    f,
                    "array has {actual_count} item(s), minimum is {min_items}"
                )
            }
            ValidationErrorKind::TooManyItems {
                max_items,
                actual_count,
                ..
            } => {
                write!(
                    f,
                    "array has {actual_count} item(s), maximum is {max_items}"
                )
            }
            ValidationErrorKind::MissingRequired { property, .. } => {
                write!(f, "missing required property \"{property}\"")
            }
            ValidationErrorKind::DisallowedAdditionalProperty { property, .. } => {
                write!(f, "additional property \"{property}\" not allowed")
            }
            ValidationErrorKind::NotInEnum {
                invalid_value,
                allowed,
                ..
            } => {
                let allowed_str: String = allowed.join(", ");
                write!(
                    f,
                    "value {invalid_value} not in enum (allowed: {allowed_str})"
                )
            }
            ValidationErrorKind::NotConst {
                expected, actual, ..
            } => {
                write!(
                    f,
                    "value {actual} does not match const (expected: {expected})"
                )
            }
            ValidationErrorKind::BelowMinimum {
                minimum, actual, ..
            } => {
                write!(f, "value {actual} is below minimum {minimum}")
            }
            ValidationErrorKind::AboveMaximum {
                maximum, actual, ..
            } => {
                write!(f, "value {actual} is above maximum {maximum}")
            }
            ValidationErrorKind::NotAboveExclusiveMinimum {
                exclusive_minimum,
                actual,
                ..
            } => {
                write!(
                    f,
                    "value {actual} is not above exclusiveMinimum {exclusive_minimum}"
                )
            }
            ValidationErrorKind::NotBelowExclusiveMaximum {
                exclusive_maximum,
                actual,
                ..
            } => {
                write!(
                    f,
                    "value {actual} is not below exclusiveMaximum {exclusive_maximum}"
                )
            }
            ValidationErrorKind::NotMultipleOf {
                multiple_of,
                actual,
                ..
            } => {
                write!(f, "value {actual} is not a multiple of {multiple_of}")
            }
            ValidationErrorKind::TooShort {
                min_length,
                actual_length,
                ..
            } => {
                write!(
                    f,
                    "string has {actual_length} code points, minLength is {min_length}"
                )
            }
            ValidationErrorKind::TooLong {
                max_length,
                actual_length,
                ..
            } => {
                write!(
                    f,
                    "string has {actual_length} code points, maxLength is {max_length}"
                )
            }
            ValidationErrorKind::PatternMismatch { pattern, value, .. } => {
                write!(f, "string \"{value}\" does not match pattern \"{pattern}\"")
            }
            ValidationErrorKind::InvalidPatternInSchema { pattern, .. } => {
                write!(f, "schema has invalid pattern \"{pattern}\"")
            }
            ValidationErrorKind::InvalidFormat { format, value, .. } => {
                write!(f, "string \"{value}\" is not a valid {format}")
            }
            ValidationErrorKind::NotSchemaMatched => {
                write!(f, "instance matches the schema of not")
            }
            ValidationErrorKind::TooFewProperties {
                min_properties,
                actual_count,
                ..
            } => write!(
                f,
                "object has {actual_count} propert(ies), minimum is {min_properties}"
            ),
            ValidationErrorKind::TooManyProperties {
                max_properties,
                actual_count,
                ..
            } => write!(
                f,
                "object has {actual_count} propert(ies), maximum is {max_properties}"
            ),
            ValidationErrorKind::MissingDependentProperty {
                property,
                required_by,
                ..
            } => write!(
                f,
                "missing property \"{property}\", required when \"{required_by}\" is present"
            ),
            ValidationErrorKind::InvalidPropertyName { property, .. } => {
                write!(
                    f,
                    "property name \"{property}\" does not match propertyNames"
                )
            }
            ValidationErrorKind::TooFewContains {
                min_contains,
                match_count,
                ..
            } => write!(
                f,
                "{match_count} item(s) match contains, minimum is {min_contains}"
            ),
            ValidationErrorKind::TooManyContains {
                max_contains,
                match_count,
                ..
            } => write!(
                f,
                "{match_count} item(s) match contains, maximum is {max_contains}"
            ),
            ValidationErrorKind::DisallowedUnevaluatedProperty { property, .. } => {
                write!(f, "unevaluated property \"{property}\" not allowed")
            }
            ValidationErrorKind::RefCycle { ref_str, .. } => write!(
                f,
                "$ref \"{ref_str}\" loops back to itself without consuming the instance"
            ),
            ValidationErrorKind::DepthLimitExceeded { limit, .. } => {
                write!(f, "evaluation nested deeper than {limit} levels")
            }
            ValidationErrorKind::NoSubschemaMatched {
                subschema_count, ..
            } => {
                write!(
                    f,
                    "instance does not match any of the {subschema_count} subschema(s)"
                )
            }
            ValidationErrorKind::MultipleSubschemasMatched {
                subschema_count,
                match_count,
                ..
            } => {
                write!(
                    f,
                    "instance matches {match_count} of the {subschema_count} oneOf subschema(s), exactly one required"
                )
            }
        }
    }
}
