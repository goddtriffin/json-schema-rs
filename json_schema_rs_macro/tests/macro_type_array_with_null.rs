//! Integration test: `json_schema_to_rust!` with an optional property whose `type` is
//! `["string", "null"]`: absent, `null` and a string are three states.

use json_schema_rs::Nullable;
use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(r#"{"type":"object","properties":{"name":{"type":["string","null"]}}}"#);

#[test]
fn type_array_with_null_property_deserializes_string() {
    let json = r#"{"name":"x"}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Nullable<String> = Nullable::Value("x".to_string());
    let actual: Nullable<String> = root.name;
    assert_eq!(expected, actual);
}

#[test]
fn type_array_with_null_property_deserializes_null_as_null() {
    let json = r#"{"name":null}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Nullable<String> = Nullable::Null;
    let actual: Nullable<String> = root.name;
    assert_eq!(expected, actual);
}

#[test]
fn type_array_with_null_property_absent_deserializes_as_absent() {
    let json = "{}";
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Nullable<String> = Nullable::Absent;
    let actual: Nullable<String> = root.name;
    assert_eq!(expected, actual);
}

#[test]
fn type_array_with_null_property_round_trips_absent_and_null_apart() {
    let round_trip = |json: &str| -> serde_json::Value {
        let root: schema_0::Root = serde_json::from_str(json).unwrap();
        serde_json::to_value(&root).unwrap()
    };
    let expected: (serde_json::Value, serde_json::Value) =
        (serde_json::json!({}), serde_json::json!({"name": null}));
    let actual: (serde_json::Value, serde_json::Value) =
        (round_trip("{}"), round_trip(r#"{"name":null}"#));
    assert_eq!(expected, actual);
}
