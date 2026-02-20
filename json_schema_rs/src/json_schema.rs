//! In-memory representation of JSON Schema for codegen.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

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

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonSchema {
    /// Schema type; `object` and `string` drive codegen; others are ignored.
    pub type_: Option<String>,

    /// Object properties (only when type is "object"). Default empty; use `BTreeMap` for stable ordering.
    pub properties: BTreeMap<String, JsonSchema>,

    /// Required property names at this object level. When absent, all properties are optional.
    pub required: Option<Vec<String>>,

    /// Used for struct naming when present (`PascalCase`).
    pub title: Option<String>,
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

/// Error when parsing (ingesting) a JSON Schema with the given settings.
#[derive(Debug)]
pub enum SchemaIngestionError {
    /// JSON or serde error (invalid JSON, wrong types, etc.).
    Serde(serde_json::Error),
    /// An unknown key was present and strict ingestion was enabled.
    UnknownField {
        /// The unknown key name.
        key: String,
        /// JSON Pointer or path to the schema object that contained the key.
        path: String,
    },
}

impl fmt::Display for SchemaIngestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaIngestionError::Serde(e) => write!(f, "invalid JSON Schema: {e}"),
            SchemaIngestionError::UnknownField { key, path } => {
                write!(f, "unknown schema key \"{key}\" at {path}")
            }
        }
    }
}

impl std::error::Error for SchemaIngestionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SchemaIngestionError::Serde(e) => Some(e),
            SchemaIngestionError::UnknownField { .. } => None,
        }
    }
}

impl From<serde_json::Error> for SchemaIngestionError {
    fn from(e: serde_json::Error) -> Self {
        SchemaIngestionError::Serde(e)
    }
}

/// Strict schema helper: same shape as our schema model but with `#[serde(deny_unknown_fields)]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictJsonSchemaHelper {
    #[serde(
        default,
        deserialize_with = "deserialize_type_optional_strict",
        rename = "type"
    )]
    type_: Option<String>,
    #[serde(default)]
    properties: Option<BTreeMap<String, StrictJsonSchemaHelper>>,
    #[serde(default)]
    required: Option<Vec<String>>,
    #[serde(default)]
    title: Option<String>,
}

fn deserialize_type_optional_strict<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
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

fn strict_helper_to_schema(h: StrictJsonSchemaHelper) -> JsonSchema {
    let properties: BTreeMap<String, JsonSchema> = h
        .properties
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, strict_helper_to_schema(v)))
        .collect();
    JsonSchema {
        type_: h.type_,
        properties,
        required: h.required,
        title: h.title,
    }
}

/// Strict parse from string. Used by the public parser when `disallow_unknown_fields` is true.
pub(crate) fn parse_strict_str(json: &str) -> Result<JsonSchema, SchemaIngestionError> {
    let helper: StrictJsonSchemaHelper = serde_json::from_str(json)?;
    Ok(strict_helper_to_schema(helper))
}

/// Strict parse from slice. Used by the public parser when `disallow_unknown_fields` is true.
pub(crate) fn parse_strict_slice(slice: &[u8]) -> Result<JsonSchema, SchemaIngestionError> {
    let helper: StrictJsonSchemaHelper =
        serde_json::from_slice(slice).map_err(SchemaIngestionError::Serde)?;
    Ok(strict_helper_to_schema(helper))
}

#[cfg(test)]
mod tests {
    use super::JsonSchema;
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
