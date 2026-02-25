//! Integration test: `json_schema_to_rust!` with a schema that has minItems and maxItems.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}},"required":["tags"]}"#
);

#[test]
fn min_items_max_items_compiles_and_deserializes() {
    let root = schema_0::Root {
        tags: vec!["a".to_string(), "b".to_string(), "c".to_string()],
    };
    assert_eq!(
        root.tags,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );
}

#[test]
fn min_items_max_items_json_schema_round_trip() {
    use json_schema_rs::{JsonSchemaSettings, ToJsonSchema, parse_schema};

    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
