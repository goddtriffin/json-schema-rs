use std::error;
use std::fmt;

/// A single validation issue with its JSON Pointer path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidationIssue {
    /// JSON Pointer path (RFC 6901) into the schema where the issue occurs.
    pub path: String,
    /// The kind of validation failure.
    pub kind: SchemaValidationIssueKind,
}

/// Categorizes every possible schema validation failure mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaValidationIssueKind {
    // --- Root / structural ---
    /// Root schema must have `type: "object"`.
    RootNotObject,
    /// Root schema has no `type` key.
    RootMissingType,
    /// Root object has no supported properties.
    NoStructsToGenerate,

    // --- Invalid schema structure ---
    /// `type` is not a string or array of strings.
    InvalidTypeValue,
    /// `type: ["null", "string"]` etc.—multiple types not supported.
    TypeArrayNotSupported,
    /// `type: "null"` not supported.
    NullTypeNotSupported,
    /// `required` is not an array of strings.
    InvalidRequiredFormat,
    /// `required` references a property not in `properties`.
    RequiredPropertyNotInProperties,
    /// `enum` is not an array.
    InvalidEnumFormat,
    /// `enum` is empty array.
    EnumEmpty,
    /// Enum has non-string values; only string enums supported.
    EnumContainsNonStringValues,
    /// `items` is not an object when `type` is `"array"`.
    InvalidItemsFormat,
    /// `type: "array"` but no `items` key.
    ArrayMissingItems,
    /// Default value doesn't match or isn't valid for the type.
    InvalidDefaultValue,
    /// `default` is an object (not supported).
    UnsupportedDefaultObject,
    /// `default` is non-empty array (not supported).
    UnsupportedDefaultNonEmptyArray,
    /// `minimum`/`maximum` not a number when present.
    InvalidMinimumMaximum,
    /// Property has unsupported or unknown `type`.
    PropertyWithUnsupportedType,
    /// Array `items` has unsupported type.
    ArrayItemsUnsupportedType,
    /// `additionalProperties` schema not supported.
    AdditionalPropertiesUnsupportedSchema,

    // --- Unsupported keywords ---
    /// `$ref` — schema reuse not supported.
    UnsupportedKeywordRef,
    /// `$defs` — definitions not supported.
    UnsupportedKeywordDefs,
    /// `definitions` — definitions not supported.
    UnsupportedKeywordDefinitions,
    /// `minLength` not supported.
    UnsupportedKeywordMinLength,
    /// `maxLength` not supported.
    UnsupportedKeywordMaxLength,
    /// `pattern` not supported.
    UnsupportedKeywordPattern,
    /// `oneOf` not supported.
    UnsupportedKeywordOneOf,
    /// `anyOf` not supported.
    UnsupportedKeywordAnyOf,
    /// `allOf` not supported.
    UnsupportedKeywordAllOf,
    /// `$id` not supported.
    UnsupportedKeywordId,
    /// `examples` not supported.
    UnsupportedKeywordExamples,
    /// `const` not supported.
    UnsupportedKeywordConst,
    /// `not` not supported.
    UnsupportedKeywordNot,
    /// `minProperties` not supported.
    UnsupportedKeywordMinProperties,
    /// `maxProperties` not supported.
    UnsupportedKeywordMaxProperties,
    /// `minItems` not supported.
    UnsupportedKeywordMinItems,
    /// `maxItems` not supported.
    UnsupportedKeywordMaxItems,
    /// `uniqueItems` not supported.
    UnsupportedKeywordUniqueItems,
    /// `exclusiveMinimum` not supported.
    UnsupportedKeywordExclusiveMinimum,
    /// `exclusiveMaximum` not supported.
    UnsupportedKeywordExclusiveMaximum,
    /// `multipleOf` not supported.
    UnsupportedKeywordMultipleOf,
    /// `readOnly` not supported.
    UnsupportedKeywordReadOnly,
    /// `writeOnly` not supported.
    UnsupportedKeywordWriteOnly,
    /// `deprecated` not supported.
    UnsupportedKeywordDeprecated,
    /// `propertyNames` not supported.
    UnsupportedKeywordPropertyNames,
    /// `additionalItems` not supported.
    UnsupportedKeywordAdditionalItems,
    /// `optional` — recognized but ignored; required/optional from `required` only.
    UnsupportedKeywordOptional,
    /// Any key not in our known set.
    UnknownKeyword(String),
}

/// Wrapper holding all schema validation issues collected during validation.
#[derive(Debug, PartialEq, Eq)]
pub struct SchemaValidationError {
    /// All issues found; never empty.
    pub issues: Vec<SchemaValidationIssue>,
}

impl fmt::Display for SchemaValidationIssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg: &str = match self {
            Self::RootNotObject => "root schema must have type \"object\"",
            Self::RootMissingType => "root has no type key",
            Self::NoStructsToGenerate => "root object has no supported properties",
            Self::InvalidTypeValue => "type is not a string or array of strings",
            Self::TypeArrayNotSupported => "type array (multiple types) not supported",
            Self::NullTypeNotSupported => "type \"null\" not supported",
            Self::InvalidRequiredFormat => "required is not an array of strings",
            Self::RequiredPropertyNotInProperties => {
                "required references property not in properties"
            }
            Self::InvalidEnumFormat => "enum is not an array",
            Self::EnumEmpty => "enum is empty array",
            Self::EnumContainsNonStringValues => {
                "enum has non-string values; only string enums supported"
            }
            Self::InvalidItemsFormat => "items is not an object when type is array",
            Self::ArrayMissingItems => "type is array but items is missing",
            Self::InvalidDefaultValue => "default value invalid for type",
            Self::UnsupportedDefaultObject => "default is object (not supported)",
            Self::UnsupportedDefaultNonEmptyArray => "default is non-empty array (not supported)",
            Self::InvalidMinimumMaximum => "minimum/maximum must be number when present",
            Self::PropertyWithUnsupportedType => "property has unsupported type",
            Self::ArrayItemsUnsupportedType => "array items has unsupported type",
            Self::AdditionalPropertiesUnsupportedSchema => {
                "additionalProperties schema not supported"
            }
            Self::UnsupportedKeywordRef => "keyword $ref not supported",
            Self::UnsupportedKeywordDefs => "keyword $defs not supported",
            Self::UnsupportedKeywordDefinitions => "keyword definitions not supported",
            Self::UnsupportedKeywordMinLength => "keyword minLength not supported",
            Self::UnsupportedKeywordMaxLength => "keyword maxLength not supported",
            Self::UnsupportedKeywordPattern => "keyword pattern not supported",
            Self::UnsupportedKeywordOneOf => "keyword oneOf not supported",
            Self::UnsupportedKeywordAnyOf => "keyword anyOf not supported",
            Self::UnsupportedKeywordAllOf => "keyword allOf not supported",
            Self::UnsupportedKeywordId => "keyword $id not supported",
            Self::UnsupportedKeywordExamples => "keyword examples not supported",
            Self::UnsupportedKeywordConst => "keyword const not supported",
            Self::UnsupportedKeywordNot => "keyword not not supported",
            Self::UnsupportedKeywordMinProperties => "keyword minProperties not supported",
            Self::UnsupportedKeywordMaxProperties => "keyword maxProperties not supported",
            Self::UnsupportedKeywordMinItems => "keyword minItems not supported",
            Self::UnsupportedKeywordMaxItems => "keyword maxItems not supported",
            Self::UnsupportedKeywordUniqueItems => "keyword uniqueItems not supported",
            Self::UnsupportedKeywordExclusiveMinimum => "keyword exclusiveMinimum not supported",
            Self::UnsupportedKeywordExclusiveMaximum => "keyword exclusiveMaximum not supported",
            Self::UnsupportedKeywordMultipleOf => "keyword multipleOf not supported",
            Self::UnsupportedKeywordReadOnly => "keyword readOnly not supported",
            Self::UnsupportedKeywordWriteOnly => "keyword writeOnly not supported",
            Self::UnsupportedKeywordDeprecated => "keyword deprecated not supported",
            Self::UnsupportedKeywordPropertyNames => "keyword propertyNames not supported",
            Self::UnsupportedKeywordAdditionalItems => "keyword additionalItems not supported",
            Self::UnsupportedKeywordOptional => {
                "keyword optional not supported (use required array)"
            }
            Self::UnknownKeyword(key) => return write!(f, "unknown keyword: {key}"),
        };
        write!(f, "{msg}")
    }
}

impl fmt::Display for SchemaValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Schema validation failed with {} issue(s):",
            self.issues.len()
        )?;
        for issue in &self.issues {
            writeln!(f, "  {}: {}", issue.path, issue.kind)?;
        }
        Ok(())
    }
}

impl error::Error for SchemaValidationError {}

/// Error type for JSON Schema code generation operations.
#[derive(Debug)]
pub enum JsonSchemaGenError {
    /// Generic error with a message.
    GenericError(String),

    /// I/O error (e.g., reading schema file, writing output file).
    IoError(std::io::Error),

    /// JSON parsing error.
    JsonError(serde_json::Error),

    /// Schema validation failed (`deny_invalid_unknown_json_schema` enabled).
    SchemaValidation(SchemaValidationError),
}

impl error::Error for JsonSchemaGenError {}

impl fmt::Display for JsonSchemaGenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenericError(message) => write!(f, "{message}"),
            Self::IoError(io_error) => fmt::Display::fmt(io_error, f),
            Self::JsonError(json_error) => fmt::Display::fmt(json_error, f),
            Self::SchemaValidation(err) => fmt::Display::fmt(err, f),
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

impl From<SchemaValidationError> for JsonSchemaGenError {
    fn from(err: SchemaValidationError) -> Self {
        Self::SchemaValidation(err)
    }
}
