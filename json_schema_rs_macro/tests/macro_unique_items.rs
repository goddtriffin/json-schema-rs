//! Integration test: `json_schema_to_rust!` with a schema that has uniqueItems: true (`Vec`, so
//! element order survives a round trip).

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#
);

#[test]
fn unique_items_round_trip_keeps_element_order() {
    let expected: serde_json::Value = serde_json::json!({"tags": ["core", "generated", "alpha"]});
    let root: schema_0::Root = serde_json::from_value(expected.clone()).expect("deserialize");
    let actual: serde_json::Value = serde_json::to_value(&root).expect("serialize");
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, Schema, ToJsonSchema};

    let root_schema: JsonSchema = schema_0::Root::json_schema().expect("schema");
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
