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

#[derive(ToJsonSchema)]
#[expect(dead_code)]
enum Status {
    Open,
    Closed,
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
        description: None,
        enum_values: None,
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
        description: None,
        enum_values: None,
    };
    let actual: JsonSchema = Address::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_unit_enum_json_schema() {
    let actual: JsonSchema = Status::json_schema();
    assert_eq!(actual.type_.as_deref(), Some("string"));
    let actual_enum = actual.enum_values.as_ref().expect("enum_values");
    assert_eq!(actual_enum.len(), 2);
    assert!(actual_enum.contains(&serde_json::Value::String("Open".to_string())));
    assert!(actual_enum.contains(&serde_json::Value::String("Closed".to_string())));
}

#[test]
fn derive_serialize_round_trip() {
    let schema: JsonSchema = Root::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}

#[derive(ToJsonSchema)]
#[to_json_schema(description = "From attribute")]
#[expect(dead_code)]
struct AttrDescription {
    y: i64,
}

#[test]
fn derive_attribute_description() {
    let actual: JsonSchema = AttrDescription::json_schema();
    let expected_desc: Option<String> = Some("From attribute".to_string());
    assert_eq!(expected_desc, actual.description);
}

#[derive(ToJsonSchema)]
#[to_json_schema(description = "Struct with description attribute")]
#[expect(dead_code)]
struct WithDocAttr {
    x: String,
}

#[test]
fn derive_struct_description_attribute() {
    let actual: JsonSchema = WithDocAttr::json_schema();
    let expected_desc: Option<String> = Some("Struct with description attribute".to_string());
    assert_eq!(expected_desc, actual.description);
}

#[derive(ToJsonSchema)]
#[to_json_schema(description = "Enum description")]
#[expect(dead_code)]
enum EnumWithDescAttr {
    A,
    B,
}

#[test]
fn derive_enum_description_attribute() {
    let actual: JsonSchema = EnumWithDescAttr::json_schema();
    let expected_desc: Option<String> = Some("Enum description".to_string());
    assert_eq!(expected_desc, actual.description);
}

#[test]
fn derive_round_trip_with_description() {
    let schema: JsonSchema = AttrDescription::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}
