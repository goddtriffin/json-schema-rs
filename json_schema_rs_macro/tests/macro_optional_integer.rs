//! Integration test: `json_schema_to_rust!` with a schema that has an optional integer property.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(r#"{"type":"object","properties":{"count":{"type":"integer"}}}"#);

#[test]
fn expanded_code_compiles_and_optional_integer_deserializes() {
    let root = schema_0::Root { count: Some(42) };
    assert_eq!(root.count, Some(42));
}

#[test]
fn optional_integer_property_deserializes() {
    let json = r#"{"count":100}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<i64> = Some(100);
    let actual: Option<i64> = root.count;
    assert_eq!(expected, actual);
}
