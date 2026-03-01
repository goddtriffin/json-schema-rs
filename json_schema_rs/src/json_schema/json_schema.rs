//! In-memory representation of JSON Schema for codegen.

use super::error::{JsonSchemaParseError, JsonSchemaParseResult};
use super::settings::JsonSchemaSettings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;

/// Returns true when `required` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_required(v: &Option<Vec<String>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `enum_values` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_enum_values(v: &Option<Vec<serde_json::Value>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `all_of` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_all_of(v: &Option<Vec<JsonSchema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `any_of` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_any_of(v: &Option<Vec<JsonSchema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// JSON Schema type keyword: either a single type string or an array of types (draft 2020-12).
/// First type in the array is used; codegen uses `object` and `string`.
fn deserialize_type_optional<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TypeOrArray {
        Single(String),
        Array(Vec<String>),
    }
    let value: TypeOrArray = Deserialize::deserialize(deserializer)?;
    let first = match value {
        TypeOrArray::Single(s) => Some(s),
        TypeOrArray::Array(a) => a.into_iter().next(),
    };
    Ok(first)
}

/// Type keyword deserializer for [`DenyUnknownFieldsJsonSchema`]: single string or array (takes first).
pub(crate) fn deserialize_type_optional_deny_unknown_fields<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TypeOrArray {
        Single(String),
        Array(Vec<String>),
    }
    let value: TypeOrArray = Deserialize::deserialize(deserializer)?;
    let first = match value {
        TypeOrArray::Single(s) => Some(s),
        TypeOrArray::Array(a) => a.into_iter().next(),
    };
    Ok(first)
}

/// Schema helper with `deny_unknown_fields`: same shape as our schema model but with `#[serde(deny_unknown_fields)]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DenyUnknownFieldsJsonSchema {
    #[serde(default, rename = "$schema")]
    pub(crate) schema: Option<String>,
    #[serde(default, rename = "$id")]
    pub(crate) id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_type_optional_deny_unknown_fields",
        rename = "type"
    )]
    pub(crate) type_: Option<String>,
    #[serde(default)]
    pub(crate) properties: Option<BTreeMap<String, DenyUnknownFieldsJsonSchema>>,
    #[serde(default)]
    pub(crate) required: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default, rename = "$comment")]
    pub(crate) comment: Option<String>,
    #[serde(default, rename = "enum")]
    pub(crate) enum_values: Option<Vec<serde_json::Value>>,
    #[serde(default, rename = "const")]
    pub(crate) const_value: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) items: Option<Box<DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "uniqueItems")]
    pub(crate) unique_items: Option<bool>,
    #[serde(default, rename = "minItems")]
    pub(crate) min_items: Option<u64>,
    #[serde(default, rename = "maxItems")]
    pub(crate) max_items: Option<u64>,
    #[serde(default)]
    pub(crate) minimum: Option<f64>,
    #[serde(default)]
    pub(crate) maximum: Option<f64>,
    #[serde(default, rename = "minLength")]
    pub(crate) min_length: Option<u64>,
    #[serde(default, rename = "maxLength")]
    pub(crate) max_length: Option<u64>,
    #[serde(default)]
    pub(crate) format: Option<String>,
    #[serde(default, rename = "allOf")]
    pub(crate) all_of: Option<Vec<DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "anyOf")]
    pub(crate) any_of: Option<Vec<DenyUnknownFieldsJsonSchema>>,
}

/// Converts a strict (deny-unknown-fields) deserialized helper into the public [`JsonSchema`] model.
pub(crate) fn deny_unknown_fields_helper_to_schema(h: DenyUnknownFieldsJsonSchema) -> JsonSchema {
    let properties: BTreeMap<String, JsonSchema> = h
        .properties
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
        .collect();
    let items: Option<Box<JsonSchema>> = h
        .items
        .map(|b| Box::new(deny_unknown_fields_helper_to_schema(*b)));
    let all_of: Option<Vec<JsonSchema>> = h.all_of.map(|v| {
        v.into_iter()
            .map(deny_unknown_fields_helper_to_schema)
            .collect()
    });
    let any_of: Option<Vec<JsonSchema>> = h.any_of.map(|v| {
        v.into_iter()
            .map(deny_unknown_fields_helper_to_schema)
            .collect()
    });
    JsonSchema {
        schema: h.schema,
        id: h.id,
        type_: h.type_,
        properties,
        required: h.required,
        title: h.title,
        description: h.description,
        comment: h.comment,
        enum_values: h.enum_values,
        const_value: h.const_value,
        items,
        unique_items: h.unique_items,
        min_items: h.min_items,
        max_items: h.max_items,
        minimum: h.minimum,
        maximum: h.maximum,
        min_length: h.min_length,
        max_length: h.max_length,
        format: h.format,
        all_of,
        any_of,
    }
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JsonSchema {
    /// Declares the JSON Schema dialect (meta-schema URI). When present, stored and round-tripped; used for draft inference when no explicit spec version is set.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Unique identifier for the schema (typically a URI). Stored and round-tripped. We support only `$id`; draft-04 `id` is not accepted or emitted. Reserved for future use as base URI when `$ref` resolution is implemented.
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Schema type; `object`, `string`, `integer`, and `number` drive codegen; others are ignored.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,

    /// Object properties (only when type is "object"). Default empty; use `BTreeMap` for stable ordering.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, JsonSchema>,

    /// Required property names at this object level. When absent, all properties are optional.
    #[serde(skip_serializing_if = "skip_required")]
    pub required: Option<Vec<String>>,

    /// Used for struct naming when present (`PascalCase`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Human-readable description. Codegen emits it as Rust doc comments (struct, enum, or field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Schema-only comment (draft-07+). Stored and round-tripped; not used for validation or user-facing docs.
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Allowed values for the instance (JSON Schema `enum`). When present and non-empty, instance must equal one of these. Codegen uses only string-only enums.
    #[serde(rename = "enum", skip_serializing_if = "skip_enum_values")]
    pub enum_values: Option<Vec<serde_json::Value>>,

    /// Single allowed value for the instance (JSON Schema `const`, draft-06+). When present, instance must equal this value. Codegen uses only string const (single-variant enum).
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<serde_json::Value>,

    /// Schema for all array elements (when type is "array"). Single-schema form only; tuple-typing (array of schemas) not supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// When true, all array elements must be unique (JSON equality). Array-only; used by validator and codegen (`HashSet` when applicable).
    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,

    /// Minimum number of array elements. Array-only; used by validator and codegen (emitted as field attribute).
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,

    /// Maximum number of array elements. Array-only; used by validator and codegen (emitted as field attribute).
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,

    /// Inclusive lower bound for numeric instances (integer or number). Used for validation and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Inclusive upper bound for numeric instances (integer or number). Used for validation and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,

    /// Minimum string length in Unicode code points. String-only; used by validator and codegen.
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,

    /// Maximum string length in Unicode code points. String-only; used by validator and codegen.
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,

    /// Format annotation for string instances (e.g. "uuid"). When the `uuid` feature is enabled,
    /// `"uuid"` drives type selection in codegen and format validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// allOf: instance must validate against every subschema. Stored as-is at ingestion; validator validates each; codegen merges on-the-fly. Not preserved on round-trip or reverse codegen.
    #[serde(rename = "allOf", skip_serializing_if = "skip_all_of")]
    pub all_of: Option<Vec<JsonSchema>>,

    /// anyOf: instance must validate against at least one subschema. Stored as-is at ingestion; validator and codegen (union enum) use it. Not emitted by reverse codegen.
    #[serde(rename = "anyOf", skip_serializing_if = "skip_any_of")]
    pub any_of: Option<Vec<JsonSchema>>,
}

impl<'de> Deserialize<'de> for JsonSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct JsonSchemaHelper {
            #[serde(default, rename = "$schema")]
            schema: Option<String>,
            #[serde(default, rename = "$id")]
            id: Option<String>,
            #[serde(default, deserialize_with = "deserialize_type_optional")]
            #[serde(rename = "type")]
            type_: Option<String>,
            #[serde(default)]
            properties: Option<BTreeMap<String, JsonSchema>>,
            #[serde(default)]
            required: Option<Vec<String>>,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, rename = "$comment")]
            comment: Option<String>,
            #[serde(default, rename = "enum")]
            enum_values: Option<Vec<serde_json::Value>>,
            #[serde(default, rename = "const")]
            const_value: Option<serde_json::Value>,
            #[serde(default)]
            items: Option<Box<JsonSchema>>,
            #[serde(default, rename = "uniqueItems")]
            unique_items: Option<bool>,
            #[serde(default, rename = "minItems")]
            min_items: Option<u64>,
            #[serde(default, rename = "maxItems")]
            max_items: Option<u64>,
            #[serde(default)]
            minimum: Option<f64>,
            #[serde(default)]
            maximum: Option<f64>,
            #[serde(default, rename = "minLength")]
            min_length: Option<u64>,
            #[serde(default, rename = "maxLength")]
            max_length: Option<u64>,
            #[serde(default)]
            format: Option<String>,
            #[serde(default, rename = "allOf")]
            all_of: Option<Vec<JsonSchema>>,
            #[serde(default, rename = "anyOf")]
            any_of: Option<Vec<JsonSchema>>,
        }
        let h: JsonSchemaHelper = JsonSchemaHelper::deserialize(deserializer)?;
        Ok(JsonSchema {
            schema: h.schema,
            id: h.id,
            type_: h.type_,
            properties: h.properties.unwrap_or_default(),
            required: h.required,
            title: h.title,
            description: h.description,
            comment: h.comment,
            enum_values: h.enum_values,
            const_value: h.const_value,
            items: h.items,
            unique_items: h.unique_items,
            min_items: h.min_items,
            max_items: h.max_items,
            minimum: h.minimum,
            maximum: h.maximum,
            min_length: h.min_length,
            max_length: h.max_length,
            format: h.format,
            all_of: h.all_of,
            any_of: h.any_of,
        })
    }
}

fn parse_strict_str(json: &str) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema = serde_json::from_str(json)?;
    Ok(deny_unknown_fields_helper_to_schema(helper))
}

fn parse_strict_slice(slice: &[u8]) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema =
        serde_json::from_slice(slice).map_err(JsonSchemaParseError::Serde)?;
    Ok(deny_unknown_fields_helper_to_schema(helper))
}

fn parse_strict_value(value: &serde_json::Value) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema =
        serde_json::from_value(value.clone()).map_err(JsonSchemaParseError::from)?;
    Ok(deny_unknown_fields_helper_to_schema(helper))
}

impl JsonSchema {
    /// Returns true if this schema is an object with properties (for codegen).
    #[must_use]
    pub(crate) fn is_object_with_properties(&self) -> bool {
        self.type_.as_deref() == Some("object") && !self.properties.is_empty()
    }

    /// Returns true if this schema is type "string".
    #[must_use]
    pub(crate) fn is_string(&self) -> bool {
        self.type_.as_deref() == Some("string")
    }

    /// Returns true if this schema is type "integer".
    #[must_use]
    pub(crate) fn is_integer(&self) -> bool {
        self.type_.as_deref() == Some("integer")
    }

    /// Returns true if this schema is type "number".
    #[must_use]
    pub(crate) fn is_number(&self) -> bool {
        self.type_.as_deref() == Some("number")
    }

    /// Returns true if this schema is type "array".
    #[must_use]
    pub(crate) fn is_array(&self) -> bool {
        self.type_.as_deref() == Some("array")
    }

    /// Returns true if this schema is type "array" and has an items schema (single-schema form).
    #[must_use]
    pub(crate) fn is_array_with_items(&self) -> bool {
        self.is_array() && self.items.is_some()
    }

    /// Returns true if the given property name is required at this object level.
    #[must_use]
    pub(crate) fn is_required(&self, name: &str) -> bool {
        self.required
            .as_ref()
            .is_some_and(|r| r.iter().any(|s| s == name))
    }

    /// Returns true if this schema has a string-only enum (non-empty, all elements are strings). Used for codegen to emit a Rust enum.
    #[must_use]
    pub(crate) fn is_string_enum(&self) -> bool {
        self.enum_values
            .as_ref()
            .is_some_and(|v| !v.is_empty() && v.iter().all(serde_json::Value::is_string))
    }

    /// Returns true when this schema has a string const (single allowed string value). Used for codegen to treat as single-value enum.
    #[must_use]
    pub(crate) fn is_string_const(&self) -> bool {
        self.const_value
            .as_ref()
            .is_some_and(serde_json::Value::is_string)
    }

    /// Parse a JSON Schema from a string with the given settings.
    ///
    /// When [`JsonSchemaSettings::disallow_unknown_fields`] is `false`, unknown keys
    /// are ignored (lenient). When `true`, any unknown key causes an error (strict).
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Serde`] on invalid JSON or type mismatch.
    /// Returns [`JsonSchemaParseError::UnknownField`] when strict and an unknown key is present.
    pub fn new_from_str(json: &str, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<Self> {
        if settings.disallow_unknown_fields {
            parse_strict_str(json)
        } else {
            let schema: JsonSchema = serde_json::from_str(json)?;
            Ok(schema)
        }
    }

    /// Parse a JSON Schema from a byte slice with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but takes bytes (e.g. from a file).
    ///
    /// # Errors
    ///
    /// Same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_slice(
        slice: &[u8],
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        if settings.disallow_unknown_fields {
            parse_strict_slice(slice)
        } else {
            let schema: JsonSchema = serde_json::from_slice(slice)?;
            Ok(schema)
        }
    }

    /// Parse a JSON Schema from an already-parsed [`serde_json::Value`] with the given settings.
    ///
    /// Same semantics as [`new_from_str`](Self::new_from_str); takes a value to avoid string round-trips
    /// when the schema is already loaded as JSON (e.g. from a test case or API response).
    ///
    /// # Errors
    ///
    /// Same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_serde_value(
        value: &serde_json::Value,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        if settings.disallow_unknown_fields {
            parse_strict_value(value)
        } else {
            let schema: JsonSchema = serde_json::from_value(value.clone())?;
            Ok(schema)
        }
    }

    /// Parse a JSON Schema from a reader with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but reads from a reader. I/O errors are
    /// returned as [`JsonSchemaParseError::Io`].
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on read failure.
    /// Otherwise same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_reader<R: Read>(
        reader: R,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let mut buf: Vec<u8> = Vec::new();
        let mut reader = reader;
        reader
            .read_to_end(&mut buf)
            .map_err(JsonSchemaParseError::Io)?;
        Self::new_from_slice(&buf, settings)
    }

    /// Parse a JSON Schema from a file path with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but reads from a file. I/O errors (e.g. file not found)
    /// are returned as [`JsonSchemaParseError::Io`].
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on open or read failure.
    /// Otherwise same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_path<P: AsRef<std::path::Path>>(
        path: P,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let f: std::fs::File = std::fs::File::open(path.as_ref())?;
        Self::new_from_reader(f, settings)
    }
}

// TryFrom: parse into JsonSchema with default settings

/// Parses a JSON Schema from a string using default [`JsonSchemaSettings`](super::settings::JsonSchemaSettings).
/// For custom settings, use [`JsonSchema::new_from_str`](Self::new_from_str).
impl TryFrom<&str> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value, &settings)
    }
}

/// Parses a JSON Schema from an owned string using default settings.
/// For custom settings, use [`JsonSchema::new_from_str`](Self::new_from_str).
impl TryFrom<String> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value.as_str(), &settings)
    }
}

/// Parses a JSON Schema from a byte slice using default settings.
/// For custom settings, use [`JsonSchema::new_from_slice`](Self::new_from_slice).
impl TryFrom<&[u8]> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_slice(value, &settings)
    }
}

/// Parses a JSON Schema from an already-parsed [`serde_json::Value`] using default settings.
/// For custom settings, use [`JsonSchema::new_from_serde_value`](Self::new_from_serde_value).
impl TryFrom<&serde_json::Value> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_serde_value(value, &settings)
    }
}

/// Parses a JSON Schema from an open file using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_reader`](Self::new_from_reader).
impl TryFrom<std::fs::File> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: std::fs::File) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_reader(value, &settings)
    }
}

/// Parses a JSON Schema from a file path using default settings. I/O errors (e.g. file not found) are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<&std::path::Path> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(value, &settings)
    }
}

/// Parses a JSON Schema from an owned path using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<std::path::PathBuf> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: std::path::PathBuf) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(&value, &settings)
    }
}

impl TryFrom<&JsonSchema> for String {
    type Error = JsonSchemaParseError;

    fn try_from(schema: &JsonSchema) -> Result<String, Self::Error> {
        serde_json::to_string(schema).map_err(Into::into)
    }
}

impl TryFrom<&JsonSchema> for Vec<u8> {
    type Error = JsonSchemaParseError;

    fn try_from(schema: &JsonSchema) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(schema).map_err(Into::into)
    }
}

impl TryFrom<JsonSchema> for String {
    type Error = JsonSchemaParseError;

    fn try_from(schema: JsonSchema) -> Result<String, Self::Error> {
        serde_json::to_string(&schema).map_err(Into::into)
    }
}

impl TryFrom<JsonSchema> for Vec<u8> {
    type Error = JsonSchemaParseError;

    fn try_from(schema: JsonSchema) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(&schema).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::JsonSchema;
    use crate::json_schema::{
        JsonSchemaParseError, JsonSchemaSettings, SpecVersion, resolved_spec_version,
    };
    use std::collections::BTreeMap;
    use std::io;

    #[test]
    fn try_from_schema_to_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: Some("Root".to_string()),
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"object","title":"Root"}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_with_comment_serializes_dollar_comment() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: Some("Created by John Doe".to_string()),
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: String = (&schema).try_into().expect("serialize");
        let expected_contains = r#""$comment":"Created by John Doe""#;
        assert!(
            actual.contains(expected_contains),
            "serialized schema should contain $comment; got: {actual}"
        );
    }

    #[test]
    fn try_from_schema_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"string\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn round_trip_via_vec_u8() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let bytes: Vec<u8> = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(bytes.as_slice()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    // TryFrom: parse into JsonSchema with default settings

    #[test]
    fn try_from_str_success() {
        let json: &str = r#"{"type":"object","properties":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_str_invalid_json() {
        let json: &str = "not json";
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(json);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn try_from_string_success() {
        let json: String = r#"{"type":"string"}"#.to_string();
        let actual: JsonSchema = JsonSchema::try_from(json).expect("try_from");
        let expected_type: Option<&str> = Some("string");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_slice_success() {
        let bytes: &[u8] = r#"{"type":"array","items":{"type":"string"}}"#.as_bytes();
        let actual: JsonSchema = JsonSchema::try_from(bytes).expect("try_from");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_serde_value_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "integer"}}});
        let actual: JsonSchema = JsonSchema::try_from(&value).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(&value);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_file_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"object","properties":{}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let f: std::fs::File = std::fs::File::open(&schema_path).expect("open");
        let actual: JsonSchema = JsonSchema::try_from(f).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_file_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let f: std::fs::File = std::fs::File::open(&schema_path).expect("open");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(f);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn try_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("try_from");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path: std::path::PathBuf = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn try_from_path_buf_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"number"}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path).expect("try_from");
        let expected_type: Option<&str> = Some("number");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn is_object_with_properties() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_object_with_properties(),
            {
                s.type_ = Some("object".to_string());
                s.is_object_with_properties()
            },
            {
                s.properties.insert("x".to_string(), JsonSchema::default());
                s.is_object_with_properties()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string() {
        let mut s = JsonSchema::default();
        let actual = [s.is_string(), {
            s.type_ = Some("string".to_string());
            s.is_string()
        }];
        let expected = [false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_integer(),
            {
                s.type_ = Some("string".to_string());
                s.is_integer()
            },
            {
                s.type_ = Some("integer".to_string());
                s.is_integer()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_integer_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"integer\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn is_number() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_number(),
            {
                s.type_ = Some("string".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("integer".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("object".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("number".to_string());
                s.is_number()
            },
        ];
        let expected = [false, false, false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_number_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"number\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    #[expect(clippy::too_many_lines)]
    fn is_string_enum() {
        let no_enum: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        assert!(!no_enum.is_string_enum());
        let empty_enum: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        assert!(!empty_enum.is_string_enum());
        let string_enum: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        assert!(string_enum.is_string_enum());
        let mixed_enum: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::Number(42_i64.into()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        assert!(!mixed_enum.is_string_enum());
    }

    #[test]
    fn is_array() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_array(),
            {
                s.type_ = Some("string".to_string());
                s.is_array()
            },
            {
                s.type_ = Some("array".to_string());
                s.is_array()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_array_with_items() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_array_with_items(),
            {
                s.type_ = Some("array".to_string());
                s.is_array_with_items()
            },
            {
                s.items = Some(Box::new(JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    properties: BTreeMap::new(),
                    required: None,
                    title: None,
                    description: None,
                    comment: None,
                    enum_values: None,
                    const_value: None,
                    items: None,
                    unique_items: None,
                    min_items: None,
                    max_items: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                    format: None,
                    all_of: None,
                    any_of: None,
                }));
                s.is_array_with_items()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_array_with_items_to_string() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"array","items":{"type":"string"}}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_unique_items_true() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = Some(true);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_false() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":false}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = Some(false);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_absent() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = None;
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_unique_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_min_length() {
        let json = r#"{"type":"string","minLength":5}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(5), parsed.min_length);
    }

    #[test]
    fn parse_max_length() {
        let json = r#"{"type":"string","maxLength":20}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(20), parsed.max_length);
    }

    #[test]
    fn parse_min_length_max_length_both() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(2), parsed.min_length);
        assert_eq!(Some(50), parsed.max_length);
    }

    #[test]
    fn parse_min_length_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.min_length);
    }

    #[test]
    fn parse_max_length_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.max_length);
    }

    #[test]
    fn round_trip_parse_serialize_parse_with_min_length_max_length() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("uuid".to_string());
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_format_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    // --- tests merged from parser ---

    #[test]
    fn deserialize_simple_object_schema() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        comment: None,
                        enum_values: None,
                        const_value: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
                        all_of: None,
                        any_of: None,
                    },
                );
                m
            },
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_present() {
        let json = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props_a = BTreeMap::new();
        props_a.insert(
            "a".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let mut props_b = BTreeMap::new();
        props_b.insert(
            "b".to_string(),
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![
            JsonSchema {
                type_: Some("object".to_string()),
                properties: props_a,
                ..Default::default()
            },
            JsonSchema {
                type_: Some("object".to_string()),
                properties: props_b,
                ..Default::default()
            },
        ]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_empty_array() {
        let json = r#"{"allOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_single_subschema() {
        let json = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props = BTreeMap::new();
        props.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![JsonSchema {
            type_: Some("object".to_string()),
            properties: props,
            ..Default::default()
        }]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_present() {
        let json = r#"{"anyOf":[{"type":"string"},{"type":"integer"}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        ]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_empty_array() {
        let json = r#"{"anyOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_single_subschema() {
        let json = r#"{"anyOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props = BTreeMap::new();
        props.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![JsonSchema {
            type_: Some("object".to_string()),
            properties: props,
            ..Default::default()
        }]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_with_required() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x"]}"#;
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "x".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        comment: None,
                        enum_values: None,
                        const_value: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
                        all_of: None,
                        any_of: None,
                    },
                );
                m.insert(
                    "y".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        comment: None,
                        enum_values: None,
                        const_value: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
                        all_of: None,
                        any_of: None,
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_ignores_unknown_keys() {
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let expected: JsonSchema = JsonSchema {
            schema: Some("https://example.com".to_string()),
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_type_array_takes_first() {
        let json = r#"{"type":["string", "null"],"properties":{}}"#;
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items present").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
    }

    #[test]
    fn parse_lenient_accepts_unknown_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_rejects_unknown_key_at_root() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{},"unknown":42}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown") || msg.contains("Unknown"),
            "error message should mention unknown field: {msg}"
        );
    }

    #[test]
    fn parse_strict_rejects_unknown_key_in_nested_properties() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"nested":{"type":"object","properties":{},"bad":1}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_err());
    }

    #[test]
    fn parse_strict_accepts_only_known_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_with_schema_uri_preserves_schema() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> =
            Some("https://json-schema.org/draft/2020-12/schema".to_string());
        assert_eq!(expected, parsed.schema);
    }

    #[test]
    fn parse_without_schema_uri_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.schema);
    }

    #[test]
    fn parse_without_id_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn parse_with_id_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("http://example.com/schema".to_string());
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn round_trip_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse");
        assert_eq!(parsed.id, reparsed.id);
    }

    #[test]
    fn parse_strict_accepts_id_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("http://example.com/schema".to_string()), parsed.id);
    }

    #[test]
    fn parse_strict_accepts_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(
            Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            parsed.schema
        );
    }

    #[test]
    fn round_trip_preserves_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        assert!(
            serialized.contains("\"$schema\""),
            "serialized output should contain $schema key: {serialized}"
        );
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed.schema, reparsed.schema);
    }

    #[test]
    fn resolved_spec_version_infers_2020_12_from_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_absent() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_unknown_uri() {
        let json =
            r#"{"$schema":"https://unknown.example/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_dollar_comment_preserved() {
        let json = r#"{"type":"object","properties":{},"$comment":"Created by John Doe"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("Created by John Doe".to_string());
        assert_eq!(expected, parsed.comment);
    }

    #[test]
    fn parse_without_comment_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.comment);
    }

    #[test]
    fn round_trip_preserves_dollar_comment() {
        let json = r#"{"type":"object","properties":{"country":{"type":"string","$comment":"TODO: add enum"}},"$comment":"Root schema"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed.comment, reparsed.comment);
        let country_schema: &JsonSchema = reparsed.properties.get("country").expect("country");
        assert_eq!(Some("TODO: add enum".to_string()), country_schema.comment);
    }

    #[test]
    fn parse_strict_accepts_dollar_comment() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"$comment":"Note"}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("Note".to_string()), parsed.comment);
    }

    #[test]
    fn deserialize_schema_with_enum() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let status_schema: &JsonSchema = parsed.properties.get("status").expect("status property");
        let expected_enum: Vec<serde_json::Value> = vec![
            serde_json::Value::String("open".to_string()),
            serde_json::Value::String("closed".to_string()),
        ];
        let actual: Option<&Vec<serde_json::Value>> = status_schema.enum_values.as_ref();
        assert_eq!(Some(&expected_enum), actual);
    }

    #[test]
    fn parse_strict_accepts_enum_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"x":{"enum":["a","b"]}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_const_string() {
        let json = r#"{"const":"only"}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> =
            Some(serde_json::Value::String("only".to_string()));
        let actual: Option<&serde_json::Value> = schema.const_value.as_ref();
        assert_eq!(expected.as_ref(), actual);
    }

    #[test]
    fn parse_const_number() {
        let json = r#"{"const":42}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> = Some(serde_json::Value::Number(42_i64.into()));
        let actual: Option<&serde_json::Value> = schema.const_value.as_ref();
        assert_eq!(expected.as_ref(), actual);
    }

    #[test]
    fn parse_const_round_trip() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::String("x".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let json: String = serde_json::to_string(&schema).expect("serialize");
        let parsed: JsonSchema = serde_json::from_str(&json).expect("parse");
        assert_eq!(schema.const_value, parsed.const_value);
    }

    #[test]
    fn parse_strict_accepts_const_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"x":{"const":"fixed"}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn is_string_const_true_when_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::String("a".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let expected: bool = true;
        let actual: bool = schema.is_string_const();
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string_const_false_when_non_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::Number(1_i64.into())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
            any_of: None,
        };
        let expected: bool = false;
        let actual: bool = schema.is_string_const();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_accepts_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_unique_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_min_items_max_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected_min: Option<u64> = Some(2);
        let expected_max: Option<u64> = Some(5);
        assert_eq!(expected_min, actual.min_items);
        assert_eq!(expected_max, actual.max_items);
    }

    #[test]
    fn parse_strict_accepts_min_items_max_items() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":10}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn deserialize_integer_with_minimum_and_maximum() {
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected_minimum: Option<f64> = Some(0.0);
        let expected_maximum: Option<f64> = Some(255.0);
        assert_eq!(expected_minimum, actual.minimum);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("integer"));
    }

    #[test]
    fn deserialize_integer_with_minimum_only() {
        let json = r#"{"type":"integer","minimum":-100}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<f64> = Some(-100.0);
        assert_eq!(expected, actual.minimum);
        assert_eq!(None, actual.maximum);
    }

    #[test]
    fn deserialize_number_with_minimum_and_maximum_float() {
        let json = r#"{"type":"number","minimum":0.5,"maximum":100.5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected_minimum: Option<f64> = Some(0.5);
        let expected_maximum: Option<f64> = Some(100.5);
        assert_eq!(expected_minimum, actual.minimum);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("number"));
    }

    #[test]
    fn deserialize_integer_with_maximum_only() {
        let json = r#"{"type":"integer","maximum":100}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, actual.minimum);
        let expected_maximum: Option<f64> = Some(100.0);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("integer"));
    }

    #[test]
    fn deserialize_number_with_maximum_only() {
        let json = r#"{"type":"number","maximum":99.5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, actual.minimum);
        let expected_maximum: Option<f64> = Some(99.5);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("number"));
    }

    #[test]
    fn parse_strict_accepts_minimum_and_maximum() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_format_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"string","format":"uuid"}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_schema_from_serde_value_lenient_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let actual: JsonSchema = JsonSchema::try_from(&value).expect("parse");
        let expected: JsonSchema =
            JsonSchema::try_from(r#"{"type":"object","properties":{"a":{"type":"string"}}}"#)
                .expect("reference parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_schema_from_serde_value_strict_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {}, "title": "Root"});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: JsonSchema =
            JsonSchema::new_from_serde_value(&value, &settings).expect("parse");
        let expected: JsonSchema = JsonSchema::new_from_str(
            r#"{"type":"object","properties":{},"title":"Root"}"#,
            &settings,
        )
        .expect("reference parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_schema_from_serde_value_strict_rejects_unknown_key() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {}, "unknown": 42});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_serde_value(&value, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn parse_schema_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(&value);
        let actual_is_err: bool = result.is_err();
        assert!(actual_is_err, "string value should fail to parse as schema");
    }

    #[test]
    fn parse_schema_from_serde_value_round_trip_with_new_from_str() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"]}"#;
        let from_str: JsonSchema = JsonSchema::try_from(json).expect("parse from str");
        let serialized: String = (&from_str).try_into().expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("str to value");
        let from_value: JsonSchema = JsonSchema::try_from(&value).expect("parse from value");
        assert_eq!(from_str, from_value);
    }

    #[test]
    fn parse_schema_from_reader_success() {
        let bytes: &[u8] = r#"{"type":"object","properties":{}}"#.as_bytes();
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(bytes);
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let actual: JsonSchema = JsonSchema::new_from_reader(reader, &settings).expect("parse");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_reader_invalid_json() {
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(b"not json");
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    struct FailingReader;

    impl io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("test"))
        }
    }

    #[test]
    fn parse_schema_from_reader_io_error() {
        let reader: FailingReader = FailingReader;
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn parse_schema_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("parse");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn parse_schema_from_path_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(schema_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }
}
