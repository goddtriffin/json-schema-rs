//! Integration test: `#[derive(ToJsonSchema)]` produces the expected JSON Schema.

use json_schema_rs::json_schema::json_schema::AdditionalProperties;
use json_schema_rs::{JsonSchema, ToJsonSchema};
use json_schema_rs_macro::ToJsonSchema;
use std::collections::{BTreeMap, HashSet};

#[derive(ToJsonSchema)]
#[json_schema(title = "Root")]
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
struct RootWithAddress {
    address: Address,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct TwoAddresses {
    addr1: Address,
    addr2: Address,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Tree {
    children: Vec<Tree>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithOptionalAddress {
    address: Option<Address>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithAddressList {
    addresses: Vec<Address>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Inner {
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Outer {
    b: Inner,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct RootWithNested {
    a: Outer,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "http://example.com/with-id")]
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

#[derive(ToJsonSchema)]
#[expect(dead_code)]
enum SingleVariant {
    Only,
}

#[test]
fn derive_root_json_schema() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("id".to_string(), String::json_schema());
            m.insert("name".to_string(), Option::<String>::json_schema());
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["id".to_string()]),
        title: Some("Root".to_string()),
        ..Default::default()
    };
    let actual: JsonSchema = Root::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_address_json_schema() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("city".to_string(), String::json_schema());
            m.insert("street".to_string(), String::json_schema());
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = Address::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_nested_struct_emits_defs_and_ref() {
    let address_schema: JsonSchema = Address::json_schema();
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["address".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = RootWithAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_two_fields_same_type_one_defs_entry() {
    let address_schema: JsonSchema = Address::json_schema();
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "addr1".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "addr2".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["addr1".to_string(), "addr2".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = TwoAddresses::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_recursive_type_emits_defs_and_ref_no_stack_overflow() {
    let tree_def_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "children".to_string(),
                JsonSchema {
                    type_: Some("array".to_string()),
                    items: Some(Box::new(JsonSchema {
                        ref_: Some("#/$defs/Tree".to_string()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["children".to_string()]),
        ..Default::default()
    };
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Tree".to_string(), tree_def_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "children".to_string(),
                JsonSchema {
                    type_: Some("array".to_string()),
                    items: Some(Box::new(JsonSchema {
                        ref_: Some("#/$defs/Tree".to_string()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["children".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = Tree::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_option_nested_struct_emits_defs() {
    let address_schema: JsonSchema = Address::json_schema();
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: None,
        ..Default::default()
    };
    let actual: JsonSchema = WithOptionalAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_vec_nested_struct_emits_defs() {
    let address_schema: JsonSchema = Address::json_schema();
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "addresses".to_string(),
                JsonSchema {
                    type_: Some("array".to_string()),
                    items: Some(Box::new(JsonSchema {
                        ref_: Some("#/$defs/Address".to_string()),
                        ..Default::default()
                    })),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["addresses".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithAddressList::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_three_level_nested_emits_flat_defs() {
    use json_schema_rs::reverse_code_gen::merge_nested_defs_into_root;

    let inner_schema: JsonSchema = Inner::json_schema();
    let mut temp_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
    let outer_schema: JsonSchema =
        merge_nested_defs_into_root(Outer::json_schema(), &mut temp_defs);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Inner".to_string(), inner_schema);
            m.insert("Outer".to_string(), outer_schema);
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Outer".to_string()),
                    ..Default::default()
                },
            );
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["a".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = RootWithNested::json_schema();
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
fn derive_single_variant_enum_emits_const() {
    let actual: JsonSchema = SingleVariant::json_schema();
    let expected_type: Option<&str> = Some("string");
    let actual_type: Option<&str> = actual.type_.as_deref();
    assert_eq!(expected_type, actual_type);
    let expected_const: Option<&serde_json::Value> =
        Some(&serde_json::Value::String("Only".to_string()));
    let actual_const: Option<&serde_json::Value> = actual.const_value.as_ref();
    assert_eq!(expected_const, actual_const);
    let expected_enum_none: bool = true;
    let actual_enum_none: bool = actual.enum_values.is_none();
    assert_eq!(expected_enum_none, actual_enum_none);
}

#[test]
fn derive_serialize_round_trip() {
    let schema: JsonSchema = Root::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(schema, parsed);
}

#[derive(ToJsonSchema)]
#[json_schema(description = "From attribute")]
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
#[json_schema(comment = "Created by X")]
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
#[json_schema(description = "Struct with description attribute")]
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
#[json_schema(description = "Enum description")]
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
    let parsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(schema, parsed);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithIntegerMinMax {
    #[json_schema(minimum = 0, maximum = 255)]
    byte: i64,
}

#[test]
fn derive_field_minimum_maximum_integer() {
    let mut byte_schema: JsonSchema = i64::json_schema();
    byte_schema.minimum = Some(0.0);
    byte_schema.maximum = Some(255.0);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("byte".to_string(), byte_schema);
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["byte".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithIntegerMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithFloatMinMax {
    #[json_schema(minimum = 0.0, maximum = 100.0)]
    score: f64,
}

#[test]
fn derive_field_minimum_maximum_float() {
    let mut score_schema: JsonSchema = f64::json_schema();
    score_schema.minimum = Some(0.0);
    score_schema.maximum = Some(100.0);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("score".to_string(), score_schema);
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["score".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithFloatMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_minimum_maximum_round_trip() {
    let schema: JsonSchema = WithIntegerMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(schema, parsed);
}

#[test]
fn derive_minimum_maximum_float_round_trip() {
    let schema: JsonSchema = WithFloatMinMax::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(schema, parsed);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithOnlyMinimum {
    #[json_schema(minimum = 10)]
    value: i64,
}

#[test]
fn derive_field_only_minimum() {
    let mut value_schema: JsonSchema = i64::json_schema();
    value_schema.minimum = Some(10.0);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["value".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithOnlyMinimum::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithOnlyMaximum {
    #[json_schema(maximum = 90)]
    value: i64,
}

#[test]
fn derive_field_only_maximum() {
    let mut value_schema: JsonSchema = i64::json_schema();
    value_schema.maximum = Some(90.0);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema);
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["value".to_string()]),
        ..Default::default()
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
    #[json_schema(min_items = 1, max_items = 10)]
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
    #[json_schema(min_items = 2, max_items = 5)]
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
    #[json_schema(min_length = 3)]
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
    #[json_schema(max_length = 10)]
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
    #[json_schema(min_length = 2, max_length = 50)]
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
    let expected: (Option<u64>, Option<u64>, Option<String>) = (None, None, None);
    let actual: (Option<u64>, Option<u64>, Option<String>) = (
        name_schema.min_length,
        name_schema.max_length,
        name_schema.pattern.clone(),
    );
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStringPattern {
    #[json_schema(pattern = "^x+$")]
    name: String,
}

#[test]
fn macro_derive_to_json_schema_string_pattern() {
    let schema: JsonSchema = WithStringPattern::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    let expected: Option<String> = Some("^x+$".to_string());
    let actual: Option<String> = name_schema.pattern.clone();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithDefaultField {
    #[json_schema(default = 42)]
    count: Option<i64>,
    #[json_schema(default = "foo")]
    name: Option<String>,
}

#[test]
fn derive_field_default_count_emits_default_in_schema() {
    let schema: JsonSchema = WithDefaultField::json_schema();
    let count_schema: &JsonSchema = schema.properties.get("count").expect("count property");
    let expected: Option<serde_json::Value> = Some(serde_json::json!(42));
    let actual: Option<serde_json::Value> = count_schema.default_value.clone();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_default_name_emits_default_in_schema() {
    let schema: JsonSchema = WithDefaultField::json_schema();
    let name_schema: &JsonSchema = schema.properties.get("name").expect("name property");
    let expected: Option<serde_json::Value> = Some(serde_json::json!("foo"));
    let actual: Option<serde_json::Value> = name_schema.default_value.clone();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_default_round_trip_serializes_and_parses() {
    let schema: JsonSchema = WithDefaultField::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let reparsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    let expected: JsonSchema = schema.clone();
    let actual: JsonSchema = reparsed;
    assert_eq!(expected, actual);
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

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithDeprecatedField {
    #[json_schema(deprecated = true)]
    legacy: String,
}

#[test]
fn derive_field_deprecated_emits_deprecated_in_schema() {
    let mut legacy_schema: JsonSchema = String::json_schema();
    legacy_schema.deprecated = Some(true);
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("legacy".to_string(), legacy_schema);
            m
        },
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["legacy".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithDeprecatedField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_deprecated_round_trip() {
    let schema: JsonSchema = WithDeprecatedField::json_schema();
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    let expected: JsonSchema = schema;
    let actual: JsonSchema = parsed;
    assert_eq!(expected, actual);
}
