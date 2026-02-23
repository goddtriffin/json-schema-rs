//! Integration test: `json_schema_to_rust!` with a schema that has an integer property.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"count":{"type":"integer"}},"required":["count"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root { count: 42 };
    assert_eq!(root.count, 42);
}

#[test]
fn integer_property_deserializes() {
    let json = r#"{"count":100}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: i64 = 100;
    let actual: i64 = root.count;
    assert_eq!(expected, actual);
}
