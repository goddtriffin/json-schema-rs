//! Integration test: `#[derive(ToJsonSchema)]` produces the expected JSON Schema.

use json_schema_rs::{
    JsonSchema, JsonType, ReverseCodeGenError, ReverseCodeGenResult, Schema, ToJsonSchema, TypeSet,
};
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
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "id".to_string(),
                String::json_schema().expect("schema").into(),
            );
            m.insert(
                "name".to_string(),
                String::json_schema().expect("schema").into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["id".to_string()]),
        title: Some("Root".to_string()),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = Root::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_address_json_schema() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "city".to_string(),
                String::json_schema().expect("schema").into(),
            );
            m.insert(
                "street".to_string(),
                String::json_schema().expect("schema").into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = Address::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_struct_serializes_closed_shape_as_additional_properties_false() {
    let expected: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"city":{"type":"string"},"street":{"type":"string"}},"additionalProperties":false,"required":["city","street"]}"#;
    let actual: String = (&Address::json_schema().expect("schema"))
        .try_into()
        .expect("serialize");
    assert_eq!(expected, actual);
}

#[test]
fn derive_nested_struct_emits_defs_and_ref() {
    let address_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([
            (
                "city".to_string(),
                String::json_schema().expect("schema").into(),
            ),
            (
                "street".to_string(),
                String::json_schema().expect("schema").into(),
            ),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["address".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = RootWithAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_two_fields_same_type_one_defs_entry() {
    let address_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([
            (
                "city".to_string(),
                String::json_schema().expect("schema").into(),
            ),
            (
                "street".to_string(),
                String::json_schema().expect("schema").into(),
            ),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "addr1".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                }
                .into(),
            );
            m.insert(
                "addr2".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["addr1".to_string(), "addr2".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = TwoAddresses::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_recursive_type_emits_defs_and_ref_no_stack_overflow() {
    let tree_def_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "children".to_string(),
                JsonSchema {
                    type_: Some(JsonType::Array.into()),
                    items: Some(
                        JsonSchema {
                            ref_: Some("#/$defs/Tree".to_string()),
                            ..Default::default()
                        }
                        .into(),
                    ),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["children".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Tree".to_string(), tree_def_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "children".to_string(),
                JsonSchema {
                    type_: Some(JsonType::Array.into()),
                    items: Some(
                        JsonSchema {
                            ref_: Some("#/$defs/Tree".to_string()),
                            ..Default::default()
                        }
                        .into(),
                    ),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["children".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = Tree::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_option_nested_struct_emits_defs() {
    let address_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([
            (
                "city".to_string(),
                String::json_schema().expect("schema").into(),
            ),
            (
                "street".to_string(),
                String::json_schema().expect("schema").into(),
            ),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Address".to_string()),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: None,
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithOptionalAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_vec_nested_struct_emits_defs() {
    let address_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([
            (
                "city".to_string(),
                String::json_schema().expect("schema").into(),
            ),
            (
                "street".to_string(),
                String::json_schema().expect("schema").into(),
            ),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["city".to_string(), "street".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Address".to_string(), address_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "addresses".to_string(),
                JsonSchema {
                    type_: Some(JsonType::Array.into()),
                    items: Some(
                        JsonSchema {
                            ref_: Some("#/$defs/Address".to_string()),
                            ..Default::default()
                        }
                        .into(),
                    ),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["addresses".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithAddressList::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_three_level_nested_emits_flat_defs() {
    let inner_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([(
            "x".to_string(),
            String::json_schema().expect("schema").into(),
        )]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["x".to_string()]),
        ..Default::default()
    };
    let outer_schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([(
            "b".to_string(),
            JsonSchema {
                ref_: Some("#/$defs/Inner".to_string()),
                ..Default::default()
            }
            .into(),
        )]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["b".to_string()]),
        ..Default::default()
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: {
            let mut m = BTreeMap::new();
            m.insert("Inner".to_string(), inner_schema.into());
            m.insert("Outer".to_string(), outer_schema.into());
            Some(m)
        },
        properties: {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    ref_: Some("#/$defs/Outer".to_string()),
                    ..Default::default()
                }
                .into(),
            );
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["a".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = RootWithNested::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_id_attribute_emits_id_in_schema() {
    let actual: JsonSchema = WithId::json_schema().expect("schema");
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
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = ResourceWithDep::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_nested_id_resource_without_named_dependencies() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = HoldsResourceNoDep::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceWithDep {
    inner: ResourceWithDep,
}

#[test]
fn nested_id_resource_carries_the_defs_its_refs_need() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = HoldsResourceWithDep::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = HoldsResourceNode::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn recursive_id_resource_as_root_refers_to_itself_as_hash() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/node",
        "type": "object",
        "properties": {
            "next": { "$ref": "#" },
            "x": { "type": "string" }
        },
        "additionalProperties": false,
        "required": ["x"]
    })));
    let actual: ReverseCodeGenResult = ResourceNode::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = HoldsResourceTree::json_schema();
    assert_eq!(expected, actual);
}

// A hand-written resource with its own `$defs`, valid on its own.
#[derive(serde::Serialize)]
struct HandWrittenResource;

impl ToJsonSchema for HandWrittenResource {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(schema_from_json(serde_json::json!({
            "$id": "https://example.com/hand",
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/Str" } },
            "required": ["x"],
            "$defs": { "Str": { "type": "string" } }
        })))
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsHandWrittenResource {
    hand: HandWrittenResource,
}

#[test]
fn hand_written_id_resource_keeps_its_own_defs() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "hand": HandWrittenResource::json_schema().expect("schema") },
        "additionalProperties": false,
        "required": ["hand"]
    })));
    let actual: ReverseCodeGenResult = HoldsHandWrittenResource::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = HoldsResourcePing::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn mutually_referencing_id_resource_as_root_embeds_the_other_in_its_defs() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
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
    })));
    let actual: ReverseCodeGenResult = ResourcePing::json_schema();
    assert_eq!(expected, actual);
}

// Relative `$id`s resolve against the base of the enclosing resource (2020-12 core §8.2.1), so
// a cross-resource `$ref` is written to resolve from the referrer's base: the target's `$id` as
// written when that works, else a relative path, else its absolute URI. Each expected schema
// was checked with jsonschema-cli 0.60.2 and `json_schema_rs::validate` (both accept a valid
// instance and reject one invalid at the recursive depth).

#[derive(ToJsonSchema)]
#[json_schema(id = "a.json")]
#[expect(dead_code)]
struct SameDirA {
    b: Option<Box<SameDirB>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "b.json")]
#[expect(dead_code)]
struct SameDirB {
    a: Option<Box<SameDirA>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsSameDirA {
    a: SameDirA,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "a/ping.json")]
#[expect(dead_code)]
struct NestedDirPing {
    pong: Option<Box<NestedDirPong>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "pong.json")]
#[expect(dead_code)]
struct NestedDirPong {
    ping: Option<Box<NestedDirPing>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsNestedDirPing {
    ping: NestedDirPing,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/ping")]
#[expect(dead_code)]
struct AbsolutePing {
    pong: Option<Box<RelativePong>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "pong.json")]
#[expect(dead_code)]
struct RelativePong {
    ping: Option<Box<AbsolutePing>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsAbsolutePing {
    ping: AbsolutePing,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsAbsolutePingAndRelativePong {
    ping: AbsolutePing,
    pong: RelativePong,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "rel.json")]
#[expect(dead_code)]
struct RelativeToAbsolute {
    abs: Option<Box<AbsoluteFromRelative>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/abs")]
#[expect(dead_code)]
struct AbsoluteFromRelative {
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsRelativeToAbsolute {
    rel: RelativeToAbsolute,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/schemas/root.json")]
#[expect(dead_code)]
struct AbsoluteRoot {
    ping: Option<Box<AbsoluteRootPing>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "a/ping.json")]
#[expect(dead_code)]
struct AbsoluteRootPing {
    pong: Option<Box<AbsoluteRootPong>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "pong.json")]
#[expect(dead_code)]
struct AbsoluteRootPong {
    ping: Option<Box<AbsoluteRootPing>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "root.json")]
#[expect(dead_code)]
struct RelativeRoot {
    child: Option<Box<RelativeRootChild>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "a/child.json")]
#[expect(dead_code)]
struct RelativeRootChild {
    root: Option<Box<RelativeRoot>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "users/user.json")]
#[expect(dead_code)]
struct ParentSegmentUser {
    address: Option<Box<ParentSegmentAddress>>,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "../common/address.json")]
#[expect(dead_code)]
struct ParentSegmentAddress {
    user: Option<Box<ParentSegmentUser>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsParentSegmentUser {
    user: ParentSegmentUser,
}

mod same_id_first {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[json_schema(id = "https://example.com/same")]
    #[expect(dead_code)]
    pub(super) struct SameIdResource {
        x: String,
    }
}

mod same_id_second {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[json_schema(id = "https://example.com/same")]
    #[expect(dead_code)]
    pub(super) struct SameIdResource {
        x: String,
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsSameIdResources {
    first: same_id_first::SameIdResource,
    second: same_id_second::SameIdResource,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "node.json")]
#[expect(dead_code)]
struct ClashingNode {
    x: String,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "./node.json")]
#[expect(dead_code)]
struct ClashingDotNode {
    y: i64,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsClashingNodes {
    node: ClashingNode,
    dot_node: ClashingDotNode,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "has space.json")]
#[expect(dead_code)]
struct InvalidIdResource {
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsInvalidIdResource {
    inner: InvalidIdResource,
}

#[test]
fn relative_id_resources_in_one_directory_refer_to_each_other_by_id() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "a": { "$ref": "#/$defs/SameDirA" } },
        "additionalProperties": false,
        "required": ["a"],
        "$defs": {
            "SameDirA": {
                "$id": "a.json",
                "type": "object",
                "properties": { "b": { "$ref": "b.json" } },
                "additionalProperties": false
            },
            "SameDirB": {
                "$id": "b.json",
                "type": "object",
                "properties": { "a": { "$ref": "a.json" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsSameDirA::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn relative_id_resource_refers_to_a_relative_id_in_another_directory() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "ping": { "$ref": "#/$defs/NestedDirPing" } },
        "additionalProperties": false,
        "required": ["ping"],
        "$defs": {
            "NestedDirPing": {
                "$id": "a/ping.json",
                "type": "object",
                "properties": { "pong": { "$ref": "../pong.json" } },
                "additionalProperties": false
            },
            "NestedDirPong": {
                "$id": "pong.json",
                "type": "object",
                "properties": { "ping": { "$ref": "a/ping.json" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsNestedDirPing::json_schema();
    assert_eq!(expected, actual);
}

// The document root has no `$id`, so `pong.json` at the root would depend on where the
// document is loaded from; the absolute resource carries the copy it refers to.
#[test]
fn absolute_id_resource_embeds_the_relative_id_resource_it_refers_to() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "ping": { "$ref": "#/$defs/AbsolutePing" } },
        "additionalProperties": false,
        "required": ["ping"],
        "$defs": {
            "AbsolutePing": {
                "$id": "https://example.com/ping",
                "type": "object",
                "properties": { "pong": { "$ref": "pong.json" } },
                "additionalProperties": false,
                "$defs": {
                    "RelativePong": {
                        "$id": "pong.json",
                        "type": "object",
                        "properties": { "ping": { "$ref": "https://example.com/ping" } },
                        "additionalProperties": false
                    }
                }
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsAbsolutePing::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn relative_id_resource_used_from_the_root_and_an_absolute_id_resource_is_embedded_in_both() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "ping": { "$ref": "#/$defs/AbsolutePing" },
            "pong": { "$ref": "#/$defs/RelativePong" }
        },
        "additionalProperties": false,
        "required": ["ping", "pong"],
        "$defs": {
            "AbsolutePing": {
                "$id": "https://example.com/ping",
                "type": "object",
                "properties": { "pong": { "$ref": "pong.json" } },
                "additionalProperties": false,
                "$defs": {
                    "RelativePong": {
                        "$id": "pong.json",
                        "type": "object",
                        "properties": { "ping": { "$ref": "https://example.com/ping" } },
                        "additionalProperties": false
                    }
                }
            },
            "RelativePong": {
                "$id": "pong.json",
                "type": "object",
                "properties": { "ping": { "$ref": "https://example.com/ping" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsAbsolutePingAndRelativePong::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn relative_id_resource_refers_to_an_absolute_id_by_id() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "rel": { "$ref": "#/$defs/RelativeToAbsolute" } },
        "additionalProperties": false,
        "required": ["rel"],
        "$defs": {
            "AbsoluteFromRelative": {
                "$id": "https://example.com/abs",
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            },
            "RelativeToAbsolute": {
                "$id": "rel.json",
                "type": "object",
                "properties": { "abs": { "$ref": "https://example.com/abs" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsRelativeToAbsolute::json_schema();
    assert_eq!(expected, actual);
}

// An absolute root `$id` fixes every resource's URI, so relative ids are always reachable by a
// relative path.
#[test]
fn absolute_id_root_refers_to_relative_id_resources_by_relative_path() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.com/schemas/root.json",
        "type": "object",
        "properties": { "ping": { "$ref": "#/$defs/AbsoluteRootPing" } },
        "additionalProperties": false,
        "$defs": {
            "AbsoluteRootPing": {
                "$id": "a/ping.json",
                "type": "object",
                "properties": { "pong": { "$ref": "../pong.json" } },
                "additionalProperties": false
            },
            "AbsoluteRootPong": {
                "$id": "pong.json",
                "type": "object",
                "properties": { "ping": { "$ref": "a/ping.json" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = AbsoluteRoot::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn relative_id_root_is_referred_to_by_relative_path() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "root.json",
        "type": "object",
        "properties": { "child": { "$ref": "#/$defs/RelativeRootChild" } },
        "additionalProperties": false,
        "$defs": {
            "RelativeRootChild": {
                "$id": "a/child.json",
                "type": "object",
                "properties": { "root": { "$ref": "../root.json" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = RelativeRoot::json_schema();
    assert_eq!(expected, actual);
}

// `../common/address.json` climbs out of the document's directory, so no relative path from it
// reaches `users/user.json` without knowing that directory's name: the address carries its own
// copy of the user, which resolves to `common/users/user.json` next to it.
#[test]
fn parent_segment_id_resource_embeds_the_relative_id_resource_it_refers_to() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "user": { "$ref": "#/$defs/ParentSegmentUser" } },
        "additionalProperties": false,
        "required": ["user"],
        "$defs": {
            "ParentSegmentAddress": {
                "$id": "../common/address.json",
                "type": "object",
                "properties": { "user": { "$ref": "users/user.json" } },
                "additionalProperties": false,
                "$defs": {
                    "ParentSegmentUser": {
                        "$id": "users/user.json",
                        "type": "object",
                        "properties": { "address": { "$ref": "../address.json" } },
                        "additionalProperties": false
                    }
                }
            },
            "ParentSegmentUser": {
                "$id": "users/user.json",
                "type": "object",
                "properties": { "address": { "$ref": "../../common/address.json" } },
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsParentSegmentUser::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn id_resources_with_one_id_and_identical_schemas_are_merged() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "first": { "$ref": "#/$defs/SameIdResource" },
            "second": { "$ref": "#/$defs/SameIdResource" }
        },
        "additionalProperties": false,
        "required": ["first", "second"],
        "$defs": {
            "SameIdResource": {
                "$id": "https://example.com/same",
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsSameIdResources::json_schema();
    assert_eq!(expected, actual);
}

// `node.json` and `./node.json` resolve to one URI.
#[test]
fn id_resources_with_one_resolved_id_and_different_schemas_are_rejected() {
    let expected: ReverseCodeGenResult = Err(vec![ReverseCodeGenError::DuplicateResourceId {
        first: "derive_to_json_schema::ClashingNode".to_string(),
        first_id: "node.json".to_string(),
        second: "derive_to_json_schema::ClashingDotNode".to_string(),
        second_id: "./node.json".to_string(),
    }]);
    let actual: ReverseCodeGenResult = HoldsClashingNodes::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn id_that_is_not_a_uri_reference_is_rejected() {
    let expected: ReverseCodeGenResult = Err(vec![ReverseCodeGenError::InvalidResourceId {
        schema_id: "derive_to_json_schema::InvalidIdResource".to_string(),
        id: "has space.json".to_string(),
        reason: "unexpected character at index 3".to_string(),
    }]);
    let actual: ReverseCodeGenResult = HoldsInvalidIdResource::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(shared_resource_expected_schema());
    let actual: ReverseCodeGenResult = HoldsSharedResourceDirectFirst::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn id_resource_used_from_two_places_is_embedded_once_when_first_used_inside_a_resource() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        required: Some(vec!["via".to_string(), "direct".to_string()]),
        ..shared_resource_expected_schema()
    });
    let actual: ReverseCodeGenResult = HoldsSharedResourceViaFirst::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_unit_enum_json_schema() {
    let actual: JsonSchema = Status::json_schema().expect("schema");
    assert_eq!(actual.type_, Some(JsonType::String.into()));
    let actual_enum = actual.enum_values.as_ref().expect("enum_values");
    assert_eq!(actual_enum.len(), 2);
    assert!(actual_enum.contains(&serde_json::Value::String("Open".to_string())));
    assert!(actual_enum.contains(&serde_json::Value::String("Closed".to_string())));
}

#[test]
fn derive_single_variant_enum_emits_const() {
    let actual: JsonSchema = SingleVariant::json_schema().expect("schema");
    let expected_type: Option<TypeSet> = Some(JsonType::String.into());
    let actual_type: Option<TypeSet> = actual.type_;
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
    let schema: JsonSchema = Root::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
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
    let actual: JsonSchema = AttrDescription::json_schema().expect("schema");
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
    let actual: JsonSchema = WithCommentAttr::json_schema().expect("schema");
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
    let actual: JsonSchema = WithDocAttr::json_schema().expect("schema");
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
    let actual: JsonSchema = EnumWithDescAttr::json_schema().expect("schema");
    let expected_desc: Option<String> = Some("Enum description".to_string());
    assert_eq!(expected_desc, actual.description);
}

#[test]
fn derive_round_trip_with_description() {
    let schema: JsonSchema = AttrDescription::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
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
    let mut byte_schema: JsonSchema = i64::json_schema().expect("schema");
    byte_schema.minimum = Some(0_u64.into());
    byte_schema.maximum = Some(255_u64.into());
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("byte".to_string(), byte_schema.into());
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["byte".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithIntegerMinMax::json_schema();
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
    let mut score_schema: JsonSchema = f64::json_schema().expect("schema");
    score_schema.minimum = serde_json::Number::from_f64(0.0);
    score_schema.maximum = serde_json::Number::from_f64(100.0);
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("score".to_string(), score_schema.into());
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["score".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithFloatMinMax::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_minimum_maximum_round_trip() {
    let schema: JsonSchema = WithIntegerMinMax::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
    assert_eq!(schema, parsed);
}

#[test]
fn derive_minimum_maximum_float_round_trip() {
    let schema: JsonSchema = WithFloatMinMax::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
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
    let mut value_schema: JsonSchema = i64::json_schema().expect("schema");
    value_schema.minimum = Some(10_u64.into());
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema.into());
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["value".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithOnlyMinimum::json_schema();
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
    let mut value_schema: JsonSchema = i64::json_schema().expect("schema");
    value_schema.maximum = Some(90_u64.into());
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("value".to_string(), value_schema.into());
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["value".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithOnlyMaximum::json_schema();
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
    let actual: String = (&WithExactBounds::json_schema().expect("schema"))
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
    let schema: JsonSchema = WithVecField::json_schema().expect("schema");
    let tags_schema: &JsonSchema = schema
        .properties
        .get("tags")
        .and_then(Schema::as_object)
        .expect("tags property");
    assert_eq!(tags_schema.type_, Some(JsonType::Array.into()));
    let items: &JsonSchema = tags_schema
        .items
        .as_ref()
        .and_then(Schema::as_object)
        .expect("items");
    assert_eq!(items.type_, Some(JsonType::String.into()));

    let counts_schema: &JsonSchema = schema
        .properties
        .get("counts")
        .and_then(Schema::as_object)
        .expect("counts property");
    assert_eq!(counts_schema.type_, Some(JsonType::Array.into()));
    let count_items: &JsonSchema = counts_schema
        .items
        .as_ref()
        .and_then(Schema::as_object)
        .expect("items");
    assert_eq!(count_items.type_, Some(JsonType::Integer.into()));
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithHashSetField {
    tags: HashSet<String>,
}

#[test]
fn derive_struct_with_hash_set_field_emits_unique_items_true() {
    let schema: JsonSchema = WithHashSetField::json_schema().expect("schema");
    let tags_schema: &JsonSchema = schema
        .properties
        .get("tags")
        .and_then(Schema::as_object)
        .expect("tags property");
    assert_eq!(tags_schema.type_, Some(JsonType::Array.into()));
    assert_eq!(tags_schema.unique_items, Some(true));
    let items: &JsonSchema = tags_schema
        .items
        .as_ref()
        .and_then(Schema::as_object)
        .expect("items");
    assert_eq!(items.type_, Some(JsonType::String.into()));
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithVecFieldMinMax {
    #[json_schema(min_items = 1, max_items = 10)]
    tags: Vec<String>,
}

#[test]
fn derive_struct_with_vec_field_min_items_max_items_emits_bounds() {
    let schema: JsonSchema = WithVecFieldMinMax::json_schema().expect("schema");
    let tags_schema: &JsonSchema = schema
        .properties
        .get("tags")
        .and_then(Schema::as_object)
        .expect("tags property");
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
    let schema: JsonSchema = WithHashSetFieldMinMax::json_schema().expect("schema");
    let ids_schema: &JsonSchema = schema
        .properties
        .get("ids")
        .and_then(Schema::as_object)
        .expect("ids property");
    let expected_min: Option<u64> = Some(2);
    let expected_max: Option<u64> = Some(5);
    let actual_min: Option<u64> = ids_schema.min_items;
    let actual_max: Option<u64> = ids_schema.max_items;
    assert_eq!(expected_min, actual_min);
    assert_eq!(expected_max, actual_max);
}

#[test]
fn vec_string_json_schema_has_min_items_max_items_none() {
    let actual: JsonSchema = Vec::<String>::json_schema().expect("schema");
    let expected_min: Option<u64> = None;
    let expected_max: Option<u64> = None;
    assert_eq!(expected_min, actual.min_items);
    assert_eq!(expected_max, actual.max_items);
}

#[test]
fn hash_set_string_json_schema_has_min_items_max_items_none() {
    let actual: JsonSchema = HashSet::<String>::json_schema().expect("schema");
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
    let schema: JsonSchema = WithStringMinLength::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
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
    let schema: JsonSchema = WithStringMaxLength::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
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
    let schema: JsonSchema = WithStringBothLengths::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
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
    let schema: JsonSchema = WithStringNoLengthConstraints::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
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
    let schema: JsonSchema = WithStringPattern::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
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
    let schema: JsonSchema = WithDefaultField::json_schema().expect("schema");
    let count_schema: &JsonSchema = schema
        .properties
        .get("count")
        .and_then(Schema::as_object)
        .expect("count property");
    let expected: Option<serde_json::Value> = Some(serde_json::json!(42));
    let actual: Option<serde_json::Value> = count_schema.default_value.clone();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_default_name_emits_default_in_schema() {
    let schema: JsonSchema = WithDefaultField::json_schema().expect("schema");
    let name_schema: &JsonSchema = schema
        .properties
        .get("name")
        .and_then(Schema::as_object)
        .expect("name property");
    let expected: Option<serde_json::Value> = Some(serde_json::json!("foo"));
    let actual: Option<serde_json::Value> = name_schema.default_value.clone();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_default_round_trip_serializes_and_parses() {
    let schema: JsonSchema = WithDefaultField::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let reparsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
    let expected: JsonSchema = schema.clone();
    let actual: JsonSchema = reparsed;
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithSetFields {
    hashed: HashSet<String>,
    sorted: std::collections::BTreeSet<i64>,
    listed: Vec<String>,
}

#[test]
fn derive_set_fields_emit_unique_items_true() {
    let schema: JsonSchema = WithSetFields::json_schema().expect("schema");
    let expected: Vec<(String, Option<bool>)> = vec![
        ("hashed".to_string(), Some(true)),
        ("listed".to_string(), None),
        ("sorted".to_string(), Some(true)),
    ];
    let actual: Vec<(String, Option<bool>)> = schema
        .properties
        .iter()
        .map(|(key, property)| {
            (
                key.clone(),
                property.as_object().and_then(|p| p.unique_items),
            )
        })
        .collect();
    assert_eq!(expected, actual);
}

#[cfg(feature = "indexmap")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithIndexSetField {
    tags: indexmap::IndexSet<String>,
}

#[cfg(feature = "indexmap")]
#[test]
fn derive_index_set_field_emits_unique_items_true() {
    let schema: JsonSchema = WithIndexSetField::json_schema().expect("schema");
    let expected: Option<bool> = Some(true);
    let actual: Option<bool> = schema
        .properties
        .get("tags")
        .and_then(Schema::as_object)
        .and_then(|p| p.unique_items);
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithExactDefaults {
    #[json_schema(default = 18446744073709551615)]
    big: Option<u64>,
    #[json_schema(default = -5)]
    negative: Option<i64>,
    #[json_schema(default = -1.5)]
    ratio: Option<f64>,
    #[json_schema(default = true)]
    flag: Option<bool>,
    plain: Option<i64>,
}

#[test]
fn derive_field_defaults_are_exact_json_values() {
    let schema: JsonSchema = WithExactDefaults::json_schema().expect("schema");
    let expected: Vec<(String, Option<serde_json::Value>)> = vec![
        (
            "big".to_string(),
            Some(serde_json::json!(18_446_744_073_709_551_615_u64)),
        ),
        ("flag".to_string(), Some(serde_json::json!(true))),
        ("negative".to_string(), Some(serde_json::json!(-5))),
        ("plain".to_string(), None),
        ("ratio".to_string(), Some(serde_json::json!(-1.5))),
    ];
    let actual: Vec<(String, Option<serde_json::Value>)> = schema
        .properties
        .iter()
        .map(|(key, property)| {
            let default: Option<serde_json::Value> =
                property.as_object().and_then(|p| p.default_value.clone());
            (key.clone(), default)
        })
        .collect();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithUniqueItemsVec {
    #[json_schema(unique_items)]
    tags: Vec<String>,
    #[json_schema(unique_items, min_items = 1)]
    ids: Option<Vec<i64>>,
    plain: Vec<String>,
}

#[test]
fn derive_unique_items_flag_sets_unique_items_on_vec_fields() {
    let schema: JsonSchema = WithUniqueItemsVec::json_schema().expect("schema");
    let expected: Vec<(String, Option<bool>, Option<u64>)> = vec![
        ("ids".to_string(), Some(true), Some(1)),
        ("plain".to_string(), None, None),
        ("tags".to_string(), Some(true), None),
    ];
    let actual: Vec<(String, Option<bool>, Option<u64>)> = schema
        .properties
        .iter()
        .map(|(key, property)| {
            let p: &JsonSchema = property.as_object().expect("schema object");
            (key.clone(), p.unique_items, p.min_items)
        })
        .collect();
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
    let schema: JsonSchema = WithUuid::json_schema().expect("schema");
    let id_schema: &JsonSchema = schema
        .properties
        .get("id")
        .and_then(Schema::as_object)
        .expect("id property");
    assert_eq!(id_schema.type_, Some(JsonType::String.into()));
    assert_eq!(id_schema.format.as_deref(), Some("uuid"));
    let secondary_schema: &JsonSchema = schema
        .properties
        .get("secondary_id")
        .and_then(Schema::as_object)
        .expect("secondary_id property");
    assert_eq!(secondary_schema.type_, Some(JsonType::String.into()));
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
        type_: Some(JsonType::Array.into()),
        items: Some(
            JsonSchema {
                type_: Some(JsonType::String.into()),
                format: Some("uuid".to_string()),
                ..Default::default()
            }
            .into(),
        ),
        ..Default::default()
    };
    let actual: JsonSchema = WithUuidVec::json_schema()
        .expect("schema")
        .properties
        .get("ids")
        .and_then(Schema::as_object)
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
    let mut legacy_schema: JsonSchema = String::json_schema().expect("schema");
    legacy_schema.deprecated = Some(true);
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        properties: {
            let mut m = BTreeMap::new();
            m.insert("legacy".to_string(), legacy_schema.into());
            m
        },
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["legacy".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithDeprecatedField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_deprecated_round_trip() {
    let schema: JsonSchema = WithDeprecatedField::json_schema().expect("schema");
    let json: String = (&schema).try_into().expect("serialize");
    let parsed: JsonSchema = Schema::try_from(json.as_str())
        .expect("parse")
        .into_object()
        .expect("schema object");
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
        type_: Some(JsonType::Array.into()),
        items: Some(ref_to(name).into()),
        ..Default::default()
    }
}

/// Body of a derived struct with one required property `key` whose schema is `property`.
fn one_property_body(key: &str, property: JsonSchema) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([(key.to_string(), property.into())]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec![key.to_string()]),
        ..Default::default()
    }
}

#[test]
fn derive_mutual_recursion_emits_both_defs_linked_by_ref() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")).into(),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")).into(),
            ),
        ])),
        ..one_property_body("bs", array_of_ref("MutualB"))
    });
    let actual: ReverseCodeGenResult = MutualA::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_mutual_recursion_from_other_root_emits_both_defs() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")).into(),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")).into(),
            ),
        ])),
        ..one_property_body("as_", array_of_ref("MutualA"))
    });
    let actual: ReverseCodeGenResult = MutualB::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_three_type_cycle_emits_all_defs() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([
            (
                "CycleA".to_string(),
                one_property_body("bs", array_of_ref("CycleB")).into(),
            ),
            (
                "CycleB".to_string(),
                one_property_body("cs", array_of_ref("CycleC")).into(),
            ),
            (
                "CycleC".to_string(),
                one_property_body("as_", array_of_ref("CycleA")).into(),
            ),
        ])),
        ..one_property_body("bs", array_of_ref("CycleB"))
    });
    let actual: ReverseCodeGenResult = CycleA::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_self_reference_with_field_override_keeps_ref_and_override() {
    let children: JsonSchema = JsonSchema {
        min_items: Some(1),
        ..array_of_ref("OverriddenNode")
    };
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "OverriddenNode".to_string(),
            one_property_body("children", children.clone()).into(),
        )])),
        ..one_property_body("children", children)
    });
    let actual: ReverseCodeGenResult = OverriddenNode::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_override_on_named_type_is_ref_sibling() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Address".to_string(),
            JsonSchema {
                type_: Some(JsonType::Object.into()),
                properties: BTreeMap::from([
                    (
                        "city".to_string(),
                        String::json_schema().expect("schema").into(),
                    ),
                    (
                        "street".to_string(),
                        String::json_schema().expect("schema").into(),
                    ),
                ]),
                additional_properties: Some(Schema::Bool(false)),
                required: Some(vec!["city".to_string(), "street".to_string()]),
                ..Default::default()
            }
            .into(),
        )])),
        ..one_property_body(
            "address",
            JsonSchema {
                deprecated: Some(true),
                ..ref_to("Address")
            },
        )
    });
    let actual: ReverseCodeGenResult = DeprecatedAddress::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_enum_field_emits_defs_entry_without_schema_keyword() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Status".to_string(),
            JsonSchema {
                type_: Some(JsonType::String.into()),
                enum_values: Some(vec![
                    serde_json::Value::String("Open".to_string()),
                    serde_json::Value::String("Closed".to_string()),
                ]),
                ..Default::default()
            }
            .into(),
        )])),
        ..one_property_body("status", ref_to("Status"))
    });
    let actual: ReverseCodeGenResult = WithStatus::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn vec_of_derived_type_json_schema_is_standalone_with_defs() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        defs: Some(BTreeMap::from([
            (
                "MutualA".to_string(),
                one_property_body("bs", array_of_ref("MutualB")).into(),
            ),
            (
                "MutualB".to_string(),
                one_property_body("as_", array_of_ref("MutualA")).into(),
            ),
        ])),
        ..array_of_ref("MutualA")
    });
    let actual: ReverseCodeGenResult = Vec::<MutualA>::json_schema();
    assert_eq!(expected, actual);
}

#[derive(ToJsonSchema)]
struct Empty {}

#[test]
fn derive_struct_without_fields_json_schema() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        additional_properties: Some(Schema::Bool(false)),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = Empty::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: Some(BTreeMap::from([
            (
                "Config0".to_string(),
                one_property_body("name", String::json_schema().expect("schema")).into(),
            ),
            (
                "Config1".to_string(),
                one_property_body("port", i64::json_schema().expect("schema")).into(),
            ),
        ])),
        properties: BTreeMap::from([
            ("a".to_string(), ref_to("Config0").into()),
            ("b".to_string(), ref_to("Config1").into()),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["a".to_string(), "b".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = TwoConfigs::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_same_name_type_used_twice_shares_one_suffixed_entry() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: Some(BTreeMap::from([
            (
                "Config0".to_string(),
                one_property_body("name", String::json_schema().expect("schema")).into(),
            ),
            (
                "Config1".to_string(),
                one_property_body("port", i64::json_schema().expect("schema")).into(),
            ),
        ])),
        properties: BTreeMap::from([
            ("a".to_string(), ref_to("Config0").into()),
            ("b".to_string(), ref_to("Config1").into()),
            ("c".to_string(), ref_to("Config0").into()),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["a".to_string(), "b".to_string(), "c".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = RepeatedConfig::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_unique_name_stays_unsuffixed_when_another_module_reuses_it_elsewhere() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Config".to_string(),
            one_property_body("name", String::json_schema().expect("schema")).into(),
        )])),
        ..one_property_body("config", ref_to("Config"))
    });
    let actual: ReverseCodeGenResult = OneConfig::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        type_: Some(JsonType::Object.into()),
        defs: Some(BTreeMap::from([(
            "Uuid".to_string(),
            one_property_body("value", String::json_schema().expect("schema")).into(),
        )])),
        properties: BTreeMap::from([
            ("id".to_string(), ref_to("Uuid").into()),
            (
                "library_id".to_string(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    format: Some("uuid".to_string()),
                    ..Default::default()
                }
                .into(),
            ),
        ]),
        additional_properties: Some(Schema::Bool(false)),
        required: Some(vec!["id".to_string(), "library_id".to_string()]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = WithUserUuid::json_schema();
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
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Struct doc.".to_string()),
        ..one_property_body("x", String::json_schema().expect("schema"))
    });
    let actual: ReverseCodeGenResult = DocContainer::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_doc_comment_becomes_property_description() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body(
            "x",
            JsonSchema {
                description: Some("Field doc.".to_string()),
                ..String::json_schema().expect("schema")
            },
        )
    });
    let actual: ReverseCodeGenResult = DocField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_field_doc_comment_on_named_type_is_ref_sibling() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        defs: Some(BTreeMap::from([(
            "Address".to_string(),
            JsonSchema {
                type_: Some(JsonType::Object.into()),
                properties: BTreeMap::from([
                    (
                        "city".to_string(),
                        String::json_schema().expect("schema").into(),
                    ),
                    (
                        "street".to_string(),
                        String::json_schema().expect("schema").into(),
                    ),
                ]),
                additional_properties: Some(Schema::Bool(false)),
                required: Some(vec!["city".to_string(), "street".to_string()]),
                ..Default::default()
            }
            .into(),
        )])),
        ..one_property_body(
            "address",
            JsonSchema {
                description: Some("Where they live.".to_string()),
                ..ref_to("Address")
            },
        )
    });
    let actual: ReverseCodeGenResult = DocNamedTypeField::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_multi_line_doc_comment_lines_are_trimmed_and_joined() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("First line.\nIndented second line.\n\nSecond paragraph.".to_string()),
        ..one_property_body("x", String::json_schema().expect("schema"))
    });
    let actual: ReverseCodeGenResult = DocMultiLine::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_description_attribute_takes_precedence_over_doc_comment() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("From attribute".to_string()),
        ..one_property_body("x", String::json_schema().expect("schema"))
    });
    let actual: ReverseCodeGenResult = DocAndAttrDescription::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_enum_doc_comment_becomes_description() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Enum doc.".to_string()),
        type_: Some(JsonType::String.into()),
        enum_values: Some(vec![
            serde_json::Value::String("A".to_string()),
            serde_json::Value::String("B".to_string()),
        ]),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = DocEnum::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_macro_doc_attributes_are_evaluated_into_description() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        description: Some("Concat.".to_string()),
        ..one_property_body(
            "x",
            JsonSchema {
                description: Some("Included doc.".to_string()),
                ..String::json_schema().expect("schema")
            },
        )
    });
    let actual: ReverseCodeGenResult = DocConcat::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_empty_doc_comment_yields_no_description() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body("x", String::json_schema().expect("schema"))
    });
    let actual: ReverseCodeGenResult = DocEmpty::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn derive_list_form_doc_attribute_yields_no_description() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        ..one_property_body("x", String::json_schema().expect("schema"))
    });
    let actual: ReverseCodeGenResult = DocHiddenOnly::json_schema();
    assert_eq!(expected, actual);
}

// --- Shapes forward codegen emits: nullable fields, flattened maps, unions, value enums ---

/// Deserializes a present field (including `null`) as `Some(value)`, so an
/// `Option<Option<T>>` tells an absent key from an explicit `null`.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(deserializer).map(Some)
}

#[expect(clippy::unnecessary_wraps)]
fn default_nullable_fields_renamed() -> Option<String> {
    Some("x".to_string())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct NullableFields {
    pub plain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[json_schema(required)]
    pub required_nullable: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    #[expect(clippy::option_option)]
    pub optional_nullable: Option<Option<bool>>,
    #[serde(
        rename = "renamed-key",
        default = "default_nullable_fields_renamed",
        skip_serializing_if = "Option::is_none"
    )]
    pub renamed: Option<String>,
    pub nullable_items: Vec<Option<String>>,
}

#[test]
fn derive_option_fields_emit_optional_required_nullable_and_renamed_properties() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "plain": { "type": "string" },
            "optional": { "type": "string" },
            "required_nullable": { "type": ["null", "string"] },
            "optional_nullable": { "type": ["boolean", "null"] },
            "renamed-key": { "type": "string" },
            "nullable_items": { "type": "array", "items": { "type": ["null", "string"] } }
        },
        "additionalProperties": false,
        "required": ["plain", "required_nullable", "nullable_items"]
    })));
    let actual: ReverseCodeGenResult = NullableFields::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct NullableLeaf {
    pub x: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct RequiredNullableNamed {
    #[serde(deserialize_with = "Option::deserialize")]
    #[json_schema(required)]
    pub leaf: Option<NullableLeaf>,
}

#[test]
fn derive_required_nullable_named_field_is_any_of_ref_and_null() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "leaf": { "anyOf": [{ "$ref": "#/$defs/NullableLeaf" }, { "type": "null" }] }
        },
        "additionalProperties": false,
        "required": ["leaf"],
        "$defs": {
            "NullableLeaf": {
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = RequiredNullableNamed::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct FlattenedStrings {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, String>,
}

#[test]
fn derive_flattened_map_sets_additional_properties_instead_of_property() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "additionalProperties": { "type": "string" },
        "required": ["name"]
    })));
    let actual: ReverseCodeGenResult = FlattenedStrings::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct FlattenedNamed {
    #[serde(flatten)]
    pub additional: BTreeMap<String, NullableLeaf>,
}

#[test]
fn derive_flattened_map_of_named_type_refs_defs_entry() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "additionalProperties": { "$ref": "#/$defs/NullableLeaf" },
        "$defs": {
            "NullableLeaf": {
                "type": "object",
                "properties": { "x": { "type": "string" } },
                "additionalProperties": false,
                "required": ["x"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = FlattenedNamed::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct Cat {
    pub meows: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct Dog {
    pub barks: bool,
}

/// A pet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(title = "Pet")]
enum PetAnyOf {
    Cat(Cat),
    String(String),
    Dog(Box<Dog>),
    Null(()),
}

#[test]
fn derive_untagged_enum_emits_any_of_of_variant_payloads() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Pet",
        "description": "A pet.",
        "anyOf": [
            { "$ref": "#/$defs/Cat" },
            { "type": "string" },
            { "$ref": "#/$defs/Dog" },
            { "type": "null" }
        ],
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            },
            "Dog": {
                "type": "object",
                "properties": { "barks": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["barks"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = PetAnyOf::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
enum PetOneOf {
    Cat(Cat),
    Dog(Box<Dog>),
}

#[test]
fn derive_untagged_enum_with_one_of_attribute_emits_one_of() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [{ "$ref": "#/$defs/Cat" }, { "$ref": "#/$defs/Dog" }],
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            },
            "Dog": {
                "type": "object",
                "properties": { "barks": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["barks"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = PetOneOf::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
enum RecursiveUnion {
    Leaf(i8),
    List(Vec<RecursiveUnion>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct HoldsRecursiveUnion {
    pub value: RecursiveUnion,
}

#[test]
fn derive_recursive_untagged_enum_refers_to_its_defs_entry() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "value": { "$ref": "#/$defs/RecursiveUnion" } },
        "additionalProperties": false,
        "required": ["value"],
        "$defs": {
            "RecursiveUnion": {
                "anyOf": [
                    { "type": "integer", "minimum": -128, "maximum": 127 },
                    { "type": "array", "items": { "$ref": "#/$defs/RecursiveUnion" } }
                ]
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsRecursiveUnion::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, ToJsonSchema)]
#[expect(dead_code)]
enum Lvl {
    #[json_schema(value = "x")]
    X,
    #[json_schema(value = 1)]
    N1,
    #[json_schema(value = null)]
    Null,
    #[json_schema(value = 1.5)]
    N1Point5,
    #[json_schema(value = -3)]
    NMinus3,
    #[json_schema(value = true)]
    True,
    #[json_schema(value = [1, "a"])]
    List,
    #[json_schema(value = {"k": [null]})]
    Object,
    #[serde(rename = "by-name")]
    ByName,
}

#[test]
fn derive_enum_with_values_emits_enum_of_json_literals_without_type() {
    let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","enum":["x",1,null,1.5,-3,true,[1,"a"],{"k":[null]},"by-name"]}"#
            .to_string(),
    );
    let actual: Result<String, Vec<ReverseCodeGenError>> = Lvl::json_schema()
        .map(|schema: JsonSchema| serde_json::to_string(&schema).expect("serialize"));
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, PartialEq, ToJsonSchema)]
#[expect(dead_code)]
enum OnlyNull {
    #[json_schema(value = null)]
    Null,
}

#[test]
fn derive_single_variant_value_enum_emits_const() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        const_value: Some(serde_json::Value::Null),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = OnlyNull::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, PartialEq, ToJsonSchema)]
enum NoVariants {}

#[test]
fn derive_enum_without_variants_emits_empty_enum() {
    let expected: ReverseCodeGenResult = Ok(JsonSchema {
        schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
        enum_values: Some(Vec::new()),
        ..Default::default()
    });
    let actual: ReverseCodeGenResult = NoVariants::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(flatten_single_entry)]
enum FlatCat {
    Cat(Cat),
}

/// A flattened one-variant union is its payload's schema, not a one-entry `anyOf`.
#[test]
fn flatten_single_entry_union_with_named_payload_is_the_payload_schema() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "#/$defs/Cat",
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = FlatCat::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
enum OnlyCat {
    Cat(Cat),
}

/// Without the flag a one-variant union stays an `anyOf` with one entry.
#[test]
fn single_variant_union_without_flag_emits_one_entry_any_of() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "anyOf": [{ "$ref": "#/$defs/Cat" }],
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = OnlyCat::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of, flatten_single_entry)]
enum FlatOneOfCat {
    Cat(Cat),
}

/// The flag flattens a `one_of` union too: no one-entry `oneOf` remains.
#[test]
fn flatten_single_entry_one_of_union_is_the_payload_schema() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "#/$defs/Cat",
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = FlatOneOfCat::json_schema();
    assert_eq!(expected, actual);
}

/// A name.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(flatten_single_entry, title = "Name")]
enum FlatName {
    Text(String),
}

/// A flattened primitive payload carries the container's title and doc comment.
#[test]
fn flatten_single_entry_primitive_payload_carries_container_annotations() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Name",
        "description": "A name.",
        "type": "string"
    })));
    let actual: ReverseCodeGenResult = FlatName::json_schema();
    assert_eq!(expected, actual);
}

// A hand-written payload with its own annotations and no `$id`.
#[derive(serde::Serialize)]
struct AnnotatedText;

impl ToJsonSchema for AnnotatedText {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(schema_from_json(serde_json::json!({
            "type": "string",
            "title": "Payload title",
            "description": "Payload description.",
            "$comment": "Payload comment."
        })))
    }
}

#[derive(serde::Serialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(flatten_single_entry, title = "Container title")]
#[expect(dead_code)]
enum FlatAnnotatedText {
    Text(AnnotatedText),
}

/// A container annotation that is set replaces the payload's; unset ones keep the payload's.
#[test]
fn flatten_single_entry_container_annotations_replace_only_the_ones_set() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "string",
        "title": "Container title",
        "description": "Payload description.",
        "$comment": "Payload comment."
    })));
    let actual: ReverseCodeGenResult = FlatAnnotatedText::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
struct HoldsFlatName {
    pub name: FlatName,
}

/// As a field, the flattened union is a `$defs` entry holding the flattened schema.
#[test]
fn flatten_single_entry_union_as_field_has_flattened_defs_entry() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "name": { "$ref": "#/$defs/FlatName" } },
        "additionalProperties": false,
        "required": ["name"],
        "$defs": {
            "FlatName": {
                "title": "Name",
                "description": "A name.",
                "type": "string"
            }
        }
    })));
    let actual: ReverseCodeGenResult = HoldsFlatName::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(flatten_single_entry)]
enum FlatTwoPets {
    Cat(Cat),
    Dog(Dog),
}

/// The flag has no effect on a union with more than one variant.
#[test]
fn flatten_single_entry_two_variant_union_keeps_any_of() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "anyOf": [{ "$ref": "#/$defs/Cat" }, { "$ref": "#/$defs/Dog" }],
        "$defs": {
            "Cat": {
                "type": "object",
                "properties": { "meows": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["meows"]
            },
            "Dog": {
                "type": "object",
                "properties": { "barks": { "type": "boolean" } },
                "additionalProperties": false,
                "required": ["barks"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = FlatTwoPets::json_schema();
    assert_eq!(expected, actual);
}

#[derive(serde::Serialize, ToJsonSchema)]
#[serde(untagged)]
#[json_schema(flatten_single_entry, title = "Hand")]
#[expect(dead_code)]
enum FlatHandWrittenResource {
    Hand(HandWrittenResource),
}

/// A payload that is a `$id` resource is not flattened: the union keeps its one-entry `anyOf`.
#[test]
fn flatten_single_entry_keeps_any_of_when_payload_is_an_id_resource() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Hand",
        "anyOf": [HandWrittenResource::json_schema().expect("schema")]
    })));
    let actual: ReverseCodeGenResult = FlatHandWrittenResource::json_schema();
    assert_eq!(expected, actual);
}
