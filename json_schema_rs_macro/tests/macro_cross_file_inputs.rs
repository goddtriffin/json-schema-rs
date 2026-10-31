//! Integration test: `json_schema_to_rust!` with path schemas that refer to each other by
//! relative `$ref` (a sibling file's root and a fragment inside a sibling file), all passed as
//! macro inputs: each referenced type is generated once and shared.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    "tests/fixtures/cross_file/order.json",
    "tests/fixtures/cross_file/customer.json",
    "tests/fixtures/cross_file/catalog.json"
);

#[test]
fn sibling_file_types_are_the_referenced_documents_types() {
    let json = r#"{"customer":{"name":"Ada"},"item":{"sku":"A-1"}}"#;
    let order: order::Order = serde_json::from_str(json).unwrap();
    let customer: customer::Customer = order.customer;
    let thing: catalog::Thing = order.item;
    let expected: (String, String) = ("Ada".to_string(), "A-1".to_string());
    let actual: (String, String) = (customer.name, thing.sku);
    assert_eq!(expected, actual);
}

#[test]
fn sibling_file_refs_round_trip() {
    let expected: serde_json::Value = serde_json::json!({
        "customer": {"name": "Ada", "vip": true},
        "item": {"sku": "A-1"}
    });
    let order: order::Order = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&order).unwrap();
    assert_eq!(expected, actual);
}
