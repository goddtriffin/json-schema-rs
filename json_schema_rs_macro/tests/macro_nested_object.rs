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

#[test]
fn nested_object_each_type_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, ToJsonSchema};
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
    let address_schema = schema_0::Address::json_schema();
    let json_addr: String = (&address_schema).try_into().expect("serialize");
    let reparsed_addr: json_schema_rs::JsonSchema =
        JsonSchema::try_from(json_addr.as_str()).expect("parse");
    assert_eq!(
        address_schema, reparsed_addr,
        "Address::json_schema() round-trip"
    );
}
