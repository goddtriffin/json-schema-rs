//! Integration test: `json_schema_to_rust!` with a schema that has a number (float) property.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"value":{"type":"number"}},"required":["value"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        value: 2.5,
        additional: BTreeMap::new(),
    };
    assert_eq!(root.value, 2.5);
}

#[test]
fn float_property_deserializes() {
    let json = r#"{"value":2.5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: f64 = 2.5;
    let actual: f64 = root.value;
    assert_eq!(expected, actual);
}
