//! In-memory representation of JSON Schema for codegen.

use super::error::JsonSchemaParseError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        deserialize_with = "super::parser::deserialize_type_optional_deny_unknown_fields",
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
    use crate::json_schema::{JsonSchemaSettings, parse_schema, parse_schema_from_slice};
    use std::collections::BTreeMap;

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
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"string\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn round_trip_via_vec_u8() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let bytes: Vec<u8> = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema_from_slice(&bytes, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
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
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"number\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
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
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"array","items":{"type":"string"}}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_unique_items_true() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<bool> = Some(true);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_false() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":false}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<bool> = Some(false);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_absent() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<bool> = None;
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_unique_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_min_length() {
        let json = r#"{"type":"string","minLength":5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(Some(5), parsed.min_length);
    }

    #[test]
    fn parse_max_length() {
        let json = r#"{"type":"string","maxLength":20}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(Some(20), parsed.max_length);
    }

    #[test]
    fn parse_min_length_max_length_both() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(Some(2), parsed.min_length);
        assert_eq!(Some(50), parsed.max_length);
    }

    #[test]
    fn parse_min_length_absent() {
        let json = r#"{"type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(None, parsed.min_length);
    }

    #[test]
    fn parse_max_length_absent() {
        let json = r#"{"type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(None, parsed.max_length);
    }

    #[test]
    fn round_trip_parse_serialize_parse_with_min_length_max_length() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> = Some("uuid".to_string());
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_format_absent() {
        let json = r#"{"type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> = None;
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed, reparsed);
    }
}
