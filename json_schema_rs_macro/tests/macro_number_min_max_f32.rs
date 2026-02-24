//! Integration test: `json_schema_to_rust!` with a schema that has a number property with minimum and maximum in f32 range, producing f32.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#
);

#[test]
#[expect(clippy::float_cmp)]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root { value: 50.5 };
    let expected: f32 = 50.5;
    let actual: f32 = root.value;
    assert_eq!(expected, actual);
}

#[test]
#[expect(clippy::float_cmp)]
fn f32_property_deserializes() {
    let json = r#"{"value":50.5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: f32 = 50.5;
    let actual: f32 = root.value;
    assert_eq!(expected, actual);
}
