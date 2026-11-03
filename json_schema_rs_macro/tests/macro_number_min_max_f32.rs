//! Integration test: `json_schema_to_rust!` with a schema that has a number property with minimum and maximum in f32 range, producing f32.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        value: schema_0::Value::try_from(50.5_f32).expect("in range"),
        additional: BTreeMap::new(),
    };
    let expected: f32 = 50.5;
    let actual: f32 = root.value.into_inner();
    assert_eq!(expected, actual);
}

#[test]
fn f32_property_deserializes() {
    let json = r#"{"value":50.5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: f32 = 50.5;
    let actual: f32 = *root.value;
    assert_eq!(expected, actual);
}

#[test]
fn f32_property_out_of_range_is_rejected() {
    let expected: bool = true;
    let actual: bool = serde_json::from_str::<schema_0::Root>(r#"{"value":100.6}"#).is_err();
    assert_eq!(expected, actual);
}
