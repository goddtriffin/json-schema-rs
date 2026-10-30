//! Integration test: `json_schema_to_rust!` with a single inline schema containing a nested object.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

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
            additional: BTreeMap::new(),
        }),
        additional: BTreeMap::new(),
    };
    let address: schema_0::Address = root.address.expect("address");
    let expected: (Option<String>, Option<String>) =
        (Some("NYC".to_string()), Some("123 Main".to_string()));
    let actual: (Option<String>, Option<String>) = (address.city, address.street_address);
    assert_eq!(expected, actual);
}

#[test]
fn nested_object_deserializes() {
    let json = r#"{"address":{"city":"Boston","street_address":"456 Oak"}}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<(Option<String>, Option<String>)> =
        Some((Some("Boston".to_string()), Some("456 Oak".to_string())));
    let actual: Option<(Option<String>, Option<String>)> =
        root.address.map(|a| (a.city, a.street_address));
    assert_eq!(expected, actual);
}

#[test]
fn nested_object_each_type_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, ToJsonSchema};
    let expected: Vec<JsonSchema> = vec![
        schema_0::Root::json_schema().expect("schema"),
        schema_0::Address::json_schema().expect("schema"),
    ];
    let actual: Vec<JsonSchema> = expected
        .iter()
        .map(|schema| {
            let json: String = schema.try_into().expect("serialize");
            JsonSchema::try_from(json.as_str()).expect("parse")
        })
        .collect();
    assert_eq!(expected, actual);
}
