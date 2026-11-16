//! Integration test: `json_schema_to_rust!`'s `crates` and `unknown_crates` settings make a
//! subschema with `x-rust-type` the named crate's type, as typify's `with_crate` and
//! `with_unknown_crates` do.

// This test crate stands in for the crate `my-types`, which the schema's `x-rust-type` names.
extern crate self as my_types;

/// The type `my_types::Item` the schema names, as the crate `my-types` would generate it.
#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    json_schema_rs_macro::ToJsonSchema,
)]
pub struct Item {
    pub sku: String,
}

mod order {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Order","type":"object","properties":{"item":{"type":"object","properties":{"sku":{"type":"string"}},"required":["sku"],"x-rust-type":{"crate":"my-types","version":"^1.2","path":"my_types::Item"}},"note":{"type":"object","properties":{"text":{"type":"string"}},"x-rust-type":{"crate":"other","version":"*","path":"other::Note"}}},"required":["item"]}"#,
        crates = ["my-types@1.2.3"]
    );
}

#[test]
fn listed_crate_deserializes_into_its_type() {
    let order: order::schema_0::Order = serde_json::from_value(serde_json::json!({
        "item": {"sku": "A1"},
        "note": {"text": "fragile"}
    }))
    .expect("deserialize");
    let expected: (Item, Option<String>) = (
        Item {
            sku: "A1".to_string(),
        },
        Some("fragile".to_string()),
    );
    let actual: (Item, Option<String>) = (order.item, order.note.and_then(|note| note.text));
    assert_eq!(expected, actual);
}
