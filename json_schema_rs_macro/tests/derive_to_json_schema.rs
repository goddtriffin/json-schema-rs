//! Integration test: `#[derive(ToJsonSchema)]` produces the expected JSON Schema.

use json_schema_rs::{JsonSchema, JsonSchemaSettings, ToJsonSchema, parse_schema_from_str};
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
#[to_json_schema(id = "http://example.com/with-id")]
#[expect(dead_code)]
struct WithId {
    value: String,
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
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
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
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
    };
    let actual: JsonSchema = Root::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_address_json_schema() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
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
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
    };
    let actual: JsonSchema = Address::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_id_attribute_emits_id_in_schema() {
    let actual: JsonSchema = WithId::json_schema();
    let expected_id: Option<String> = Some("http://example.com/with-id".to_string());
    assert_eq!(expected_id, actual.id);
    let json: String = (&actual).try_into().expect("serialize");
    assert!(json.contains("\"$id\":\"http://example.com/with-id\""));
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
    let parsed: JsonSchema = parse_schema_from_str(&json, &settings).expect("parse");
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
#[to_json_schema(comment = "Created by X")]
#[expect(dead_code)]
struct WithCommentAttr {
    id: String,
}

#[test]
fn derive_attribute_comment() {
    let actual: JsonSchema = WithCommentAttr::json_schema();
    let expected_comment: Option<String> = Some("Created by X".to_string());
    assert_eq!(expected_comment, actual.comment);
    let json: String = (&actual).try_into().expect("serialize");
    assert!(
        json.contains(r#""$comment":"Created by X""#),
        "serialized schema should contain $comment; got: {json}"
    );
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
    let parsed: JsonSchema = parse_schema_from_str(&json, &settings).expect("parse");
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
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("byte".to_string(), byte_schema);
            m
        },
        required: Some(vec!["byte".to_string()]),
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
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
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("score".to_string(), score_schema);
            m
        },
        required: Some(vec!["score".to_string()]),
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
    };
    let actual: JsonSchema = WithFloatMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_minimum_maximum_round_trip() {
    let schema: JsonSchema = WithIntegerMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema_from_str(&json, &settings).expect("parse");
    assert_eq!(schema, parsed);
}

#[test]
fn derive_minimum_maximum_float_round_trip() {
    let schema: JsonSchema = WithFloatMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
    let parsed: JsonSchema = parse_schema_from_str(&json, &settings).expect("parse");
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
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        required: Some(vec!["value".to_string()]),
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
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
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        id: None,
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        required: Some(vec!["value".to_string()]),
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: None,
        maximum: None,
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
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

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithVecFieldMinMax {
    #[to_json_schema(min_items = 1, max_items = 10)]
    tags: Vec<String>,
}

#[test]
fn derive_struct_with_vec_field_min_items_max_items_emits_bounds() {
    let schema: JsonSchema = WithVecFieldMinMax::json_schema();
    let tags_schema: &JsonSchema = schema.properties.get("tags").expect("tags property");
    let expected_min: Option<u64> = Some(1);
    let expected_max: Option<u64> = Some(10);
    let actual_min: Option<u64> = tags_schema.min_items;
    let actual_max: Option<u64> = tags_schema.max_items;
    assert_eq!(expected_min, actual_min);
    assert_eq!(expected_max, actual_max);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithHashSetFieldMinMax {
    #[to_json_schema(min_items = 2, max_items = 5)]
    ids: HashSet<String>,
}

#[test]
fn derive_struct_with_hash_set_field_min_items_max_items_emits_bounds() {
    let schema: JsonSchema = WithHashSetFieldMinMax::json_schema();
    let ids_schema: &JsonSchema = schema.properties.get("ids").expect("ids property");
    let expected_min: Option<u64> = Some(2);
    let expected_max: Option<u64> = Some(5);
    let actual_min: Option<u64> = ids_schema.min_items;
    let actual_max: Option<u64> = ids_schema.max_items;
    assert_eq!(expected_min, actual_min);
    assert_eq!(expected_max, actual_max);
}

#[test]
fn vec_string_json_schema_has_min_items_max_items_none() {
    let actual: JsonSchema = Vec::<String>::json_schema();
    let expected_min: Option<u64> = None;
    let expected_max: Option<u64> = None;
    assert_eq!(expected_min, actual.min_items);
    assert_eq!(expected_max, actual.max_items);
}

#[test]
fn hash_set_string_json_schema_has_min_items_max_items_none() {
    let actual: JsonSchema = HashSet::<String>::json_schema();
    let expected_min: Option<u64> = None;
    let expected_max: Option<u64> = None;
    assert_eq!(expected_min, actual.min_items);
    assert_eq!(expected_max, actual.max_items);
}
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStringMinLength {
    #[to_json_schema(min_length = 3)]
    name: String,
}

#[test]
fn macro_derive_to_json_schema_string_min_length() {
    let schema: JsonSchema = WithStringMinLength::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    assert_eq!(Some(3), name_schema.min_length);
    assert_eq!(None, name_schema.max_length);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStringMaxLength {
    #[to_json_schema(max_length = 10)]
    name: String,
}

#[test]
fn macro_derive_to_json_schema_string_max_length() {
    let schema: JsonSchema = WithStringMaxLength::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    assert_eq!(None, name_schema.min_length);
    assert_eq!(Some(10), name_schema.max_length);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStringBothLengths {
    #[to_json_schema(min_length = 2, max_length = 50)]
    name: String,
}

#[test]
fn macro_derive_to_json_schema_string_min_length_max_length() {
    let schema: JsonSchema = WithStringBothLengths::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    assert_eq!(Some(2), name_schema.min_length);
    assert_eq!(Some(50), name_schema.max_length);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStringNoLengthConstraints {
    name: String,
}

#[test]
fn macro_derive_to_json_schema_string_no_length_constraints() {
    let schema: JsonSchema = WithStringNoLengthConstraints::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    assert_eq!(None, name_schema.min_length);
    assert_eq!(None, name_schema.max_length);
}

#[cfg(feature = "uuid")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithUuid {
    id: uuid::Uuid,
    secondary_id: Option<uuid::Uuid>,
}

#[cfg(feature = "uuid")]
#[test]
fn derive_uuid_field_json_schema() {
    let schema: JsonSchema = WithUuid::json_schema();
    let id_schema: &JsonSchema = schema.properties.get("id").expect("id property");
    assert_eq!(id_schema.type_.as_deref(), Some("string"));
    assert_eq!(id_schema.format.as_deref(), Some("uuid"));
    let secondary_schema: &JsonSchema = schema
        .properties
        .get("secondary_id")
        .expect("secondary_id property");
    assert_eq!(secondary_schema.type_.as_deref(), Some("string"));
    assert_eq!(secondary_schema.format.as_deref(), Some("uuid"));
    let required = schema.required.as_ref().expect("required");
    assert!(
        required.contains(&"id".to_string()),
        "id should be required"
    );
    assert!(
        !required.contains(&"secondary_id".to_string()),
        "secondary_id should not be required"
    );
}
