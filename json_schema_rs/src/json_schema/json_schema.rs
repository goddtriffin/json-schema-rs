//! In-memory representation of JSON Schema for codegen.

use super::error::JsonSchemaParseError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Returns true when `required` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_required(v: &Option<Vec<String>>) -> bool {
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
}

/// Converts a strict (deny-unknown-fields) deserialized helper into the public [`JsonSchema`] model.
pub(crate) fn deny_unknown_fields_helper_to_schema(h: DenyUnknownFieldsJsonSchema) -> JsonSchema {
    let properties: BTreeMap<String, JsonSchema> = h
        .properties
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
        .collect();
    JsonSchema {
        type_: h.type_,
        properties,
        required: h.required,
        title: h.title,
    }
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct JsonSchema {
    /// Schema type; `object` and `string` drive codegen; others are ignored.
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

    /// Returns true if the given property name is required at this object level.
    #[must_use]
    pub(crate) fn is_required(&self, name: &str) -> bool {
        self.required
            .as_ref()
            .is_some_and(|r| r.iter().any(|s| s == name))
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
}
