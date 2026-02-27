//! Public API for parsing JSON Schema with configurable settings.

use super::error::JsonSchemaParseError;
use super::json_schema::{
    DenyUnknownFieldsJsonSchema, JsonSchema, deny_unknown_fields_helper_to_schema,
};
use super::settings::JsonSchemaSettings;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;

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
            #[serde(default, rename = "allOf")]
            all_of: Option<Vec<JsonSchema>>,
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
            all_of: h.all_of,
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

/// Strict parse from a serde JSON value. Used by [`parse_schema_from_serde_value`] when strict.
pub(crate) fn parse_strict_value(
    value: &serde_json::Value,
) -> Result<JsonSchema, JsonSchemaParseError> {
    let helper: DenyUnknownFieldsJsonSchema =
        serde_json::from_value(value.clone()).map_err(JsonSchemaParseError::from)?;
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
pub fn parse_schema_from_str(
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
/// Same as [`parse_schema_from_str`] but takes bytes (e.g. from a file).
///
/// # Errors
///
/// Same as [`parse_schema_from_str`].
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

/// Parse a JSON Schema from an already-parsed [`serde_json::Value`] with the given settings.
///
/// Same semantics as [`parse_schema_from_str`]; takes a value to avoid string round-trips
/// when the schema is already loaded as JSON (e.g. from a test case or API response).
///
/// # Errors
///
/// Same as [`parse_schema_from_str`].
pub fn parse_schema_from_serde_value(
    value: &serde_json::Value,
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, JsonSchemaParseError> {
    if settings.disallow_unknown_fields {
        parse_strict_value(value)
    } else {
        let schema: JsonSchema = serde_json::from_value(value.clone())?;
        Ok(schema)
    }
}

/// Parse a JSON Schema from a reader with the given settings.
///
/// Same as [`parse_schema_from_str`] but reads from a reader. I/O errors are
/// returned as [`JsonSchemaParseError::Io`].
///
/// # Errors
///
/// Returns [`JsonSchemaParseError::Io`] on read failure.
/// Otherwise same as [`parse_schema_from_str`].
pub fn parse_schema_from_reader<R: Read>(
    reader: R,
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, JsonSchemaParseError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut reader = reader;
    reader
        .read_to_end(&mut buf)
        .map_err(JsonSchemaParseError::Io)?;
    parse_schema_from_slice(&buf, settings)
}

/// Parse a JSON Schema from a file path with the given settings.
///
/// Same as [`parse_schema_from_str`] but reads from a file. I/O errors (e.g. file not found)
/// are returned as [`JsonSchemaParseError::Io`].
///
/// # Errors
///
/// Returns [`JsonSchemaParseError::Io`] on open or read failure.
/// Otherwise same as [`parse_schema_from_str`].
pub fn parse_schema_from_path<P: AsRef<std::path::Path>>(
    path: P,
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, JsonSchemaParseError> {
    let f: std::fs::File = std::fs::File::open(path.as_ref())?;
    parse_schema_from_reader(f, settings)
}

#[cfg(test)]
mod tests {
    use super::{
        JsonSchema, JsonSchemaSettings, parse_schema_from_path, parse_schema_from_reader,
        parse_schema_from_serde_value, parse_schema_from_str,
    };
    use crate::json_schema::{JsonSchemaParseError, SpecVersion, resolved_spec_version};
    use std::collections::BTreeMap;
    use std::io;

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
                        all_of: None,
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
            all_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_present() {
        let json = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let settings = JsonSchemaSettings::builder().build();
        let parsed = parse_schema_from_str(json, &settings).expect("parse");
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
        let settings = JsonSchemaSettings::builder().build();
        let parsed = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_empty_array() {
        let json = r#"{"allOf":[]}"#;
        let settings = JsonSchemaSettings::builder().build();
        let parsed = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_single_subschema() {
        let json = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let settings = JsonSchemaSettings::builder().build();
        let parsed = parse_schema_from_str(json, &settings).expect("parse");
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
                        all_of: None,
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
                        all_of: None,
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
            all_of: None,
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
            all_of: None,
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
            all_of: None,
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items present").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
    }

    #[test]
    fn parse_lenient_accepts_unknown_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_rejects_unknown_key_at_root() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{},"unknown":42}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
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
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_err());
    }

    #[test]
    fn parse_strict_accepts_only_known_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_with_schema_uri_preserves_schema() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<String> =
            Some("https://json-schema.org/draft/2020-12/schema".to_string());
        assert_eq!(expected, parsed.schema);
    }

    #[test]
    fn parse_without_schema_uri_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        assert_eq!(None, parsed.schema);
    }

    #[test]
    fn parse_without_id_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<String> = None;
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn parse_with_id_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<String> = Some("http://example.com/schema".to_string());
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn round_trip_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = parse_schema_from_str(&serialized, &settings).expect("parse");
        assert_eq!(parsed.id, reparsed.id);
    }

    #[test]
    fn parse_strict_accepts_id_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = parse_schema_from_str(json, &settings);
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
        let result: Result<JsonSchema, _> = parse_schema_from_str(json, &settings);
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
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        assert!(
            serialized.contains("\"$schema\""),
            "serialized output should contain $schema key: {serialized}"
        );
        let reparsed: JsonSchema =
            parse_schema_from_str(&serialized, &settings).expect("parse again");
        assert_eq!(parsed.schema, reparsed.schema);
    }

    #[test]
    fn resolved_spec_version_infers_2020_12_from_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_absent() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_unknown_uri() {
        let json =
            r#"{"$schema":"https://unknown.example/schema","type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_dollar_comment_preserved() {
        let json = r#"{"type":"object","properties":{},"$comment":"Created by John Doe"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<String> = Some("Created by John Doe".to_string());
        assert_eq!(expected, parsed.comment);
    }

    #[test]
    fn parse_without_comment_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        assert_eq!(None, parsed.comment);
    }

    #[test]
    fn round_trip_preserves_dollar_comment() {
        let json = r#"{"type":"object","properties":{"country":{"type":"string","$comment":"TODO: add enum"}},"$comment":"Root schema"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema =
            parse_schema_from_str(&serialized, &settings).expect("parse again");
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
        let result: Result<JsonSchema, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("Note".to_string()), parsed.comment);
    }

    #[test]
    fn deserialize_schema_with_enum() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let parsed: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
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
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_unique_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_min_items_max_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
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
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn deserialize_integer_with_minimum_and_maximum() {
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
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
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        let expected: Option<f64> = Some(-100.0);
        assert_eq!(expected, actual.minimum);
        assert_eq!(None, actual.maximum);
    }

    #[test]
    fn deserialize_number_with_minimum_and_maximum_float() {
        let json = r#"{"type":"number","minimum":0.5,"maximum":100.5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
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
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
        assert_eq!(None, actual.minimum);
        let expected_maximum: Option<f64> = Some(100.0);
        assert_eq!(expected_maximum, actual.maximum);
        assert_eq!(actual.type_.as_deref(), Some("integer"));
    }

    #[test]
    fn deserialize_number_with_maximum_only() {
        let json = r#"{"type":"number","maximum":99.5}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_str(json, &settings).expect("parse");
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
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_format_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"string","format":"uuid"}"#;
        let result: Result<_, _> = parse_schema_from_str(json, &settings);
        assert!(result.is_ok());
    }

    // --- parse_schema_from_serde_value ---

    #[test]
    fn parse_schema_from_serde_value_lenient_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_serde_value(&value, &settings).expect("parse");
        let expected: JsonSchema = parse_schema_from_str(
            r#"{"type":"object","properties":{"a":{"type":"string"}}}"#,
            &settings,
        )
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
        let actual: JsonSchema = parse_schema_from_serde_value(&value, &settings).expect("parse");
        let expected: JsonSchema = parse_schema_from_str(
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
            parse_schema_from_serde_value(&value, &settings);
        // from_value maps serde's unknown-field error to Serde
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn parse_schema_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            parse_schema_from_serde_value(&value, &settings);
        let actual_is_err: bool = result.is_err();
        assert!(actual_is_err, "string value should fail to parse as schema");
    }

    #[test]
    fn parse_schema_from_serde_value_round_trip_with_parse_schema_from_str() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"]}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let from_str: JsonSchema = parse_schema_from_str(json, &settings).expect("parse from str");
        let serialized: String = (&from_str).try_into().expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("str to value");
        let from_value: JsonSchema =
            parse_schema_from_serde_value(&value, &settings).expect("parse from value");
        assert_eq!(from_str, from_value);
    }

    // --- parse_schema_from_reader ---

    #[test]
    fn parse_schema_from_reader_success() {
        let bytes: &[u8] = r#"{"type":"object","properties":{}}"#.as_bytes();
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(bytes);
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_reader(reader, &settings).expect("parse");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_reader_invalid_json() {
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(b"not json");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            parse_schema_from_reader(reader, &settings);
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
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            parse_schema_from_reader(reader, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    // --- parse_schema_from_path ---

    #[test]
    fn parse_schema_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let actual: JsonSchema = parse_schema_from_path(&schema_path, &settings).expect("parse");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("nonexistent.json");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            parse_schema_from_path(&missing_path, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn parse_schema_from_path_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            parse_schema_from_path(&schema_path, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }
}
