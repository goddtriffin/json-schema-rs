//! Integration test: `json_schema_to_rust!` with a schema that has minLength and maxLength.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"name":{"type":"string","minLength":2,"maxLength":50}},"required":["name"]}"#
);

#[test]
fn min_length_max_length_compiles_and_deserializes() {
    let root = schema_0::Root {
        name: "Alice".to_string(),
        additional: BTreeMap::new(),
    };
    assert_eq!(root.name, "Alice");
}

#[test]
fn min_length_max_length_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, Schema, ToJsonSchema};

    let root_schema: JsonSchema = schema_0::Root::json_schema().expect("schema");
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
