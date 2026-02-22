//! Integration test: `#[derive(ToJsonSchema)]` produces the expected JSON Schema.

use json_schema_rs::{JsonSchema, JsonSchemaSettings, ToJsonSchema, parse_schema};
use json_schema_rs_macro::ToJsonSchema;
use std::collections::BTreeMap;

#[derive(ToJsonSchema)]
#[to_json_schema(title = "Root")]
#[expect(dead_code)]
struct Root {
    id: String,
    name: Option<String>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Address {
    city: String,
    street: String,
}

#[test]
fn derive_root_json_schema() {
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("id".to_string(), String::json_schema());
            m.insert("name".to_string(), Option::<String>::json_schema());
            m
        },
        required: Some(vec!["id".to_string()]),
        title: Some("Root".to_string()),
    };
    let actual: JsonSchema = Root::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_address_json_schema() {
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("city".to_string(), String::json_schema());
            m.insert("street".to_string(), String::json_schema());
            m
        },
        required: Some(vec!["city".to_string(), "street".to_string()]),
        title: None,
    };
    let actual: JsonSchema = Address::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_serialize_round_trip() {
    let schema: JsonSchema = Root::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}
