//! Integration test: `json_schema_to_rust!` with a property whose `type` is `["string", "null"]`.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(r#"{"type":"object","properties":{"name":{"type":["string","null"]}}}"#);

#[test]
fn type_array_with_null_property_deserializes_string() {
    let json = r#"{"name":"x"}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<String> = Some("x".to_string());
    let actual: Option<String> = root.name;
    assert_eq!(expected, actual);
}

#[test]
fn type_array_with_null_property_deserializes_null_as_none() {
    let json = r#"{"name":null}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<String> = None;
    let actual: Option<String> = root.name;
    assert_eq!(expected, actual);
}
