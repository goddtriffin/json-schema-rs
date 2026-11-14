//! Integration test: `#[derive(ToJsonSchema)]` describes what serde deserializes: every enum
//! representation (external, internal, adjacent, untagged, per-variant `untagged`, `other`),
//! `flatten` of any type, `skip`, `default`, `rename` / `rename_all` (including their
//! `deserialize = ...` forms), `rename_all_fields`, `deserialize_with`, `transparent`, `from`,
//! unit and tuple structs; and the field overrides `json_schema(with)` / `json_schema(schema_with)`.
//! The agreement tests run the same instances through serde and through the derived schema.

use json_schema_rs::{JsonSchema, ReverseCodeGenResult, SchemaGenerator, ToJsonSchema};
use json_schema_rs_macro::ToJsonSchema;
use serde::Deserialize;
use std::collections::BTreeMap;

fn schema_from_json(value: serde_json::Value) -> JsonSchema {
    serde_json::from_value::<JsonSchema>(value).expect("valid schema")
}

/// Whether the derived schema of `T` accepts each instance.
fn schema_accepts<T: ToJsonSchema>(instances: &[serde_json::Value]) -> Vec<bool> {
    let schema: JsonSchema = T::json_schema().expect("schema");
    instances
        .iter()
        .map(|instance: &serde_json::Value| json_schema_rs::validate(&schema, instance).is_ok())
        .collect()
}

/// Whether serde deserializes `T` from each instance.
fn serde_accepts<T: for<'de> Deserialize<'de>>(instances: &[serde_json::Value]) -> Vec<bool> {
    instances
        .iter()
        .map(|instance: &serde_json::Value| T::deserialize(instance).is_ok())
        .collect()
}

#[derive(Deserialize, ToJsonSchema)]
struct Side {
    side: f64,
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(tag = "type")]
#[expect(dead_code)]
enum Shape {
    Circle { radius: f64 },
    Square(Side),
    Empty,
}

#[test]
fn internally_tagged_enum_merges_the_tag_into_each_variant_object() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "const": "Circle" },
                    "radius": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                },
                "additionalProperties": false,
                "required": ["type", "radius"]
            },
            {
                "type": "object",
                "properties": {
                    "type": { "type": "string", "const": "Square" },
                    "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                },
                "additionalProperties": false,
                "required": ["type", "side"]
            },
            {
                "type": "object",
                "properties": { "type": { "type": "string", "const": "Empty" } },
                "additionalProperties": false,
                "required": ["type"]
            }
        ]
    })));
    let actual: ReverseCodeGenResult = Shape::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn internally_tagged_enum_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!({"type": "Circle", "radius": 1.5}),
        serde_json::json!({"type": "Square", "side": 2.0}),
        serde_json::json!({"type": "Empty"}),
        serde_json::json!({"type": "Circle"}),
        serde_json::json!({"type": "Triangle"}),
        serde_json::json!({"radius": 1.5}),
        serde_json::json!("Empty"),
    ];
    let expected: Vec<bool> = serde_accepts::<Shape>(&instances);
    let actual: Vec<bool> = schema_accepts::<Shape>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(tag = "t", content = "c")]
#[expect(dead_code)]
enum Adjacent {
    A(String),
    B { x: i32 },
    C,
    D(i32, String),
}

#[test]
fn adjacently_tagged_enum_puts_the_payload_under_the_content_key() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [
            {
                "type": "object",
                "properties": { "t": { "type": "string", "const": "A" }, "c": { "type": "string" } },
                "additionalProperties": false,
                "required": ["t", "c"]
            },
            {
                "type": "object",
                "properties": {
                    "t": { "type": "string", "const": "B" },
                    "c": {
                        "type": "object",
                        "properties": { "x": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                        "additionalProperties": false,
                        "required": ["x"]
                    }
                },
                "additionalProperties": false,
                "required": ["t", "c"]
            },
            {
                "type": "object",
                "properties": { "t": { "type": "string", "const": "C" }, "c": { "type": "null" } },
                "additionalProperties": false,
                "required": ["t"]
            },
            {
                "type": "object",
                "properties": {
                    "t": { "type": "string", "const": "D" },
                    "c": {
                        "type": "array",
                        "prefixItems": [
                            { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
                            { "type": "string" }
                        ],
                        "minItems": 2,
                        "maxItems": 2
                    }
                },
                "additionalProperties": false,
                "required": ["t", "c"]
            }
        ]
    })));
    let actual: ReverseCodeGenResult = Adjacent::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn adjacently_tagged_enum_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!({"t": "A", "c": "s"}),
        serde_json::json!({"t": "B", "c": {"x": 1}}),
        serde_json::json!({"t": "C"}),
        serde_json::json!({"t": "C", "c": null}),
        serde_json::json!({"t": "D", "c": [1, "s"]}),
        serde_json::json!({"t": "D", "c": [1]}),
        serde_json::json!({"t": "A"}),
        serde_json::json!({"t": "E", "c": 1}),
    ];
    let expected: Vec<bool> = serde_accepts::<Adjacent>(&instances);
    let actual: Vec<bool> = schema_accepts::<Adjacent>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
enum External {
    A(String),
    B {
        x: i32,
    },
    C,
    D(i32, String),
    #[serde(rename = "e")]
    E,
}

#[test]
fn externally_tagged_enum_lists_unit_variants_as_strings_and_wraps_the_others() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [
            { "type": "string", "enum": ["C", "e"] },
            {
                "type": "object",
                "properties": { "A": { "type": "string" } },
                "additionalProperties": false,
                "required": ["A"]
            },
            {
                "type": "object",
                "properties": {
                    "B": {
                        "type": "object",
                        "properties": { "x": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                        "additionalProperties": false,
                        "required": ["x"]
                    }
                },
                "additionalProperties": false,
                "required": ["B"]
            },
            {
                "type": "object",
                "properties": {
                    "D": {
                        "type": "array",
                        "prefixItems": [
                            { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
                            { "type": "string" }
                        ],
                        "minItems": 2,
                        "maxItems": 2
                    }
                },
                "additionalProperties": false,
                "required": ["D"]
            }
        ]
    })));
    let actual: ReverseCodeGenResult = External::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn externally_tagged_enum_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!("C"),
        serde_json::json!("e"),
        serde_json::json!("E"),
        serde_json::json!({"A": "s"}),
        serde_json::json!({"B": {"x": 1}}),
        serde_json::json!({"D": [1, "s"]}),
        serde_json::json!({"A": 1}),
        serde_json::json!({"A": "s", "B": {"x": 1}}),
    ];
    let expected: Vec<bool> = serde_accepts::<External>(&instances);
    let actual: Vec<bool> = schema_accepts::<External>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(tag = "kind")]
#[expect(dead_code)]
enum WithOther {
    A {
        x: i32,
    },
    #[serde(other)]
    Unknown,
}

#[test]
fn other_variant_accepts_every_tag_no_variant_names() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "const": "A" },
                    "x": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX }
                },
                "additionalProperties": false,
                "required": ["kind", "x"]
            },
            {
                "type": "object",
                "properties": { "kind": { "type": "string", "not": { "enum": ["A"] } } },
                "additionalProperties": false,
                "required": ["kind"]
            }
        ]
    })));
    let actual: ReverseCodeGenResult = WithOther::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
enum MixedUntagged {
    A(String),
    #[serde(untagged)]
    Raw(i64),
}

#[test]
fn untagged_variant_is_tried_as_its_bare_payload_after_the_tagged_ones() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "anyOf": [
            {
                "type": "object",
                "properties": { "A": { "type": "string" } },
                "additionalProperties": false,
                "required": ["A"]
            },
            { "type": "integer", "minimum": i64::MIN, "maximum": i64::MAX }
        ]
    })));
    let actual: ReverseCodeGenResult = MixedUntagged::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Default, Deserialize, ToJsonSchema)]
#[serde(rename_all(deserialize = "kebab-case"), default)]
struct Defaults {
    first_name: String,
    #[serde(skip)]
    cache: u8,
    #[serde(rename(deserialize = "LAST"))]
    last: String,
}

#[test]
fn container_default_skip_and_deserialize_renames_shape_the_object() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "first-name": { "type": "string" }, "LAST": { "type": "string" } },
        "additionalProperties": false
    })));
    let actual: ReverseCodeGenResult = Defaults::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FieldDefaults {
    #[serde(default)]
    count: i32,
    #[serde(default, deserialize_with = "nullable")]
    defaulted: Option<i32>,
    #[serde(deserialize_with = "nullable")]
    custom: Option<i32>,
    #[serde(skip_deserializing)]
    computed: i32,
}

fn nullable<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<i32>, D::Error> {
    Option::deserialize(deserializer)
}

#[test]
fn field_default_is_optional_and_custom_deserializer_option_is_required() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "count": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
            "defaulted": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
            "custom": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX }
        },
        "additionalProperties": false,
        "required": ["custom"]
    })));
    let actual: ReverseCodeGenResult = FieldDefaults::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn field_defaults_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!({"custom": 1}),
        serde_json::json!({"custom": 1, "count": 2, "defaulted": 3}),
        serde_json::json!({}),
        serde_json::json!({"count": 2}),
    ];
    let expected: Vec<bool> = serde_accepts::<FieldDefaults>(&instances);
    let actual: Vec<bool> = schema_accepts::<FieldDefaults>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct Extra {
    y: bool,
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenStructs {
    id: u32,
    #[serde(flatten)]
    side: Side,
    #[serde(flatten)]
    extra: Option<Extra>,
}

#[test]
fn flattened_structs_merge_their_properties_and_an_optional_one_requires_none() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "id": { "type": "integer", "minimum": 0, "maximum": u32::MAX },
            "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX },
            "y": { "type": "boolean" }
        },
        "additionalProperties": false,
        "required": ["id", "side"]
    })));
    let actual: ReverseCodeGenResult = FlattenStructs::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenTagged {
    id: u8,
    #[serde(flatten)]
    shape: Shape,
}

#[test]
fn flattened_enum_is_an_all_of_part_closed_by_unevaluated_properties() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "id": { "type": "integer", "minimum": 0, "maximum": 255 } },
        "required": ["id"],
        "allOf": [{
            "oneOf": [
                {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "const": "Circle" },
                        "radius": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                    },
                    "required": ["type", "radius"]
                },
                {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "const": "Square" },
                        "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                    },
                    "required": ["type", "side"]
                },
                {
                    "type": "object",
                    "properties": { "type": { "type": "string", "const": "Empty" } },
                    "required": ["type"]
                }
            ]
        }],
        "unevaluatedProperties": false
    })));
    let actual: ReverseCodeGenResult = FlattenTagged::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn flattened_enum_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!({"id": 1, "type": "Circle", "radius": 1.0}),
        serde_json::json!({"id": 1, "type": "Empty"}),
        serde_json::json!({"id": 1}),
        serde_json::json!({"type": "Empty"}),
        serde_json::json!({"id": 1, "type": "Square"}),
    ];
    let expected: Vec<bool> = serde_accepts::<FlattenTagged>(&instances);
    let actual: Vec<bool> = schema_accepts::<FlattenTagged>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenOptionalEnum {
    id: u8,
    #[serde(flatten)]
    shape: Option<Shape>,
}

#[test]
fn optional_flattened_enum_also_accepts_none_of_its_variants() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "id": { "type": "integer", "minimum": 0, "maximum": 255 } },
        "required": ["id"],
        "allOf": [{
            "anyOf": [
                {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string", "const": "Circle" },
                                "radius": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                            },
                            "required": ["type", "radius"]
                        },
                        {
                            "type": "object",
                            "properties": {
                                "type": { "type": "string", "const": "Square" },
                                "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
                            },
                            "required": ["type", "side"]
                        },
                        {
                            "type": "object",
                            "properties": { "type": { "type": "string", "const": "Empty" } },
                            "required": ["type"]
                        }
                    ]
                },
                true
            ]
        }],
        "unevaluatedProperties": false
    })));
    let actual: ReverseCodeGenResult = FlattenOptionalEnum::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenExternal {
    #[serde(flatten)]
    value: External,
}

#[test]
fn flattened_externally_tagged_enum_reads_unit_variants_as_keys_with_null() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "allOf": [{
            "oneOf": [
                { "type": "object", "properties": { "A": { "type": "string" } }, "required": ["A"] },
                {
                    "type": "object",
                    "properties": {
                        "B": {
                            "type": "object",
                            "properties": { "x": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                            "additionalProperties": false,
                            "required": ["x"]
                        }
                    },
                    "required": ["B"]
                },
                { "type": "object", "properties": { "C": { "type": "null" } }, "required": ["C"] },
                {
                    "type": "object",
                    "properties": {
                        "D": {
                            "type": "array",
                            "prefixItems": [
                                { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
                                { "type": "string" }
                            ],
                            "minItems": 2,
                            "maxItems": 2
                        }
                    },
                    "required": ["D"]
                },
                { "type": "object", "properties": { "e": { "type": "null" } }, "required": ["e"] }
            ]
        }],
        "unevaluatedProperties": false
    })));
    let actual: ReverseCodeGenResult = FlattenExternal::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
enum Units {
    /// The first.
    A,
    #[serde(skip_deserializing)]
    B,
    C,
}

#[test]
fn unit_enum_leaves_out_variants_serde_never_reads() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "string",
        "enum": ["A", "C"]
    })));
    let actual: ReverseCodeGenResult = Units::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenUnits {
    #[serde(flatten)]
    units: Units,
}

#[test]
fn flattened_unit_enum_reads_each_variant_as_a_key_with_null() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "allOf": [{
            "oneOf": [
                {
                    "type": "object",
                    "description": "The first.",
                    "properties": { "A": { "type": "null" } },
                    "required": ["A"]
                },
                { "type": "object", "properties": { "C": { "type": "null" } }, "required": ["C"] }
            ]
        }],
        "unevaluatedProperties": false
    })));
    let actual: ReverseCodeGenResult = FlattenUnits::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn flattened_unit_enum_schema_agrees_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!({"A": null}),
        serde_json::json!({"C": null}),
        serde_json::json!({"B": null}),
        serde_json::json!({}),
    ];
    let expected: Vec<bool> = serde_accepts::<FlattenUnits>(&instances);
    let actual: Vec<bool> = schema_accepts::<FlattenUnits>(&instances);
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct Chain {
    v: i32,
    #[serde(flatten)]
    next: Option<Box<Chain>>,
}

#[test]
fn self_flatten_refers_to_the_type_instead_of_inlining_forever() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "v": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
        "required": ["v"],
        "allOf": [{ "anyOf": [{ "$ref": "#/$defs/Chain" }, true] }],
        "unevaluatedProperties": false,
        "$defs": {
            "Chain": {
                "type": "object",
                "properties": { "v": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                "required": ["v"],
                "allOf": [{ "anyOf": [{ "$ref": "#/$defs/Chain" }, true] }],
                "unevaluatedProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = Chain::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(rename_all_fields = "camelCase")]
#[expect(dead_code)]
enum VariantFields {
    V {
        some_field: i32,
    },
    #[serde(rename_all = "UPPERCASE")]
    W {
        other_field: i32,
    },
}

#[test]
fn rename_all_fields_renames_variant_fields_unless_the_variant_has_its_own_rule() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "V": {
                        "type": "object",
                        "properties": { "someField": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                        "additionalProperties": false,
                        "required": ["someField"]
                    }
                },
                "additionalProperties": false,
                "required": ["V"]
            },
            {
                "type": "object",
                "properties": {
                    "W": {
                        "type": "object",
                        "properties": { "OTHER_FIELD": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX } },
                        "additionalProperties": false,
                        "required": ["OTHER_FIELD"]
                    }
                },
                "additionalProperties": false,
                "required": ["W"]
            }
        ]
    })));
    let actual: ReverseCodeGenResult = VariantFields::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(transparent)]
#[expect(dead_code)]
struct Transparent {
    inner: Side,
}

#[test]
fn transparent_struct_is_a_newtype_of_its_field() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$ref": "#/$defs/Side",
        "$defs": {
            "Side": {
                "type": "object",
                "properties": {
                    "side": {"type": "number", "minimum": f64::MIN, "maximum": f64::MAX}
                },
                "required": ["side"],
                "additionalProperties": false
            }
        }
    })));
    let actual: ReverseCodeGenResult = Transparent::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(from = "Side")]
#[expect(dead_code)]
struct FromSide {
    length: f64,
}

impl From<Side> for FromSide {
    fn from(side: Side) -> Self {
        Self { length: side.side }
    }
}

#[test]
fn from_struct_has_the_source_type_schema() {
    let expected: ReverseCodeGenResult = Side::json_schema();
    let actual: ReverseCodeGenResult = FromSide::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
struct Unit;

#[test]
fn unit_struct_is_null() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "null"
    })));
    let actual: ReverseCodeGenResult = Unit::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct Pair(i32, String);

#[test]
fn tuple_struct_is_a_fixed_length_array() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "array",
        "prefixItems": [
            { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
            { "type": "string" }
        ],
        "minItems": 2,
        "maxItems": 2
    })));
    let actual: ReverseCodeGenResult = Pair::json_schema();
    assert_eq!(expected, actual);
}

#[test]
fn unit_and_tuple_struct_schemas_agree_with_serde() {
    let instances: Vec<serde_json::Value> = vec![
        serde_json::json!(null),
        serde_json::json!([1, "s"]),
        serde_json::json!([1]),
        serde_json::json!([1, "s", 2]),
    ];
    let expected: Vec<bool> = serde_accepts::<Unit>(&instances)
        .into_iter()
        .chain(serde_accepts::<Pair>(&instances))
        .collect();
    let actual: Vec<bool> = schema_accepts::<Unit>(&instances)
        .into_iter()
        .chain(schema_accepts::<Pair>(&instances))
        .collect();
    assert_eq!(expected, actual);
}

fn described(_generator: &mut SchemaGenerator) -> JsonSchema {
    JsonSchema {
        description: Some("any value".to_string()),
        ..JsonSchema::default()
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Overrides {
    #[json_schema(with = "String")]
    as_string: u8,
    #[json_schema(schema_with = "described")]
    custom: u8,
}

#[test]
fn field_with_and_schema_with_replace_the_field_schema() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "as_string": { "type": "string" },
            "custom": { "description": "any value" }
        },
        "additionalProperties": false,
        "required": ["as_string", "custom"]
    })));
    let actual: ReverseCodeGenResult = Overrides::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[serde(tag = "type")]
#[expect(dead_code)]
enum Documented {
    /// A circle.
    Circle { radius: f64 },
}

#[test]
fn variant_doc_comment_describes_its_branch() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "description": "A circle.",
        "properties": {
            "type": { "type": "string", "const": "Circle" },
            "radius": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX }
        },
        "additionalProperties": false,
        "required": ["type", "radius"]
    })));
    let actual: ReverseCodeGenResult = Documented::json_schema();
    assert_eq!(expected, actual);
}

// A flattened map next to a flattened struct.
#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct FlattenMapAndStruct {
    #[serde(flatten)]
    side: Side,
    #[serde(flatten)]
    rest: BTreeMap<String, i32>,
}

#[test]
fn flattened_struct_and_map_merge_into_one_object() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": { "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX } },
        "additionalProperties": { "type": "integer", "minimum": i32::MIN, "maximum": i32::MAX },
        "required": ["side"]
    })));
    let actual: ReverseCodeGenResult = FlattenMapAndStruct::json_schema();
    assert_eq!(expected, actual);
}

#[derive(Deserialize, ToJsonSchema)]
#[expect(dead_code)]
struct Sides(Side, Side);

#[test]
fn named_types_inside_prefix_items_are_shared_definitions() {
    let expected: ReverseCodeGenResult = Ok(schema_from_json(serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "array",
        "prefixItems": [{ "$ref": "#/$defs/Side" }, { "$ref": "#/$defs/Side" }],
        "minItems": 2,
        "maxItems": 2,
        "$defs": {
            "Side": {
                "type": "object",
                "properties": { "side": { "type": "number", "minimum": f64::MIN, "maximum": f64::MAX } },
                "additionalProperties": false,
                "required": ["side"]
            }
        }
    })));
    let actual: ReverseCodeGenResult = Sides::json_schema();
    assert_eq!(expected, actual);
}
