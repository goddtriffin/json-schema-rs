//! Integration test: `json_schema_to_rust!` with a number property bounded by minimum and maximum: an `f64` newtype that keeps every digit of a valid value.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        value: schema_0::Value::try_from(50.5_f64).expect("in range"),
        additional: BTreeMap::new(),
    };
    let expected: f64 = 50.5;
    let actual: f64 = root.value.into_inner();
    assert_eq!(expected, actual);
}

#[test]
fn bounded_number_deserializes() {
    let json = r#"{"value":50.5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: f64 = 50.5;
    let actual: f64 = *root.value;
    assert_eq!(expected, actual);
}

#[test]
fn bounded_number_keeps_every_digit_on_a_round_trip() {
    let json: &str = r#"{"value":12.345678901234567}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: String = json.to_string();
    let actual: String = serde_json::to_string(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn bounded_number_out_of_range_is_rejected() {
    let expected: bool = true;
    let actual: bool = serde_json::from_str::<schema_0::Root>(r#"{"value":100.6}"#).is_err();
    assert_eq!(expected, actual);
}
