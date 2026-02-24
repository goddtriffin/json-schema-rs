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

/// Schema helper with `deny_unknown_fields`: same shape as our schema model but with `#[serde(deny_unknown_fields)]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DenyUnknownFieldsJsonSchema {
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
    #[serde(default, rename = "enum")]
    pub(crate) enum_values: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub(crate) items: Option<Box<DenyUnknownFieldsJsonSchema>>,
    #[serde(default)]
    pub(crate) minimum: Option<f64>,
    #[serde(default)]
    pub(crate) maximum: Option<f64>,
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
    JsonSchema {
        type_: h.type_,
        properties,
        required: h.required,
        title: h.title,
        description: h.description,
        enum_values: h.enum_values,
        items,
        minimum: h.minimum,
        maximum: h.maximum,
    }
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JsonSchema {
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

    /// Allowed values for the instance (JSON Schema `enum`). When present and non-empty, instance must equal one of these. Codegen uses only string-only enums.
    #[serde(rename = "enum", skip_serializing_if = "skip_enum_values")]
    pub enum_values: Option<Vec<serde_json::Value>>,

    /// Schema for all array elements (when type is "array"). Single-schema form only; tuple-typing (array of schemas) not supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// Inclusive lower bound for numeric instances (integer or number). Used for validation and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,

    /// Inclusive upper bound for numeric instances (integer or number). Used for validation and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
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
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: Some("Root".to_string()),
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"object","title":"Root"}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
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
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
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
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"number\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn is_string_enum() {
        let no_enum: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        };
        assert!(!no_enum.is_string_enum());
        let empty_enum: JsonSchema = JsonSchema {
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![]),
            items: None,
            minimum: None,
            maximum: None,
        };
        assert!(!empty_enum.is_string_enum());
        let string_enum: JsonSchema = JsonSchema {
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            items: None,
            minimum: None,
            maximum: None,
        };
        assert!(string_enum.is_string_enum());
        let mixed_enum: JsonSchema = JsonSchema {
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::Number(42_i64.into()),
            ]),
            items: None,
            minimum: None,
            maximum: None,
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
                    type_: Some("string".to_string()),
                    properties: BTreeMap::new(),
                    required: None,
                    title: None,
                    description: None,
                    enum_values: None,
                    items: None,
                    minimum: None,
                    maximum: None,
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
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            minimum: None,
            maximum: None,
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
}
