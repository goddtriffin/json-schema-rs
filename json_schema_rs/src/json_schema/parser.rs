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
            items: h.items,
            unique_items: h.unique_items,
            min_items: h.min_items,
            max_items: h.max_items,
            minimum: h.minimum,
            maximum: h.maximum,
            min_length: h.min_length,
            max_length: h.max_length,
            format: h.format,
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
    use crate::json_schema::{SpecVersion, resolved_spec_version};
    use std::collections::BTreeMap;

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
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
                    },
                );
                m
            },
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
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
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
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
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
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                        format: None,
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
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
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
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
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
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
    fn parse_with_schema_uri_preserves_schema() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> =
            Some("https://json-schema.org/draft/2020-12/schema".to_string());
        assert_eq!(expected, parsed.schema);
    }

    #[test]
    fn parse_without_schema_uri_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(None, parsed.schema);
    }

    #[test]
    fn parse_without_id_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> = None;
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn parse_with_id_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> = Some("http://example.com/schema".to_string());
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn round_trip_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse");
        assert_eq!(parsed.id, reparsed.id);
    }

    #[test]
    fn parse_strict_accepts_id_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = parse_schema(json, &settings);
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
        let result: Result<JsonSchema, _> = parse_schema(json, &settings);
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
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        assert!(
            serialized.contains("\"$schema\""),
            "serialized output should contain $schema key: {serialized}"
        );
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
        assert_eq!(parsed.schema, reparsed.schema);
    }

    #[test]
    fn resolved_spec_version_infers_2020_12_from_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_absent() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_unknown_uri() {
        let json =
            r#"{"$schema":"https://unknown.example/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_dollar_comment_preserved() {
        let json = r#"{"type":"object","properties":{},"$comment":"Created by John Doe"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let expected: Option<String> = Some("Created by John Doe".to_string());
        assert_eq!(expected, parsed.comment);
    }

    #[test]
    fn parse_without_comment_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        assert_eq!(None, parsed.comment);
    }

    #[test]
    fn round_trip_preserves_dollar_comment() {
        let json = r#"{"type":"object","properties":{"country":{"type":"string","$comment":"TODO: add enum"}},"$comment":"Root schema"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema(&serialized, &settings).expect("parse again");
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
        let result: Result<JsonSchema, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("Note".to_string()), parsed.comment);
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
    fn parse_strict_accepts_unique_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_min_items_max_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema(json, &settings).expect("parse");
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

    #[test]
    fn parse_strict_accepts_format_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"string","format":"uuid"}"#;
        let result: Result<_, _> = parse_schema(json, &settings);
        assert!(result.is_ok());
    }
}
