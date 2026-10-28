//! Integration test: `json_schema_to_rust!` with an integer property bounded by `exclusiveMinimum` -1 and `exclusiveMaximum` 256 (0..=255) producing u8.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"byte":{"type":"integer","exclusiveMinimum":-1,"exclusiveMaximum":256}},"required":["byte"]}"#
);

#[test]
fn u8_property_deserializes() {
    let json = r#"{"byte":255}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: u8 = 255;
    let actual: u8 = root.byte;
    assert_eq!(expected, actual);
}
