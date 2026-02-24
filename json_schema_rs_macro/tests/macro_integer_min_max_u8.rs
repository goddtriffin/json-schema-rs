//! Integration test: `json_schema_to_rust!` with a schema that has an integer property with minimum and maximum (0..=255) producing u8.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"byte":{"type":"integer","minimum":0,"maximum":255}},"required":["byte"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root { byte: 42 };
    let expected: u8 = 42;
    let actual: u8 = root.byte;
    assert_eq!(expected, actual);
}

#[test]
fn u8_property_deserializes() {
    let json = r#"{"byte":100}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: u8 = 100;
    let actual: u8 = root.byte;
    assert_eq!(expected, actual);
}
