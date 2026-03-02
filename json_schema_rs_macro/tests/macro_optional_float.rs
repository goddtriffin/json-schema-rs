//! Integration test: `json_schema_to_rust!` with a schema that has an optional number (float) property.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(r#"{"type":"object","properties":{"value":{"type":"number"}}}"#);

#[test]
fn expanded_code_compiles_and_optional_float_deserializes() {
    let root = schema_0::Root { value: Some(2.5) };
    assert_eq!(root.value, Some(2.5));
}

#[test]
fn optional_float_property_deserializes() {
    let json = r#"{"value":2.5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<f64> = Some(2.5);
    let actual: Option<f64> = root.value;
    assert_eq!(expected, actual);
}
