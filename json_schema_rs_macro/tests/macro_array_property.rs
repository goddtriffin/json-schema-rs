//! Integration test: `json_schema_to_rust!` with a schema that has an array property.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}},"required":["tags"]}"#
);

#[test]
fn array_property_compiles_and_deserializes() {
    let root = schema_0::Root {
        tags: vec!["a".to_string(), "b".to_string()],
    };
    assert_eq!(root.tags, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn array_property_json_schema_round_trip() {
    use json_schema_rs::{JsonSchemaSettings, ToJsonSchema, parse_schema_from_str};

    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema =
        parse_schema_from_str(&json, &settings).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
