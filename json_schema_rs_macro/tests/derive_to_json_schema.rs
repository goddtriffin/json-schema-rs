//! Integration test: `#[derive(ToJsonSchema)]` produces the expected JSON Schema.

use json_schema_rs::{JsonSchema, JsonSchemaSettings, ToJsonSchema, parse_schema};
use json_schema_rs_macro::ToJsonSchema;
use std::collections::{BTreeMap, HashSet};

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
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
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
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
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

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithIntegerMinMax {
    #[to_json_schema(minimum = 0, maximum = 255)]
    byte: i64,
}

#[test]
fn derive_field_minimum_maximum_integer() {
    let mut byte_schema: JsonSchema = i64::json_schema();
    byte_schema.minimum = Some(0.0);
    byte_schema.maximum = Some(255.0);
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("byte".to_string(), byte_schema);
            m
        },
        required: Some(vec!["byte".to_string()]),
        title: None,
        description: None,
        enum_values: None,
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
    };
    let actual: JsonSchema = WithIntegerMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithFloatMinMax {
    #[to_json_schema(minimum = 0.0, maximum = 100.0)]
    score: f64,
}

#[test]
fn derive_field_minimum_maximum_float() {
    let mut score_schema: JsonSchema = f64::json_schema();
    score_schema.minimum = Some(0.0);
    score_schema.maximum = Some(100.0);
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("score".to_string(), score_schema);
            m
        },
        required: Some(vec!["score".to_string()]),
        title: None,
        description: None,
        enum_values: None,
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
    };
    let actual: JsonSchema = WithFloatMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_minimum_maximum_round_trip() {
    let schema: JsonSchema = WithIntegerMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}

#[test]
fn derive_minimum_maximum_float_round_trip() {
    let schema: JsonSchema = WithFloatMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithOnlyMinimum {
    #[to_json_schema(minimum = 10)]
    value: i64,
}

#[test]
fn derive_field_only_minimum() {
    let mut value_schema: JsonSchema = i64::json_schema();
    value_schema.minimum = Some(10.0);
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        required: Some(vec!["value".to_string()]),
        title: None,
        description: None,
        enum_values: None,
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
    };
    let actual: JsonSchema = WithOnlyMinimum::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithOnlyMaximum {
    #[to_json_schema(maximum = 90)]
    value: i64,
}

#[test]
fn derive_field_only_maximum() {
    let mut value_schema: JsonSchema = i64::json_schema();
    value_schema.maximum = Some(90.0);
    let expected: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        required: Some(vec!["value".to_string()]),
        title: None,
        description: None,
        enum_values: None,
        items: None,
        unique_items: None,
        minimum: None,
        maximum: None,
    };
    let actual: JsonSchema = WithOnlyMaximum::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithVecField {
    tags: Vec<String>,
    counts: Option<Vec<i64>>,
}

#[test]
fn derive_struct_with_vec_field_emits_array_schema() {
    let schema: JsonSchema = WithVecField::json_schema();
    let tags_schema: &JsonSchema = schema.properties.get("tags").expect("tags property");
    assert_eq!(tags_schema.type_.as_deref(), Some("array"));
    let items: &JsonSchema = tags_schema.items.as_ref().expect("items").as_ref();
    assert_eq!(items.type_.as_deref(), Some("string"));

    let counts_schema: &JsonSchema = schema.properties.get("counts").expect("counts property");
    assert_eq!(counts_schema.type_.as_deref(), Some("array"));
    let count_items: &JsonSchema = counts_schema.items.as_ref().expect("items").as_ref();
    assert_eq!(count_items.type_.as_deref(), Some("integer"));
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithHashSetField {
    tags: HashSet<String>,
}

#[test]
fn derive_struct_with_hash_set_field_emits_unique_items_true() {
    let schema: JsonSchema = WithHashSetField::json_schema();
    let tags_schema: &JsonSchema = schema.properties.get("tags").expect("tags property");
    assert_eq!(tags_schema.type_.as_deref(), Some("array"));
    assert_eq!(tags_schema.unique_items, Some(true));
    let items: &JsonSchema = tags_schema.items.as_ref().expect("items").as_ref();
    assert_eq!(items.type_.as_deref(), Some("string"));
}
