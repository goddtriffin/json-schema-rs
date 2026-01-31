use serde::Deserialize;
use std::collections::BTreeMap;

/// Wraps the JSON Schema `default` keyword to preserve `null`.
/// Serde deserializes `Option<Value>` with JSON null as `None`; we need to
/// distinguish absent key from `"default": null`.
#[derive(Debug, Default)]
pub enum DefaultKeyword {
    /// Key "default" was absent from the schema.
    #[default]
    Absent,
    /// Key "default" was present; the value may be `Value::Null`.
    Present(serde_json::Value),
}

impl<'de> Deserialize<'de> for DefaultKeyword {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v: serde_json::Value = Deserialize::deserialize(deserializer)?;
        Ok(DefaultKeyword::Present(v))
    }
}

/// Root or nested JSON Schema object.
///
/// Only the schema fields used by the generator are modeled.
/// Extra keys in the JSON are ignored via serde's default behavior.
/// Uses `BTreeMap` for deterministic property ordering (alphabetical by key).
#[derive(Debug, Deserialize)]
pub struct JsonSchema {
    #[serde(default)]
    pub title: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub r#type: Option<String>,

    #[serde(default)]
    pub properties: Option<BTreeMap<String, Box<JsonSchema>>>,

    #[serde(default)]
    pub required: Option<Vec<String>>,

    #[serde(default)]
    pub r#enum: Option<Vec<serde_json::Value>>,

    #[serde(default)]
    pub items: Option<Box<JsonSchema>>,

    #[serde(default, rename = "additionalProperties")]
    pub additional_properties: Option<serde_json::Value>,

    #[serde(default)]
    pub default: DefaultKeyword,
}
