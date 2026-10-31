//! Integration test: `json_schema_to_rust!` with one path schema whose relative `$ref`s name
//! sibling files that are not macro inputs: they are read from the crate's directory and their
//! types are generated into the input's module.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!("tests/fixtures/cross_file/order.json");

#[test]
fn sibling_files_not_inputs_are_read_and_generated() {
    let json = r#"{"customer":{"name":"Ada"},"item":{"sku":"A-1"}}"#;
    let order: order::Order = serde_json::from_str(json).unwrap();
    let customer: order::Customer = order.customer;
    let thing: order::Thing = order.item;
    let expected: (String, String) = ("Ada".to_string(), "A-1".to_string());
    let actual: (String, String) = (customer.name, thing.sku);
    assert_eq!(expected, actual);
}

#[test]
fn sibling_files_not_inputs_round_trip() {
    let expected: serde_json::Value = serde_json::json!({
        "customer": {"name": "Ada"},
        "item": {"sku": "A-1", "color": "red"}
    });
    let order: order::Order = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&order).unwrap();
    assert_eq!(expected, actual);
}
