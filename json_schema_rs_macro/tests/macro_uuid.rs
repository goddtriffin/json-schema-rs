//! Integration test: `json_schema_to_rust!` with a schema that has a UUID property.
//! Only compiled and run when the `uuid` feature is enabled.
#![cfg(feature = "uuid")]

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}},"required":["id"]}"#
);

#[test]
fn uuid_field_is_uuid_type() {
    let id = uuid::Uuid::new_v4();
    let root = schema_0::Root { id };
    assert_eq!(root.id, id);
}

#[test]
fn uuid_macro_round_trip() {
    let id = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let json = format!(r#"{{"id":"{}"}}"#, id);
    let root: schema_0::Root = serde_json::from_str(&json).unwrap();
    assert_eq!(root.id, id);
    let serialized = serde_json::to_string(&root).unwrap();
    let parsed: schema_0::Root = serde_json::from_str(&serialized).unwrap();
    assert_eq!(parsed.id, id);
}
