//! Public API for parsing JSON Schema with configurable settings.

use crate::json_schema::{JsonSchema, SchemaIngestionError, parse_strict_slice, parse_strict_str};
use crate::json_schema_settings::JsonSchemaSettings;

/// Parse a JSON Schema from a string with the given settings.
///
/// When [`JsonSchemaSettings::disallow_unknown_fields`] is `false`, unknown keys
/// are ignored (lenient). When `true`, any unknown key causes an error (strict).
///
/// # Errors
///
/// Returns [`SchemaIngestionError::Serde`] on invalid JSON or type mismatch.
/// Returns [`SchemaIngestionError::UnknownField`] when strict and an unknown key is present.
pub fn parse_schema(
    json: &str,
    settings: &JsonSchemaSettings,
) -> Result<JsonSchema, SchemaIngestionError> {
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
) -> Result<JsonSchema, SchemaIngestionError> {
    if settings.disallow_unknown_fields {
        parse_strict_slice(slice)
    } else {
        let schema: JsonSchema = serde_json::from_slice(slice)?;
        Ok(schema)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_schema;
    use crate::json_schema_settings::JsonSchemaSettings;

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
