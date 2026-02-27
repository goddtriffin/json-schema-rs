//! Integration test: `json_schema_to_rust!` with a schema that has uniqueItems: true (`HashSet`).

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::HashSet;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#
);

#[test]
fn unique_items_compiles_and_deserializes() {
    let root = schema_0::Root {
        tags: ["a".to_string(), "b".to_string()].into_iter().collect(),
    };
    let expected: HashSet<String> = ["a".to_string(), "b".to_string()].into_iter().collect();
    assert_eq!(root.tags, expected);
}

#[test]
fn unique_items_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, JsonSchemaSettings, ToJsonSchema};

    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema =
        JsonSchema::new_from_str(&json, &settings).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
