//! Integration test: `json_schema_to_rust!` with a schema that has minLength and maxLength.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"name":{"type":"string","minLength":2,"maxLength":50}},"required":["name"]}"#
);

#[test]
fn min_length_max_length_compiles_and_deserializes() {
    let root = schema_0::Root {
        name: "Alice".to_string(),
    };
    assert_eq!(root.name, "Alice");
}

#[test]
fn min_length_max_length_json_schema_round_trip() {
    use json_schema_rs::{JsonSchemaSettings, ToJsonSchema, parse_schema};

    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
