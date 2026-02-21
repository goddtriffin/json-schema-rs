//! Integration test: `json_schema_to_rust!` with a single inline schema containing a nested object.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{
    "type": "object",
    "properties": {
        "address": {
            "type": "object",
            "properties": {
                "city": { "type": "string" },
                "street_address": { "type": "string" }
            }
        }
    }
}"#
);

#[test]
fn expanded_code_compiles_and_nested_type_exists() {
    let root = schema_0::Root {
        address: Some(schema_0::Address {
            city: Some(String::from("NYC")),
            street_address: Some(String::from("123 Main")),
        }),
    };
    assert_eq!(root.address.as_ref().unwrap().city.as_deref(), Some("NYC"));
    assert_eq!(
        root.address.as_ref().unwrap().street_address.as_deref(),
        Some("123 Main")
    );
}

#[test]
fn nested_object_deserializes() {
    let json = r#"{"address":{"city":"Boston","street_address":"456 Oak"}}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    assert!(root.address.is_some());
    assert_eq!(
        root.address.as_ref().unwrap().city.as_deref(),
        Some("Boston")
    );
    assert_eq!(
        root.address.as_ref().unwrap().street_address.as_deref(),
        Some("456 Oak")
    );
}
