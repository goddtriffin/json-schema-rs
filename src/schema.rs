use serde::Deserialize;
use std::collections::BTreeMap;

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
    pub r#type: Option<String>,

    #[serde(default)]
    pub properties: Option<BTreeMap<String, Box<JsonSchema>>>,

    #[serde(default)]
    pub required: Option<Vec<String>>,

    #[serde(default)]
    pub r#enum: Option<Vec<serde_json::Value>>,
}
