//! Integration test: `#[json_schema(id = "...")]` with a fragment-only or empty `id` (`"#a"`,
//! `""`) names an anchor, not a schema resource, so the type is a definition of the enclosing
//! resource and its refs resolve there, on every target, as the root, as a field and recursively.
//! An `id` with a URI part before the fragment (`"x.json#a"`) still starts a resource.

use json_schema_rs::{
    GeneratedSchema, ReverseCodeGenError, ReverseCodeGenSettings, Schema, SchemaGenerator,
    TargetDialect, ToJsonSchema, ValidateError,
};
use json_schema_rs_macro::ToJsonSchema;
use serde_json::{Value, json};

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Leaf {
    v: String,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "#a")]
#[expect(dead_code)]
struct Anchored {
    leaf: Leaf,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "")]
#[expect(dead_code)]
struct Blank {
    leaf: Leaf,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct Holder {
    a: Anchored,
    b: Blank,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "#r")]
#[expect(dead_code)]
struct Rec {
    leaf: Leaf,
    next: Option<Box<Rec>>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct RecHolder {
    rec: Rec,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "#dup")]
#[expect(dead_code)]
struct DupOne {
    n: u8,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "#dup")]
#[expect(dead_code)]
struct DupTwo {
    n: u8,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct DupHolder {
    one: DupOne,
    two: DupTwo,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "x.json#a")]
#[expect(dead_code)]
struct Remote {
    leaf: Leaf,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct RemoteHolder {
    remote: Remote,
}

const TARGETS: [TargetDialect; 7] = [
    TargetDialect::Draft04,
    TargetDialect::Draft06,
    TargetDialect::Draft07,
    TargetDialect::Draft201909,
    TargetDialect::Draft202012,
    TargetDialect::OpenApi30,
    TargetDialect::OpenApi31,
];

/// For each target: whether the refs of `to_value()` and of `schema()` resolve, or the errors.
type ResolvedByTarget = Vec<(
    TargetDialect,
    Result<(bool, bool), Vec<ReverseCodeGenError>>,
)>;

/// For each target: the document `to_value()` writes, or the errors.
type WrittenByTarget = Vec<(TargetDialect, Result<Value, Vec<ReverseCodeGenError>>)>;

fn settings_for(dialect: TargetDialect) -> ReverseCodeGenSettings {
    ReverseCodeGenSettings {
        dialect: Some(dialect),
        ..ReverseCodeGenSettings::default()
    }
}

fn generated<T: ToJsonSchema>(
    settings: ReverseCodeGenSettings,
) -> Result<GeneratedSchema, Vec<ReverseCodeGenError>> {
    SchemaGenerator::new(settings).into_root_schema_for::<T>()
}

/// The document `to_value()` writes for `T` in `dialect`.
fn written_in<T: ToJsonSchema>(dialect: TargetDialect) -> Result<Value, Vec<ReverseCodeGenError>> {
    generated::<T>(settings_for(dialect)).map(|generated| generated.to_value())
}

/// The 2020-12 model `schema()` returns for `T` in `dialect`, as JSON.
fn model_in<T: ToJsonSchema>(dialect: TargetDialect) -> Result<Value, Vec<ReverseCodeGenError>> {
    generated::<T>(settings_for(dialect))
        .map(|generated| serde_json::to_value(generated.schema()).expect("serialize"))
}

/// Whether every `$ref` in `document` resolves: the registry and compiler accept it.
fn refs_resolve(document: &Value) -> bool {
    let schema: Schema = Schema::try_from(document).expect("parse");
    !matches!(
        json_schema_rs::validate(&schema, &Value::Null),
        Err(ValidateError::Schema(_))
    )
}

/// For each target: whether the refs of `to_value()` and of `schema()` for `T` resolve, or the
/// errors when `T` cannot be written there.
fn refs_resolve_on_every_target<T: ToJsonSchema>() -> ResolvedByTarget {
    TARGETS
        .iter()
        .map(|target| {
            let resolved: Result<(bool, bool), Vec<ReverseCodeGenError>> =
                generated::<T>(settings_for(*target)).map(|generated| {
                    let model: Value = serde_json::to_value(generated.schema()).expect("serialize");
                    (refs_resolve(&generated.to_value()), refs_resolve(&model))
                });
            (*target, resolved)
        })
        .collect()
}

/// What [`refs_resolve_on_every_target`] gives when every target but `OpenApi30` (which has no
/// `$id`) writes the type and all its refs resolve; `OpenApi30` gives `errors_in_openapi_30`.
fn resolving_everywhere_but_openapi_30(
    errors_in_openapi_30: &[ReverseCodeGenError],
) -> ResolvedByTarget {
    TARGETS
        .iter()
        .map(|target| match target {
            TargetDialect::OpenApi30 => (*target, Err(errors_in_openapi_30.to_vec())),
            _ => (*target, Ok((true, true))),
        })
        .collect()
}

fn unrepresentable_id(pointer: &str) -> ReverseCodeGenError {
    ReverseCodeGenError::UnrepresentableKeyword {
        dialect: TargetDialect::OpenApi30,
        keyword: "$id".to_string(),
        pointer: pointer.to_string(),
    }
}

fn leaf_schema() -> Value {
    json!({
        "type": "object",
        "properties": {"v": {"type": "string"}},
        "required": ["v"],
        "additionalProperties": false
    })
}

#[test]
fn fragment_only_and_empty_ids_are_definitions_of_the_document_in_2020_12() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "Anchored": {
                "$anchor": "a",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/$defs/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Blank": {
                "$id": "",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/$defs/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Leaf": leaf_schema()
        },
        "type": "object",
        "properties": {"a": {"$ref": "#/$defs/Anchored"}, "b": {"$ref": "#/$defs/Blank"}},
        "required": ["a", "b"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Holder>(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_and_empty_ids_are_definitions_of_the_document_in_draft_04() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-04/schema#",
        "definitions": {
            "Anchored": {
                "id": "#a",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Blank": {
                "id": "",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Leaf": leaf_schema()
        },
        "type": "object",
        "properties": {
            "a": {"$ref": "#/definitions/Anchored"},
            "b": {"$ref": "#/definitions/Blank"}
        },
        "required": ["a", "b"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Holder>(TargetDialect::Draft04);
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_and_empty_ids_keep_the_2020_12_model_definitions_in_draft_04() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "Anchored": {
                "$id": "#a",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/$defs/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Blank": {
                "$id": "",
                "type": "object",
                "properties": {"leaf": {"$ref": "#/$defs/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Leaf": leaf_schema()
        },
        "type": "object",
        "properties": {"a": {"$ref": "#/$defs/Anchored"}, "b": {"$ref": "#/$defs/Blank"}},
        "required": ["a", "b"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        model_in::<Holder>(TargetDialect::Draft04);
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_and_empty_ids_as_fields_leave_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget = resolving_everywhere_but_openapi_30(&[
        unrepresentable_id("/components/schemas/Anchored"),
        unrepresentable_id("/components/schemas/Blank"),
    ]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<Holder>();
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_and_empty_ids_are_never_written_in_place() {
    let leaf_inlined: Value = leaf_schema();
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "Anchored": {
                "$anchor": "a",
                "type": "object",
                "properties": {"leaf": leaf_inlined},
                "required": ["leaf"],
                "additionalProperties": false
            },
            "Blank": {
                "$id": "",
                "type": "object",
                "properties": {"leaf": leaf_schema()},
                "required": ["leaf"],
                "additionalProperties": false
            }
        },
        "type": "object",
        "properties": {"a": {"$ref": "#/$defs/Anchored"}, "b": {"$ref": "#/$defs/Blank"}},
        "required": ["a", "b"],
        "additionalProperties": false
    }));
    let settings: ReverseCodeGenSettings = ReverseCodeGenSettings {
        inline_subschemas: true,
        ..ReverseCodeGenSettings::default()
    };
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        generated::<Holder>(settings).map(|generated| generated.to_value());
    assert_eq!(expected, actual);
}

#[test]
fn recursive_fragment_only_id_field_refers_to_its_definition_in_2020_12() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "Leaf": leaf_schema(),
            "Rec": {
                "$anchor": "r",
                "type": "object",
                "properties": {
                    "leaf": {"$ref": "#/$defs/Leaf"},
                    "next": {"$ref": "#/$defs/Rec"}
                },
                "required": ["leaf"],
                "additionalProperties": false
            }
        },
        "type": "object",
        "properties": {"rec": {"$ref": "#/$defs/Rec"}},
        "required": ["rec"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<RecHolder>(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn recursive_fragment_only_id_field_refers_to_its_definition_in_draft_07() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {
            "Leaf": leaf_schema(),
            "Rec": {
                "$id": "#r",
                "type": "object",
                "properties": {
                    "leaf": {"$ref": "#/definitions/Leaf"},
                    "next": {"$ref": "#/definitions/Rec"}
                },
                "required": ["leaf"],
                "additionalProperties": false
            }
        },
        "type": "object",
        "properties": {"rec": {"$ref": "#/definitions/Rec"}},
        "required": ["rec"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<RecHolder>(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn recursive_fragment_only_id_field_leaves_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget =
        resolving_everywhere_but_openapi_30(&[unrepresentable_id("/components/schemas/Rec")]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<RecHolder>();
    assert_eq!(expected, actual);
}

#[test]
fn recursive_fragment_only_id_root_refers_to_itself_as_hash() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$anchor": "r",
        "$defs": {"Leaf": leaf_schema()},
        "type": "object",
        "properties": {"leaf": {"$ref": "#/$defs/Leaf"}, "next": {"$ref": "#"}},
        "required": ["leaf"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Rec>(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn recursive_fragment_only_id_root_leaves_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget = resolving_everywhere_but_openapi_30(&[unrepresentable_id("")]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<Rec>();
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_id_root_keeps_its_id_and_definitions_in_draft_04() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-04/schema#",
        "id": "#a",
        "definitions": {"Leaf": leaf_schema()},
        "type": "object",
        "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
        "required": ["leaf"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<Anchored>(TargetDialect::Draft04);
    assert_eq!(expected, actual);
}

#[test]
fn fragment_only_id_root_leaves_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget = resolving_everywhere_but_openapi_30(&[unrepresentable_id("")]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<Anchored>();
    assert_eq!(expected, actual);
}

#[test]
fn empty_id_root_leaves_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget = resolving_everywhere_but_openapi_30(&[unrepresentable_id("")]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<Blank>();
    assert_eq!(expected, actual);
}

#[test]
fn two_types_with_one_fragment_only_id_are_a_duplicate_anchor_on_every_target() {
    let anchor = |first: &str, second: &str| -> Result<Value, Vec<ReverseCodeGenError>> {
        Err(vec![ReverseCodeGenError::DuplicateAnchor {
            first: first.to_string(),
            second: second.to_string(),
            anchor: "dup".to_string(),
        }])
    };
    let draft_4_to_7: Result<Value, Vec<ReverseCodeGenError>> =
        anchor("#/definitions/DupOne", "#/definitions/DupTwo");
    let expected: WrittenByTarget = vec![
        (TargetDialect::Draft04, draft_4_to_7.clone()),
        (TargetDialect::Draft06, draft_4_to_7.clone()),
        (TargetDialect::Draft07, draft_4_to_7),
        (
            TargetDialect::Draft201909,
            anchor("#/$defs/DupOne", "#/$defs/DupTwo"),
        ),
        (
            TargetDialect::Draft202012,
            anchor("#/$defs/DupOne", "#/$defs/DupTwo"),
        ),
        (
            TargetDialect::OpenApi30,
            Err(vec![
                unrepresentable_id("/components/schemas/DupOne"),
                unrepresentable_id("/components/schemas/DupTwo"),
            ]),
        ),
        (
            TargetDialect::OpenApi31,
            anchor("#/components/schemas/DupOne", "#/components/schemas/DupTwo"),
        ),
    ];
    let actual: WrittenByTarget = TARGETS
        .iter()
        .map(|target| (*target, written_in::<DupHolder>(*target)))
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn two_types_with_one_fragment_only_id_are_a_duplicate_anchor_from_schema() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> =
        Err(vec![ReverseCodeGenError::DuplicateAnchor {
            first: "#/definitions/DupOne".to_string(),
            second: "#/definitions/DupTwo".to_string(),
            anchor: "dup".to_string(),
        }]);
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        model_in::<DupHolder>(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn id_with_a_uri_before_the_fragment_still_starts_a_resource_in_2020_12() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$defs": {
            "Remote": {
                "$id": "x.json",
                "$anchor": "a",
                "$defs": {"Leaf": leaf_schema()},
                "type": "object",
                "properties": {"leaf": {"$ref": "#/$defs/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            }
        },
        "type": "object",
        "properties": {"remote": {"$ref": "#/$defs/Remote"}},
        "required": ["remote"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<RemoteHolder>(TargetDialect::Draft202012);
    assert_eq!(expected, actual);
}

#[test]
fn id_with_a_uri_before_the_fragment_still_starts_a_resource_in_draft_07() {
    let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {
            "Remote": {
                "$id": "x.json#a",
                "definitions": {"Leaf": leaf_schema()},
                "type": "object",
                "properties": {"leaf": {"$ref": "#/definitions/Leaf"}},
                "required": ["leaf"],
                "additionalProperties": false
            }
        },
        "type": "object",
        "properties": {"remote": {"$ref": "#/definitions/Remote"}},
        "required": ["remote"],
        "additionalProperties": false
    }));
    let actual: Result<Value, Vec<ReverseCodeGenError>> =
        written_in::<RemoteHolder>(TargetDialect::Draft07);
    assert_eq!(expected, actual);
}

#[test]
fn id_with_a_uri_before_the_fragment_leaves_no_dangling_ref_on_any_target() {
    let expected: ResolvedByTarget = resolving_everywhere_but_openapi_30(&[
        ReverseCodeGenError::UnrepresentableKeyword {
            dialect: TargetDialect::OpenApi30,
            keyword: "$defs".to_string(),
            pointer: "/components/schemas/Remote".to_string(),
        },
        unrepresentable_id("/components/schemas/Remote"),
    ]);
    let actual: ResolvedByTarget = refs_resolve_on_every_target::<RemoteHolder>();
    assert_eq!(expected, actual);
}
