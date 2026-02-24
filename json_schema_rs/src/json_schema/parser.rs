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
            #[serde(default)]
            description: Option<String>,
            #[serde(default, rename = "enum")]
            enum_values: Option<Vec<serde_json::Value>>,
            #[serde(default)]
            items: Option<Box<JsonSchema>>,
            #[serde(default)]
            minimum: Option<f64>,
            #[serde(default)]
            maximum: Option<f64>,
        }
        let h: JsonSchemaHelper = JsonSchemaHelper::deserialize(deserializer)?;
        Ok(JsonSchema {
            type_: h.type_,
            properties: h.properties.unwrap_or_default(),
            required: h.required,
            title: h.title,
            description: h.description,
            enum_values: h.enum_values,
            items: h.items,
            minimum: h.minimum,
            maximum: h.maximum,
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
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        enum_values: None,
                        items: None,
                        minimum: None,
                        maximum: None,
                    },
                );
                m
            },
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
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
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        enum_values: None,
                        items: None,
                        minimum: None,
                        maximum: None,
                    },
                );
                m.insert(
                    "y".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        properties: std::collections::BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        enum_values: None,
                        items: None,
                        minimum: None,
                        maximum: None,
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
            title: None,
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
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
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
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
            description: None,
            enum_values: None,
            items: None,
            minimum: None,
            maximum: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items present").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
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

    #[test]
    fn deserialize_schema_with_enum() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
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
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn deserialize_integer_with_minimum_and_maximum() {
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected_minimum: Option<f64> = Some(0.0);
        let expected_maximum: Option<f64> = Some(255.0);
        assert_eq!(expected_minimum, actual.minimum);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("integer"));
    }

    #[test]
    fn deserialize_integer_with_minimum_only() {
        let json = r#"{"type":"integer","minimum":-100}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<f64> = Some(-100.0);
        assert_eq!(expected, actual.minimum);
        assert_eq!(None, actual.maximum);
    }

    #[test]
    fn deserialize_number_with_minimum_and_maximum_float() {
        let json = r#"{"type":"number","minimum":0.5,"maximum":100.5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected_minimum: Option<f64> = Some(0.5);
        let expected_maximum: Option<f64> = Some(100.5);
        assert_eq!(expected_minimum, actual.minimum);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("number"));
    }

    #[test]
    fn deserialize_integer_with_maximum_only() {
        let json = r#"{"type":"integer","maximum":100}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(None, actual.minimum);
        let expected_maximum: Option<f64> = Some(100.0);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("integer"));
    }

    #[test]
    fn deserialize_number_with_maximum_only() {
        let json = r#"{"type":"number","maximum":99.5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
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
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }
}
