//! Integration test: `SchemaGenerator::new(ReverseCodeGenSettings { .. })` writes the schema of a
//! derived type in the target dialect and document layout: the dialect conversions, the
//! definitions path, inlined subschemas, the meta schema and the number formats.

use json_schema_rs::{
    MetaSchema, NumberFormats, ReverseCodeGenError, ReverseCodeGenSettings, SchemaGenerator,
    TargetDialect, ToJsonSchema, meta,
};
use json_schema_rs_macro::ToJsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Address {
    city: String,
}

#[derive(ToJsonSchema)]
#[json_schema(title = "Order")]
#[expect(dead_code)]
struct Order {
    #[json_schema(exclusive_minimum = 0)]
    quantity: u8,
    nicknames: Vec<Option<String>>,
    buyer: Address,
    #[json_schema(deprecated = true)]
    backup: Address,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
enum Mode {
    Fast,
    Slow,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
enum Only {
    Single,
}

#[derive(ToJsonSchema, Deserialize)]
#[expect(dead_code)]
enum Shape {
    Circle(f64),
    Square { side: f64 },
}

/// A type that uses most of what the dialect conversions rewrite.
#[derive(ToJsonSchema)]
#[json_schema(title = "Everything")]
#[expect(dead_code)]
struct Everything {
    id: u8,
    #[json_schema(exclusive_minimum = 0, exclusive_maximum = 10)]
    ratio: f64,
    #[json_schema(minimum = 1, exclusive_maximum = 9)]
    mixed: f64,
    nicknames: Vec<Option<String>>,
    buyer: Address,
    #[json_schema(deprecated = true)]
    backup: Address,
    mode: Mode,
    only: Only,
    shape: Shape,
    tags: HashSet<String>,
    #[json_schema(min_items = 1, max_items = 3)]
    triple: Vec<u8>,
    #[json_schema(default = 5)]
    limit: u32,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Node {
    next: Option<Box<Node>>,
    address: Address,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/leaf.json")]
#[expect(dead_code)]
struct Leaf {
    v: u8,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Tree {
    leaf: Leaf,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Orchard {
    pickers: Vec<Option<Address>>,
}

#[derive(ToJsonSchema, Deserialize)]
#[expect(dead_code)]
struct Flat {
    a: u8,
    #[serde(flatten)]
    shape: Shape,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Numbers {
    small: i8,
    wide: i64,
    unsigned: u64,
    single: f32,
    double: f64,
}

fn settings_for(dialect: TargetDialect) -> ReverseCodeGenSettings {
    ReverseCodeGenSettings {
        dialect: Some(dialect),
        ..ReverseCodeGenSettings::default()
    }
}

/// The schema of `T` written as `settings` say, as JSON.
fn written<T: ToJsonSchema>(
    settings: ReverseCodeGenSettings,
) -> Result<Value, Vec<ReverseCodeGenError>> {
    SchemaGenerator::new(settings)
        .into_root_schema_for::<T>()
        .map(|schema| serde_json::to_value(&schema).expect("serialize"))
}

fn written_in<T: ToJsonSchema>(dialect: TargetDialect) -> Result<Value, Vec<ReverseCodeGenError>> {
    written::<T>(settings_for(dialect))
}

fn address_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"city": {"type": "string"}},
        "additionalProperties": false,
        "required": ["city"]
    })
}

#[test]
fn default_settings_write_the_same_schema_as_json_schema() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> =
        Order::json_schema().map(|schema| serde_json::to_value(&schema).expect("serialize"));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written::<Order>(ReverseCodeGenSettings::default());
    assert_eq!(expected, actual);
}

#[test]
fn order_in_2020_12() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Order",
        "type": "object",
        "$defs": {"Address": address_schema()},
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/$defs/Address"},
            "backup": {"$ref": "#/$defs/Address", "deprecated": true}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_2019_09() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2019-09/schema",
        "title": "Order",
        "type": "object",
        "$defs": {"Address": address_schema()},
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/$defs/Address"},
            "backup": {"$ref": "#/$defs/Address", "deprecated": true}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::Draft201909);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_draft_07() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Order",
        "type": "object",
        "definitions": {"Address": address_schema()},
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/definitions/Address"},
            "backup": {"deprecated": true, "allOf": [{"$ref": "#/definitions/Address"}]}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_draft_06() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-06/schema#",
        "title": "Order",
        "type": "object",
        "definitions": {"Address": address_schema()},
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/definitions/Address"},
            "backup": {"deprecated": true, "allOf": [{"$ref": "#/definitions/Address"}]}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::Draft06);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_draft_04() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-04/schema#",
        "title": "Order",
        "type": "object",
        "definitions": {"Address": address_schema()},
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": true},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/definitions/Address"},
            "backup": {"deprecated": true, "allOf": [{"$ref": "#/definitions/Address"}]}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::Draft04);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_openapi_30() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "title": "Order",
        "type": "object",
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": true},
            "nicknames": {"type": "array", "items": {"type": "string", "nullable": true}},
            "buyer": {"$ref": "#/components/schemas/Address"},
            "backup": {"deprecated": true, "allOf": [{"$ref": "#/components/schemas/Address"}]}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"],
        "components": {"schemas": {"Address": address_schema()}}
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::OpenApi30);
    assert_eq!(expected, actual);
}

#[test]
fn order_in_openapi_31() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "title": "Order",
        "type": "object",
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/components/schemas/Address"},
            "backup": {"$ref": "#/components/schemas/Address", "deprecated": true}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"],
        "components": {"schemas": {"Address": address_schema()}}
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Order>(TargetDialect::OpenApi31);
    assert_eq!(expected, actual);
}

#[test]
fn a_single_variant_enum_is_const_from_draft_06_and_enum_in_draft_04_and_openapi_30() {
    let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        Ok(
            json!({"$schema": "http://json-schema.org/draft-04/schema#", "type": "string", "enum": ["Single"]}),
        ),
        Ok(
            json!({"$schema": "http://json-schema.org/draft-06/schema#", "type": "string", "const": "Single"}),
        ),
        Ok(json!({"type": "string", "enum": ["Single"]})),
        Ok(json!({"type": "string", "const": "Single"})),
    ];
    let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        written_in::<Only>(TargetDialect::Draft04),
        written_in::<Only>(TargetDialect::Draft06),
        written_in::<Only>(TargetDialect::OpenApi30),
        written_in::<Only>(TargetDialect::OpenApi31),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn an_inclusive_and_an_exclusive_bound_keep_the_tighter_one_in_draft_04() {
    let expected: Value =
        json!({"type": "number", "minimum": 1, "maximum": 9, "exclusiveMaximum": true});
    let schema: Value = written_in::<Everything>(TargetDialect::Draft04).expect("draft-04 schema");
    let actual: Value = schema["properties"]["mixed"].clone();
    assert_eq!(expected, actual);
}

#[test]
fn both_exclusive_bounds_become_boolean_modifiers_in_draft_04() {
    let expected: Value = json!({
        "type": "number",
        "minimum": 0,
        "maximum": 10,
        "exclusiveMinimum": true,
        "exclusiveMaximum": true
    });
    let schema: Value = written_in::<Everything>(TargetDialect::Draft04).expect("draft-04 schema");
    let actual: Value = schema["properties"]["ratio"].clone();
    assert_eq!(expected, actual);
}

// --- every draft's output is valid against its own metaschema ---

/// Whether `Everything` written as `dialect` conforms to its draft's metaschema.
fn conforms(dialect: TargetDialect) -> bool {
    let schema: Value = written_in::<Everything>(dialect).expect("schema");
    meta::is_valid(&schema)
}

#[test]
fn draft_04_output_conforms_to_the_draft_04_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::Draft04);
    assert_eq!(expected, actual);
}

#[test]
fn draft_06_output_conforms_to_the_draft_06_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::Draft06);
    assert_eq!(expected, actual);
}

#[test]
fn draft_07_output_conforms_to_the_draft_07_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn draft_2019_09_output_conforms_to_the_2019_09_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::Draft201909);
    assert_eq!(expected, actual);
}

#[test]
fn draft_2020_12_output_conforms_to_the_2020_12_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn openapi_31_output_conforms_to_the_2020_12_metaschema() {
    let expected: bool = true;
    let actual: bool = conforms(TargetDialect::OpenApi31);
    assert_eq!(expected, actual);
}

#[test]
fn a_dialect_with_resources_conforms_to_its_metaschema() {
    let schema: Value = written_in::<Tree>(TargetDialect::Draft04).expect("schema");
    let expected: bool = true;
    let actual: bool = meta::is_valid(&schema);
    assert_eq!(expected, actual);
}

/// Whether `Everything` written as `dialect`, read back from its JSON text under the draft its
/// `$schema` names, accepts `instance`.
fn accepts(dialect: TargetDialect, instance: &Value) -> bool {
    let text: String = serde_json::to_string(&written_in::<Everything>(dialect).expect("schema"))
        .expect("serialize");
    let schema: json_schema_rs::Schema = json_schema_rs::Schema::try_from(text).expect("parse");
    json_schema_rs::validate(&schema, instance).is_ok()
}

#[test]
fn every_dialect_accepts_the_instances_the_derived_schema_accepts() {
    let instance: Value = json!({
        "id": 1, "ratio": 5, "mixed": 2, "nicknames": [null, "a"],
        "buyer": {"city": "X"}, "backup": {"city": "Y"}, "mode": "Fast", "only": "Single",
        "shape": {"Circle": 1.5}, "tags": ["a"], "triple": [1], "limit": 5
    });
    let expected: Vec<bool> = vec![true; 6];
    let actual: Vec<bool> = [
        TargetDialect::Draft04,
        TargetDialect::Draft06,
        TargetDialect::Draft07,
        TargetDialect::Draft201909,
        TargetDialect::Draft202012,
        TargetDialect::OpenApi31,
    ]
    .into_iter()
    .map(|dialect: TargetDialect| accepts(dialect, &instance))
    .collect();
    assert_eq!(expected, actual);
}

#[test]
fn every_dialect_rejects_the_instances_the_derived_schema_rejects() {
    // `ratio` at its exclusive bound and `mixed` at its exclusive bound.
    let instance: Value = json!({
        "id": 1, "ratio": 10, "mixed": 9, "nicknames": [], "buyer": {"city": "X"},
        "backup": {"city": "Y"}, "mode": "Fast", "only": "Single", "shape": {"Circle": 1.5},
        "tags": [], "triple": [1], "limit": 5
    });
    let expected: Vec<bool> = vec![false; 6];
    let actual: Vec<bool> = [
        TargetDialect::Draft04,
        TargetDialect::Draft06,
        TargetDialect::Draft07,
        TargetDialect::Draft201909,
        TargetDialect::Draft202012,
        TargetDialect::OpenApi31,
    ]
    .into_iter()
    .map(|dialect: TargetDialect| accepts(dialect, &instance))
    .collect();
    assert_eq!(expected, actual);
}

// --- what a target cannot express is an error ---

#[test]
fn a_nullable_ref_is_unrepresentable_in_openapi_30() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> =
        Err(vec![ReverseCodeGenError::UnrepresentableKeyword {
            dialect: TargetDialect::OpenApi30,
            keyword: "type".to_string(),
            pointer: "/properties/pickers/items/anyOf/1".to_string(),
        }]);
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Orchard>(TargetDialect::OpenApi30);
    assert_eq!(expected, actual);
}

#[test]
fn a_nullable_ref_is_fine_in_draft_07() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "definitions": {"Address": address_schema()},
        "properties": {
            "pickers": {
                "type": "array",
                "items": {"anyOf": [{"$ref": "#/definitions/Address"}, {"type": "null"}]}
            }
        },
        "additionalProperties": false,
        "required": ["pickers"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Orchard>(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn a_flattened_enum_needs_unevaluated_properties_which_older_dialects_lack() {
    let unevaluated = |dialect: TargetDialect| ReverseCodeGenError::UnrepresentableKeyword {
        dialect,
        keyword: "unevaluatedProperties".to_string(),
        pointer: String::new(),
    };
    let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        Err(vec![unevaluated(TargetDialect::Draft04)]),
        Err(vec![unevaluated(TargetDialect::Draft07)]),
        Err(vec![unevaluated(TargetDialect::OpenApi30)]),
    ];
    let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        written_in::<Flat>(TargetDialect::Draft04),
        written_in::<Flat>(TargetDialect::Draft07),
        written_in::<Flat>(TargetDialect::OpenApi30),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn a_flattened_enum_keeps_unevaluated_properties_in_2019_09() {
    let schema: Value = written_in::<Flat>(TargetDialect::Draft201909).expect("2019-09 schema");
    let expected: Value = json!(false);
    let actual: Value = schema["unevaluatedProperties"].clone();
    assert_eq!(expected, actual);
}

// --- schema resources ---

#[test]
fn a_resource_id_is_id_in_draft_04_and_dollar_id_in_draft_07() {
    let leaf = |id_keyword: &str| -> Value {
        json!({
            "type": "object",
            "properties": {"v": {"type": "integer", "minimum": 0, "maximum": 255}},
            "additionalProperties": false,
            "required": ["v"],
            id_keyword: "https://example.com/leaf.json"
        })
    };
    let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        Ok(json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "type": "object",
            "definitions": {"Leaf": leaf("id")},
            "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
            "additionalProperties": false,
            "required": ["leaf"]
        })),
        Ok(json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"Leaf": leaf("$id")},
            "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
            "additionalProperties": false,
            "required": ["leaf"]
        })),
    ];
    let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
        written_in::<Tree>(TargetDialect::Draft04),
        written_in::<Tree>(TargetDialect::Draft07),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn a_resource_id_is_unrepresentable_in_openapi_30() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> =
        Err(vec![ReverseCodeGenError::UnrepresentableKeyword {
            dialect: TargetDialect::OpenApi30,
            keyword: "$id".to_string(),
            pointer: "/components/schemas/Leaf".to_string(),
        }]);
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Tree>(TargetDialect::OpenApi30);
    assert_eq!(expected, actual);
}

#[test]
fn a_resource_cannot_sit_under_a_custom_definitions_path() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> =
        Err(vec![ReverseCodeGenError::ResourceOutsideDefinitions {
            path: "/schemas".to_string(),
            schema_id: format!("{}::Leaf", module_path!()),
        }]);
    let actual: Result<Value, Vec<ReverseCodeGenError>> = written::<Tree>(ReverseCodeGenSettings {
        definitions_path: Some("/schemas".to_string()),
        ..ReverseCodeGenSettings::default()
    });
    assert_eq!(expected, actual);
}

// --- document layout and number formats ---

#[test]
fn inline_subschemas_writes_nested_types_in_place() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Order",
        "type": "object",
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": address_schema(),
            "backup": {
                "type": "object",
                "properties": {"city": {"type": "string"}},
                "additionalProperties": false,
                "required": ["city"],
                "deprecated": true
            }
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written::<Order>(ReverseCodeGenSettings {
            inline_subschemas: true,
            ..ReverseCodeGenSettings::default()
        });
    assert_eq!(expected, actual);
}

#[test]
fn inline_subschemas_keeps_a_definition_for_a_recursive_type() {
    let node: Value = json!({
        "type": "object",
        "properties": {
            "next": {"$ref": "#/$defs/Node"},
            "address": address_schema()
        },
        "additionalProperties": false,
        "required": ["address"]
    });
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "$defs": {"Node": node},
        "properties": {
            "next": {"$ref": "#/$defs/Node"},
            "address": address_schema()
        },
        "additionalProperties": false,
        "required": ["address"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> = written::<Node>(ReverseCodeGenSettings {
        inline_subschemas: true,
        ..ReverseCodeGenSettings::default()
    });
    assert_eq!(expected, actual);
}

#[test]
fn a_definitions_path_places_definitions_and_refs_like_pydantic_and_openapi_do() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Order",
        "type": "object",
        "properties": {
            "quantity": {"type": "integer", "minimum": 0, "maximum": 255, "exclusiveMinimum": 0},
            "nicknames": {"type": "array", "items": {"type": ["null", "string"]}},
            "buyer": {"$ref": "#/components/schemas/Address"},
            "backup": {"$ref": "#/components/schemas/Address", "deprecated": true}
        },
        "additionalProperties": false,
        "required": ["quantity", "nicknames", "buyer", "backup"],
        "components": {"schemas": {"Address": address_schema()}}
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written::<Order>(ReverseCodeGenSettings {
            definitions_path: Some("#/components/schemas/".to_string()),
            ..ReverseCodeGenSettings::default()
        });
    assert_eq!(expected, actual);
}

#[test]
fn the_meta_schema_can_be_omitted_or_replaced() {
    let expected: Vec<Option<Value>> = vec![None, Some(json!("https://example.com/meta"))];
    let actual: Vec<Option<Value>> = [
        MetaSchema::Omit,
        MetaSchema::Uri("https://example.com/meta".to_string()),
    ]
    .into_iter()
    .map(|meta_schema: MetaSchema| {
        written::<Address>(ReverseCodeGenSettings {
            meta_schema,
            ..ReverseCodeGenSettings::default()
        })
        .expect("schema")
        .get("$schema")
        .cloned()
    })
    .collect();
    assert_eq!(expected, actual);
}

#[test]
fn openapi_number_formats_are_written_on_number_fields() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "small": {"type": "integer", "minimum": -128, "maximum": 127, "format": "int32"},
            "wide": {
                "type": "integer",
                "minimum": -9_223_372_036_854_775_808_i64,
                "maximum": 9_223_372_036_854_775_807_i64,
                "format": "int64"
            },
            "unsigned": {"type": "integer", "minimum": 0, "maximum": 18_446_744_073_709_551_615_u64},
            "single": {
                "type": "number",
                "minimum": f64::from(f32::MIN),
                "maximum": f64::from(f32::MAX),
                "format": "float"
            },
            "double": {
                "type": "number",
                "minimum": f64::MIN,
                "maximum": f64::MAX,
                "format": "double"
            }
        },
        "additionalProperties": false,
        "required": ["small", "wide", "unsigned", "single", "double"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written::<Numbers>(ReverseCodeGenSettings {
            number_formats: Some(NumberFormats::OpenApi),
            ..ReverseCodeGenSettings::default()
        });
    assert_eq!(expected, actual);
}

#[test]
fn schemars_number_formats_are_written_on_number_fields() {
    let schema: Value = written::<Numbers>(ReverseCodeGenSettings {
        number_formats: Some(NumberFormats::Schemars),
        ..ReverseCodeGenSettings::default()
    })
    .expect("schema");
    let expected: Vec<Value> = vec![
        json!("int8"),
        json!("int64"),
        json!("uint64"),
        json!("float"),
        json!("double"),
    ];
    let actual: Vec<Value> = ["small", "wide", "unsigned", "single", "double"]
        .into_iter()
        .map(|field: &str| schema["properties"][field]["format"].clone())
        .collect();
    assert_eq!(expected, actual);
}

// --- the migration example in the README ---

#[test]
fn the_readme_migration_example_writes_a_draft_07_schema() {
    // README, "Reverse codegen": the schemars `SchemaSettings::draft07()` generator.
    let schema: json_schema_rs::ReverseCodeGenResult =
        SchemaGenerator::new(ReverseCodeGenSettings {
            dialect: Some(TargetDialect::Draft07),
            ..Default::default()
        })
        .into_root_schema_for::<Address>();
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"city": {"type": "string"}},
        "additionalProperties": false,
        "required": ["city"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        schema.map(|schema| serde_json::to_value(&schema).expect("serialize"));
    assert_eq!(expected, actual);
}

#[test]
fn the_readme_schemars_options_map_one_to_one() {
    // `definitions_path = "/components/schemas/"`, `inline_subschemas = true`,
    // `meta_schema = None`.
    let settings: ReverseCodeGenSettings = ReverseCodeGenSettings {
        definitions_path: Some("/components/schemas/".to_string()),
        inline_subschemas: true,
        meta_schema: MetaSchema::Omit,
        ..Default::default()
    };
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "type": "object",
        "properties": {"city": {"type": "string"}},
        "additionalProperties": false,
        "required": ["city"]
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> = written::<Address>(settings);
    assert_eq!(expected, actual);
}
