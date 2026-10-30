//! Integration test: `json_schema_to_rust!` with object keys the schema does not name: captured
//! by default, typed by an `additionalProperties` schema, rejected by
//! `additionalProperties: false`.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
    r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":{"type":"integer"}}"#,
    r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}"#
);

#[test]
fn unknown_keys_are_captured() {
    let json = r#"{"id":"x","extra":1,"nested":{"k":[true]}}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: BTreeMap<String, serde_json::Value> = BTreeMap::from([
        ("extra".to_string(), serde_json::json!(1)),
        ("nested".to_string(), serde_json::json!({"k": [true]})),
    ]);
    let actual: BTreeMap<String, serde_json::Value> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn captured_unknown_keys_serialize_back() {
    let expected: serde_json::Value =
        serde_json::json!({"id": "x", "extra": 1, "nested": {"k": [true]}});
    let root: schema_0::Root = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn typed_additional_properties_capture_typed_values() {
    let json = r#"{"id":"x","a":1,"b":2}"#;
    let root: schema_1::Root = serde_json::from_str(json).unwrap();
    let expected: BTreeMap<String, i64> =
        BTreeMap::from([("a".to_string(), 1), ("b".to_string(), 2)]);
    let actual: BTreeMap<String, i64> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn additional_properties_false_rejects_unknown_keys() {
    let expected: (bool, bool) = (true, true);
    let actual: (bool, bool) = (
        serde_json::from_str::<schema_2::Root>(r#"{"id":"x"}"#).is_ok(),
        serde_json::from_str::<schema_2::Root>(r#"{"id":"x","extra":1}"#).is_err(),
    );
    assert_eq!(expected, actual);
}
