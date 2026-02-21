//! Public API for parsing JSON Schema with configurable settings.

use super::error::JsonSchemaParseError;
use super::json_schema::{
    DenyUnknownFieldsJsonSchema, JsonSchema, deny_unknown_fields_helper_to_schema,
};
use super::settings::JsonSchemaSettings;
use serde::Deserialize;
use std::collections::BTreeMap;

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

impl<'de> Deserialize<'de> for JsonSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct JsonSchemaHelper {
            #[serde(default, deserialize_with = "deserialize_type_optional")]
            #[serde(rename = "type")]
            type_: Option<String>,
            #[serde(default)]
            properties: Option<BTreeMap<String, JsonSchema>>,
            #[serde(default)]
            required: Option<Vec<String>>,
            #[serde(default)]
            title: Option<String>,
        }
        let h: JsonSchemaHelper = JsonSchemaHelper::deserialize(deserializer)?;
        Ok(JsonSchema {
            type_: h.type_,
            properties: h.properties.unwrap_or_default(),
            required: h.required,
            title: h.title,
        })
    }
}

/// Strict parse from string. Used by the public parser when `disallow_unknown_fields` is true.
pub(crate) fn parse_strict_str(json: &str) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema = serde_json::from_str(json)?;
    Ok(deny_unknown_fields_helper_to_schema(helper))
}

/// Strict parse from slice. Used by the public parser when `disallow_unknown_fields` is true.
pub(crate) fn parse_strict_slice(slice: &[u8]) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema =
        serde_json::from_slice(slice).map_err(JsonSchemaParseError::Serde)?;
    Ok(deny_unknown_fields_helper_to_schema(helper))
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
pub fn parse_schema(
    json: &str,
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, JsonSchemaParseError> {
    if settings.disallow_unknown_fields {
        parse_strict_str(json)
    } else {
        let schema: JsonSchema = serde_json::from_str(json)?;
        Ok(schema)
    }
}

/// Parse a JSON Schema from a byte slice with the given settings.
///
/// Same as [`parse_schema`] but takes bytes (e.g. from a file).
///
/// # Errors
///
/// Same as [`parse_schema`].
pub fn parse_schema_from_slice(
    slice: &[u8],
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, JsonSchemaParseError> {
    if settings.disallow_unknown_fields {
        parse_strict_slice(slice)
    } else {
        let schema: JsonSchema = serde_json::from_slice(slice)?;
        Ok(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonSchema, JsonSchemaSettings, parse_schema};
    use std::collections::BTreeMap;

    #[test]
    fn deserialize_simple_object_schema() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: None,
            title: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_with_required() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x"]}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "x".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m.insert(
                    "y".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
            title: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_ignores_unknown_keys() {
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_type_array_takes_first() {
        let json = r#"{"type":["string", "null"],"properties":{}}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_lenient_accepts_unknown_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_rejects_unknown_key_at_root() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{},"unknown":42}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
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
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_err());
    }

    #[test]
    fn parse_strict_accepts_only_known_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }
}
