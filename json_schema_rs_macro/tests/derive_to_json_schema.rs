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
    let address_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([
            ("city".to_string(), String::json_schema()),
            ("street".to_string(), String::json_schema()),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
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
    let address_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([
            ("city".to_string(), String::json_schema()),
            ("street".to_string(), String::json_schema()),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
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
    let address_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([
            ("city".to_string(), String::json_schema()),
            ("street".to_string(), String::json_schema()),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
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
    let address_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([
            ("city".to_string(), String::json_schema()),
            ("street".to_string(), String::json_schema()),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
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
    let inner_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([("x".to_string(), String::json_schema())]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["x".to_string()]),
        ..Default::default()
    };
    let outer_schema: JsonSchema = JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([(
            "b".to_string(),
            JsonSchema {
                ref_: Some("#/$defs/Inner".to_string()),
                ..Default::default()
            },
        )]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["b".to_string()]),
        ..Default::default()
    };
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

// A type with `id` is its own schema resource: inside it, `#/$defs/...` resolves against its
// `$id` (2020-12 core §9.2.1), so it carries the `$defs` its refs name and refers to itself as
// `#`. Each expected schema was checked with jsonschema-cli 0.60.2 (it accepts a valid instance
// and rejects an invalid one).

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct ResourceLeaf {
    x: String,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/with-dep")]
#[expect(dead_code)]
struct ResourceWithDep {
    leaf: ResourceLeaf,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/no-dep")]
#[expect(dead_code)]
struct ResourceNoDep {
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceNoDep {
    inner: ResourceNoDep,
}

fn schema_from_json(value: serde_json::Value) -> JsonSchema {
    serde_json::from_value::<JsonSchema>(value).expect("valid schema")
}

#[test]
fn derive_id_resource_as_root_keeps_its_defs_at_its_own_root() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/with-dep",
        "type": "object",
        "properties": { "leaf": { "$ref": "#/$defs/ResourceLeaf" } },
        "additionalProperties": false,
        "required": ["leaf"],
        "$defs": {
            "ResourceLeaf": {
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    }));
    let actual: JsonSchema = ResourceWithDep::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_nested_id_resource_without_named_dependencies() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "inner": { "$ref": "#/$defs/ResourceNoDep" } },
        "additionalProperties": false,
        "required": ["inner"],
        "$defs": {
            "ResourceNoDep": {
                "$id": "https://example.com/no-dep",
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    }));
    let actual: JsonSchema = HoldsResourceNoDep::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceWithDep {
    inner: ResourceWithDep,
}

#[test]
fn nested_id_resource_carries_the_defs_its_refs_need() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "inner": { "$ref": "#/$defs/ResourceWithDep" } },
        "additionalProperties": false,
        "required": ["inner"],
        "$defs": {
            "ResourceWithDep": {
                "$id": "https://example.com/with-dep",
                "type": "object",
                "properties": { "leaf": { "$ref": "#/$defs/ResourceLeaf" } },
                "additionalProperties": false,
                "required": ["leaf"],
                "$defs": {
                    "ResourceLeaf": {
                        "type": "object",
                        "properties": { "x": { "type": "string" } },
                        "additionalProperties": false,
                        "required": ["x"]
                    }
                }
            }
        }
    }));
    let actual: JsonSchema = HoldsResourceWithDep::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/node")]
#[expect(dead_code)]
struct ResourceNode {
    next: Option<Box<ResourceNode>>,
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceNode {
    node: ResourceNode,
}

#[test]
fn nested_recursive_id_resource_refers_to_its_own_root() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "node": { "$ref": "#/$defs/ResourceNode" } },
        "additionalProperties": false,
        "required": ["node"],
        "$defs": {
            "ResourceNode": {
                "$id": "https://example.com/node",
                "type": "object",
                "properties": {
                    "next": { "$ref": "#" },
                    "x": { "type": "string" }
                },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    }));
    let actual: JsonSchema = HoldsResourceNode::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn recursive_id_resource_as_root_refers_to_itself_as_hash() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/node",
        "type": "object",
        "properties": {
            "next": { "$ref": "#" },
            "x": { "type": "string" }
        },
        "additionalProperties": false,
        "required": ["x"]
    }));
    let actual: JsonSchema = ResourceNode::json_schema();
    assert_eq!(expected, actual);
}

// A resource whose own `$defs` entry refers back to the resource: `#` inside that entry
// resolves against the resource too.
#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/tree")]
#[expect(dead_code)]
struct ResourceTree {
    branches: Vec<ResourceBranch>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct ResourceBranch {
    subtree: Option<Box<ResourceTree>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceTree {
    tree: ResourceTree,
}

#[test]
fn nested_id_resource_dependency_refers_back_to_the_resource_as_hash() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "tree": { "$ref": "#/$defs/ResourceTree" } },
        "additionalProperties": false,
        "required": ["tree"],
        "$defs": {
            "ResourceTree": {
                "$id": "https://example.com/tree",
                "type": "object",
                "properties": {
                    "branches": { "type": "array", "items": { "$ref": "#/$defs/ResourceBranch" } }
                },
                "additionalProperties": false,
                "required": ["branches"],
                "$defs": {
                    "ResourceBranch": {
                        "type": "object",
                        "properties": { "subtree": { "$ref": "#" } },
                        "additionalProperties": false
                    }
                }
            }
        }
    }));
    let actual: JsonSchema = HoldsResourceTree::json_schema();
    assert_eq!(expected, actual);
}

// A hand-written resource with its own `$defs`, valid on its own.
struct HandWrittenResource;

impl ToJsonSchema for HandWrittenResource {
    fn json_schema() -> JsonSchema {
        schema_from_json(serde_json::json!({
            "$id": "https://example.com/hand",
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/Str" } },
            "required": ["x"],
            "$defs": { "Str": { "type": "string" } }
        }))
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsHandWrittenResource {
    hand: HandWrittenResource,
}

#[test]
fn hand_written_id_resource_keeps_its_own_defs() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "hand": HandWrittenResource::json_schema() },
        "additionalProperties": false,
        "required": ["hand"]
    }));
    let actual: JsonSchema = HoldsHandWrittenResource::json_schema();
    assert_eq!(expected, actual);
}

// Two `$id` types that refer to each other: each is embedded once at the document root, and
// refers to the other by its `$id`.
#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/ping")]
#[expect(dead_code)]
struct ResourcePing {
    pong: Option<Box<ResourcePong>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/pong")]
#[expect(dead_code)]
struct ResourcePong {
    ping: Option<Box<ResourcePing>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourcePing {
    ping: ResourcePing,
}

#[test]
fn mutually_referencing_id_resources_refer_to_each_other_by_id() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "ping": { "$ref": "#/$defs/ResourcePing" } },
        "additionalProperties": false,
        "required": ["ping"],
        "$defs": {
            "ResourcePing": {
                "$id": "https://example.com/ping",
                "type": "object",
                "properties": { "pong": { "$ref": "https://example.com/pong" } },
                "additionalProperties": false
            },
            "ResourcePong": {
                "$id": "https://example.com/pong",
                "type": "object",
                "properties": { "ping": { "$ref": "https://example.com/ping" } },
                "additionalProperties": false
            }
        }
    }));
    let actual: JsonSchema = HoldsResourcePing::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn mutually_referencing_id_resource_as_root_embeds_the_other_in_its_defs() {
    let expected: JsonSchema = schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/ping",
        "type": "object",
        "properties": { "pong": { "$ref": "#/$defs/ResourcePong" } },
        "additionalProperties": false,
        "$defs": {
            "ResourcePong": {
                "$id": "https://example.com/pong",
                "type": "object",
                "properties": { "ping": { "$ref": "https://example.com/ping" } },
                "additionalProperties": false
            }
        }
    }));
    let actual: JsonSchema = ResourcePing::json_schema();
    assert_eq!(expected, actual);
}

// One `$id` type used directly and from inside another resource: embedded once.
#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/shared")]
#[expect(dead_code)]
struct SharedResource {
    x: String,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/uses-shared")]
#[expect(dead_code)]
struct UsesSharedResource {
    shared: SharedResource,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsSharedResourceDirectFirst {
    direct: SharedResource,
    via: UsesSharedResource,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsSharedResourceViaFirst {
    via: UsesSharedResource,
    direct: SharedResource,
}

fn shared_resource_expected_schema() -> JsonSchema {
    schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "direct": { "$ref": "#/$defs/SharedResource" },
            "via": { "$ref": "#/$defs/UsesSharedResource" }
        },
        "additionalProperties": false,
        "required": ["direct", "via"],
        "$defs": {
            "SharedResource": {
                "$id": "https://example.com/shared",
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            },
            "UsesSharedResource": {
                "$id": "https://example.com/uses-shared",
                "type": "object",
                "properties": { "shared": { "$ref": "https://example.com/shared" } },
                "additionalProperties": false,
                "required": ["shared"]
            }
        }
    }))
}

#[test]
fn id_resource_used_from_two_places_is_embedded_once_when_first_used_directly() {
    let expected: JsonSchema = shared_resource_expected_schema();
    let actual: JsonSchema = HoldsSharedResourceDirectFirst::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn id_resource_used_from_two_places_is_embedded_once_when_first_used_inside_a_resource() {
    let expected: JsonSchema = JsonSchema {
        required: Some(vec!["via".to_string(), "direct".to_string()]),
        ..shared_resource_expected_schema()
    };
    let actual: JsonSchema = HoldsSharedResourceViaFirst::json_schema();
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
    byte_schema.minimum = Some(0_u64.into());
    byte_schema.maximum = Some(255_u64.into());
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
    score_schema.minimum = serde_json::Number::from_f64(0.0);
    score_schema.maximum = serde_json::Number::from_f64(100.0);
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
    value_schema.minimum = Some(10_u64.into());
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
    value_schema.maximum = Some(90_u64.into());
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
struct WithExactBounds {
    #[json_schema(minimum = -9223372036854775808, maximum = 18446744073709551615)]
    wide: i64,
    #[json_schema(exclusive_minimum = -1.5, exclusive_maximum = 9007199254740993, multiple_of = 0.25)]
    fine: f64,
}

#[test]
fn derive_numeric_attributes_are_exact() {
    let expected: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"fine":{"type":"number","minimum":-1.7976931348623157e+308,"maximum":1.7976931348623157e+308,"exclusiveMinimum":-1.5,"exclusiveMaximum":9007199254740993,"multipleOf":0.25},"wide":{"type":"integer","minimum":-9223372036854775808,"maximum":18446744073709551615}},"additionalProperties":false,"required":["wide","fine"]}"#;
    let actual: String = (&WithExactBounds::json_schema())
        .try_into()
        .expect("serialize");
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

#[cfg(feature = "uuid")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithUuidVec {
    ids: Vec<uuid::Uuid>,
}

#[cfg(feature = "uuid")]
#[test]
fn derive_uuid_vec_field_json_schema() {
    let expected: JsonSchema = JsonSchema {
        type_: Some("array".to_string()),
        items: Some(Box::new(JsonSchema {
            type_: Some("string".to_string()),
            format: Some("uuid".to_string()),
            ..Default::default()
        })),
        ..Default::default()
    };
    let actual: JsonSchema = WithUuidVec::json_schema()
        .properties
        .get("ids")
        .expect("ids property")
        .clone();
    assert_eq!(expected, actual);
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

// --- Recursion through the generator context (#4) ---

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct MutualA {
    bs: Vec<MutualB>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct MutualB {
    as_: Vec<MutualA>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleA {
    bs: Vec<CycleB>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleB {
    cs: Vec<CycleC>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleC {
    as_: Vec<CycleA>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct OverriddenNode {
    #[json_schema(min_items = 1)]
    children: Vec<OverriddenNode>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DeprecatedAddress {
    #[json_schema(deprecated = true)]
    address: Address,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithStatus {
    status: Status,
}

fn ref_to(name: &str) -> JsonSchema {
    JsonSchema {
        ref_: Some(format!("#/$defs/{name}")),
        ..Default::default()
    }
}

fn array_of_ref(name: &str) -> JsonSchema {
    JsonSchema {
        type_: Some("array".to_string()),
        items: Some(Box::new(ref_to(name))),
        ..Default::default()
    }
}

/// Body of a derived struct with one required property `key` whose schema is `property`.
fn one_property_body(key: &str, property: JsonSchema) -> JsonSchema {
    JsonSchema {
        type_: Some("object".to_string()),
        properties: BTreeMap::from([(key.to_string(), property)]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec![key.to_string()]),
        ..Default::default()
    }
}

#[test]
fn derive_mutual_recursion_emits_both_defs_linked_by_ref() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")),
            ),
        ])),
        ..one_property_body("bs", array_of_ref("MutualB"))
    };
    let actual: JsonSchema = MutualA::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_mutual_recursion_from_other_root_emits_both_defs() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")),
            ),
        ])),
        ..one_property_body("as_", array_of_ref("MutualA"))
    };
    let actual: JsonSchema = MutualB::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_three_type_cycle_emits_all_defs() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "CycleA".to_string(),
                one_property_body("bs", array_of_ref("CycleB")),
            ),
            (
                "CycleB".to_string(),
                one_property_body("cs", array_of_ref("CycleC")),
            ),
            (
                "CycleC".to_string(),
                one_property_body("as_", array_of_ref("CycleA")),
            ),
        ])),
        ..one_property_body("bs", array_of_ref("CycleB"))
    };
    let actual: JsonSchema = CycleA::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_self_reference_with_field_override_keeps_ref_and_override() {
    let children: JsonSchema = JsonSchema {
        min_items: Some(1),
        ..array_of_ref("OverriddenNode")
    };
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "OverriddenNode".to_string(),
            one_property_body("children", children.clone()),
        )])),
        ..one_property_body("children", children)
    };
    let actual: JsonSchema = OverriddenNode::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_override_on_named_type_is_ref_sibling() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Address".to_string(),
            JsonSchema {
                type_: Some("object".to_string()),
                properties: BTreeMap::from([
                    ("city".to_string(), String::json_schema()),
                    ("street".to_string(), String::json_schema()),
                ]),
                additional_properties: Some(AdditionalProperties::Forbid),
                required: Some(vec!["city".to_string(), "street".to_string()]),
                ..Default::default()
            },
        )])),
        ..one_property_body(
            "address",
            JsonSchema {
                deprecated: Some(true),
                ..ref_to("Address")
            },
        )
    };
    let actual: JsonSchema = DeprecatedAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_enum_field_emits_defs_entry_without_schema_keyword() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Status".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                enum_values: Some(vec![
                    serde_json::Value::String("Open".to_string()),
                    serde_json::Value::String("Closed".to_string()),
                ]),
                ..Default::default()
            },
        )])),
        ..one_property_body("status", ref_to("Status"))
    };
    let actual: JsonSchema = WithStatus::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn vec_of_derived_type_json_schema_is_standalone_with_defs() {
    let expected: JsonSchema = JsonSchema {
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")),
            ),
        ])),
        ..array_of_ref("MutualA")
    };
    let actual: JsonSchema = Vec::<MutualA>::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
struct Empty {}

#[test]
fn derive_struct_without_fields_json_schema() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        additional_properties: Some(AdditionalProperties::Forbid),
        ..Default::default()
    };
    let actual: JsonSchema = Empty::json_schema();
    assert_eq!(expected, actual);
}

// --- $defs keyed by type identity (#24) ---

mod first {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Config {
        name: String,
    }
}

mod second {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Config {
        port: i64,
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct TwoConfigs {
    a: first::Config,
    b: second::Config,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct RepeatedConfig {
    a: first::Config,
    b: second::Config,
    c: first::Config,
}

#[test]
fn derive_schema_id_is_module_path_and_type_name() {
    let expected: &str = "derive_to_json_schema::first::Config";
    let actual: &str = first::Config::schema_id();
    assert_eq!(expected, actual);
}

#[test]
fn derive_same_name_types_get_suffixed_defs_entries() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: Some(BTreeMap::from([
            (
                "Config0".to_string(),
                one_property_body("name", String::json_schema()),
            ),
            (
                "Config1".to_string(),
                one_property_body("port", i64::json_schema()),
            ),
        ])),
        properties: BTreeMap::from([
            ("a".to_string(), ref_to("Config0")),
            ("b".to_string(), ref_to("Config1")),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["a".to_string(), "b".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = TwoConfigs::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_same_name_type_used_twice_shares_one_suffixed_entry() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: Some(BTreeMap::from([
            (
                "Config0".to_string(),
                one_property_body("name", String::json_schema()),
            ),
            (
                "Config1".to_string(),
                one_property_body("port", i64::json_schema()),
            ),
        ])),
        properties: BTreeMap::from([
            ("a".to_string(), ref_to("Config0")),
            ("b".to_string(), ref_to("Config1")),
            ("c".to_string(), ref_to("Config0")),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = RepeatedConfig::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_unique_name_stays_unsuffixed_when_another_module_reuses_it_elsewhere() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Config".to_string(),
            one_property_body("name", String::json_schema()),
        )])),
        ..one_property_body("config", ref_to("Config"))
    };
    let actual: JsonSchema = OneConfig::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct OneConfig {
    config: first::Config,
}

#[cfg(feature = "uuid")]
mod user {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Uuid {
        value: String,
    }
}

#[cfg(feature = "uuid")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithUserUuid {
    id: user::Uuid,
    library_id: uuid::Uuid,
}

#[cfg(feature = "uuid")]
#[test]
fn derive_user_type_named_uuid_gets_defs_entry_beside_inlined_library_uuid() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some("object".to_string()),
        defs: Some(BTreeMap::from([(
            "Uuid".to_string(),
            one_property_body("value", String::json_schema()),
        )])),
        properties: BTreeMap::from([
            ("id".to_string(), ref_to("Uuid")),
            (
                "library_id".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    format: Some("uuid".to_string()),
                    ..Default::default()
                },
            ),
        ]),
        additional_properties: Some(AdditionalProperties::Forbid),
        required: Some(vec!["id".to_string(), "library_id".to_string()]),
        ..Default::default()
    };
    let actual: JsonSchema = WithUserUuid::json_schema();
    assert_eq!(expected, actual);
}

/// Struct doc.
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocContainer {
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocField {
    /// Field doc.
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocNamedTypeField {
    /// Where they live.
    address: Address,
}

/// First line.
///   Indented second line.
///
/// Second paragraph.
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocMultiLine {
    x: String,
}

/// From doc.
#[derive(ToJsonSchema)]
#[json_schema(description = "From attribute")]
#[expect(dead_code)]
struct DocAndAttrDescription {
    x: String,
}

/// Enum doc.
#[derive(ToJsonSchema)]
#[expect(dead_code)]
enum DocEnum {
    A,
    B,
}

#[doc = concat!("Con", "cat.")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocConcat {
    #[doc = include_str!("fixtures/doc.txt")]
    x: String,
}

///
#[derive(ToJsonSchema)]
#[expect(dead_code, clippy::empty_docs)]
struct DocEmpty {
    ///
    x: String,
}

#[doc(hidden)]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DocHiddenOnly {
    x: String,
}

#[test]
fn derive_container_doc_comment_becomes_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Struct doc.".to_string()),
        ..one_property_body("x", String::json_schema())
    };
    let actual: JsonSchema = DocContainer::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_doc_comment_becomes_property_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body(
            "x",
            JsonSchema {
                description: Some("Field doc.".to_string()),
                ..String::json_schema()
            },
        )
    };
    let actual: JsonSchema = DocField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_doc_comment_on_named_type_is_ref_sibling() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Address".to_string(),
            JsonSchema {
                type_: Some("object".to_string()),
                properties: BTreeMap::from([
                    ("city".to_string(), String::json_schema()),
                    ("street".to_string(), String::json_schema()),
                ]),
                additional_properties: Some(AdditionalProperties::Forbid),
                required: Some(vec!["city".to_string(), "street".to_string()]),
                ..Default::default()
            },
        )])),
        ..one_property_body(
            "address",
            JsonSchema {
                description: Some("Where they live.".to_string()),
                ..ref_to("Address")
            },
        )
    };
    let actual: JsonSchema = DocNamedTypeField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_multi_line_doc_comment_lines_are_trimmed_and_joined() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("First line.\nIndented second line.\n\nSecond paragraph.".to_string()),
        ..one_property_body("x", String::json_schema())
    };
    let actual: JsonSchema = DocMultiLine::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_description_attribute_takes_precedence_over_doc_comment() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("From attribute".to_string()),
        ..one_property_body("x", String::json_schema())
    };
    let actual: JsonSchema = DocAndAttrDescription::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_enum_doc_comment_becomes_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Enum doc.".to_string()),
        type_: Some("string".to_string()),
        enum_values: Some(vec![
            serde_json::Value::String("A".to_string()),
            serde_json::Value::String("B".to_string()),
        ]),
        ..Default::default()
    };
    let actual: JsonSchema = DocEnum::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_macro_doc_attributes_are_evaluated_into_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Concat.".to_string()),
        ..one_property_body(
            "x",
            JsonSchema {
                description: Some("Included doc.".to_string()),
                ..String::json_schema()
            },
        )
    };
    let actual: JsonSchema = DocConcat::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_empty_doc_comment_yields_no_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body("x", String::json_schema())
    };
    let actual: JsonSchema = DocEmpty::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_list_form_doc_attribute_yields_no_description() {
    let expected: JsonSchema = JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body("x", String::json_schema())
    };
    let actual: JsonSchema = DocHiddenOnly::json_schema();
    assert_eq!(expected, actual);
}
