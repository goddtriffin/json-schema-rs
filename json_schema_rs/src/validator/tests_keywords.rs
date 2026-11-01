//! Keyword-by-keyword tests of the compiled validator: sibling keywords, typeless assertions,
//! `$ref` siblings per draft, in-place loops, the depth guard, every applicator and assertion,
//! `format` modes, error `Display` text and compile errors.

use super::ValidationErrorKind;
use super::eval::DEPTH_LIMIT;
use super::{
    CompileError, CompileErrors, CompiledSchema, ValidationError, ValidationResult, validate,
};
use crate::json_pointer::JsonPointer;
use crate::json_schema::{
    FormatMode, JsonSchema, JsonSchemaSettings, RefSiblings, Registry, Schema, SpecVersion,
};
use serde_json::{Number, Value, json};

/// Parses `json` with the default settings (Draft 2020-12 when `$schema` is absent).
fn schema(json: &str) -> JsonSchema {
    schema_with(json, &JsonSchemaSettings::default())
}

/// Parses `json` as `version` when it has no `$schema`.
fn schema_in(json: &str, version: SpecVersion) -> JsonSchema {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .default_spec_version(version)
        .build();
    schema_with(json, &settings)
}

fn schema_with(json: &str, settings: &JsonSchemaSettings) -> JsonSchema {
    Schema::new_from_str(json, settings)
        .expect("parse schema")
        .into_object()
        .expect("schema object")
}

/// Compiles `schema` through a default registry.
fn compile(schema: &JsonSchema) -> CompiledSchema {
    let registry: Registry<'_> = Registry::new(schema).expect("build registry");
    CompiledSchema::new(&registry).expect("compile schema")
}

/// The compile result of `schema`, without the compiled schema (which has no `PartialEq`).
fn compile_result(schema: &JsonSchema) -> Result<(), CompileErrors> {
    let registry: Registry<'_> = Registry::new(schema).expect("build registry");
    CompiledSchema::new(&registry).map(|_| ())
}

fn ptr(pointer: &str) -> JsonPointer {
    JsonPointer::try_from(pointer).expect("valid JSON Pointer")
}

/// A keyword location.
fn kl(pointer: &str) -> JsonPointer {
    ptr(pointer)
}

fn root() -> JsonPointer {
    JsonPointer::root()
}

fn num(value: i64) -> Number {
    Number::from(value)
}

fn expected_integer(path: &str, keyword_location: &str, got: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::ExpectedInteger {
            got: got.to_string(),
        },
        ptr(path),
        ptr(keyword_location),
    )
}

fn expected_string(path: &str, keyword_location: &str, got: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::ExpectedString {
            got: got.to_string(),
        },
        ptr(path),
        ptr(keyword_location),
    )
}

fn below_minimum(path: &str, keyword_location: &str, minimum: i64, actual: i64) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::BelowMinimum {
            minimum: num(minimum),
            actual: num(actual),
        },
        ptr(path),
        ptr(keyword_location),
    )
}

fn false_schema(path: &str, keyword_location: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::FalseSchema,
        ptr(path),
        ptr(keyword_location),
    )
}

fn unevaluated(path: &str, keyword_location: &str, property: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::DisallowedUnevaluatedProperty {
            property: property.to_string(),
        },
        ptr(path),
        ptr(keyword_location),
    )
}

/// An `anyOf`/`oneOf` at `keyword_location` none of whose branches matched, with the branch
/// errors as `causes`.
fn no_subschema_matched(
    path: &str,
    keyword_location: &str,
    subschema_count: usize,
    causes: Vec<ValidationError>,
) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::NoSubschemaMatched {
            subschema_count,
            causes,
        },
        ptr(path),
        ptr(keyword_location),
    )
}

fn ref_cycle(path: &str, keyword_location: &str, ref_str: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::RefCycle {
            ref_str: ref_str.to_string(),
        },
        ptr(path),
        ptr(keyword_location),
    )
}

/// `levels` arrays nested in one another: `[[...[]...]]`. Built iteratively, since
/// `serde_json` refuses to parse documents nested deeper than 128.
fn nested_arrays(levels: usize) -> Value {
    let mut value: Value = Value::Array(Vec::new());
    for _ in 1..levels {
        value = Value::Array(vec![value]);
    }
    value
}

/// The pointer `/0/0/.../0` with `count` segments.
fn zeros(count: usize) -> JsonPointer {
    JsonPointer::from(vec!["0".to_string(); count])
}

// --- Sibling keywords are all evaluated ---

#[test]
fn const_with_minimum_reports_minimum_when_const_matches() {
    let schema: JsonSchema = schema(r#"{"const":5,"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn const_with_minimum_reports_const_when_above_minimum() {
    let schema: JsonSchema = schema(r#"{"const":5,"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(12));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotConst {
            expected: "5".to_string(),
            actual: "12".to_string(),
        },
        root(),
        kl("/const"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn const_with_minimum_reports_both_when_both_fail() {
    let schema: JsonSchema = schema(r#"{"const":5,"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(3));
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::NotConst {
                expected: "5".to_string(),
                actual: "3".to_string(),
            },
            root(),
            kl("/const"),
        ),
        below_minimum("", "/minimum", 10, 3),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn enum_with_minimum_reports_minimum_for_enum_member() {
    let schema: JsonSchema = schema(r#"{"enum":[1,20],"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![below_minimum("", "/minimum", 10, 1)]);
    assert_eq!(expected, actual);
}

#[test]
fn enum_with_minimum_accepts_enum_member_above_minimum() {
    let schema: JsonSchema = schema(r#"{"enum":[1,20],"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(20));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn enum_with_minimum_reports_both_when_both_fail() {
    let schema: JsonSchema = schema(r#"{"enum":[1,20],"minimum":10}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::NotInEnum {
                invalid_value: "5".to_string(),
                allowed: vec!["1".to_string(), "20".to_string()],
            },
            root(),
            kl("/enum"),
        ),
        below_minimum("", "/minimum", 10, 5),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn empty_enum_rejects_number() {
    let schema: JsonSchema = schema(r#"{"enum":[]}"#);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotInEnum {
            invalid_value: "1".to_string(),
            allowed: Vec::new(),
        },
        root(),
        kl("/enum"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn empty_enum_rejects_null() {
    let schema: JsonSchema = schema(r#"{"enum":[]}"#);
    let actual: ValidationResult = validate(&schema, &Value::Null);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotInEnum {
            invalid_value: "null".to_string(),
            allowed: Vec::new(),
        },
        root(),
        kl("/enum"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn any_of_next_to_type_reports_any_of_for_matching_type() {
    let schema: JsonSchema = schema(r#"{"type":"integer","anyOf":[{"minimum":10},{"maximum":0}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![no_subschema_matched(
        "",
        "/anyOf",
        2,
        vec![
            below_minimum("", "/anyOf/0/minimum", 10, 5),
            ValidationError::new(
                ValidationErrorKind::AboveMaximum {
                    maximum: num(0),
                    actual: num(5),
                },
                root(),
                kl("/anyOf/1/maximum"),
            ),
        ],
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn any_of_next_to_type_reports_type_only_when_branches_ignore_instance() {
    let schema: JsonSchema = schema(r#"{"type":"integer","anyOf":[{"minimum":10},{"maximum":0}]}"#);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Err(vec![expected_integer("", "/type", "string")]);
    assert_eq!(expected, actual);
}

#[test]
fn any_of_next_to_type_accepts_matching_branch() {
    let schema: JsonSchema = schema(r#"{"type":"integer","anyOf":[{"minimum":10},{"maximum":0}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(11));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn one_of_next_to_minimum_reports_multiple_matches() {
    let schema: JsonSchema = schema(r#"{"minimum":0,"oneOf":[{"multipleOf":2},{"multipleOf":3}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(6));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MultipleSubschemasMatched {
            subschema_count: 2,
            match_count: 2,
        },
        root(),
        kl("/oneOf"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn one_of_next_to_minimum_reports_both_failures() {
    let schema: JsonSchema = schema(r#"{"minimum":0,"oneOf":[{"multipleOf":2},{"multipleOf":3}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(-1));
    let expected: ValidationResult = Err(vec![
        below_minimum("", "/minimum", 0, -1),
        no_subschema_matched(
            "",
            "/oneOf",
            2,
            vec![
                ValidationError::new(
                    ValidationErrorKind::NotMultipleOf {
                        multiple_of: num(2),
                        actual: num(-1),
                    },
                    root(),
                    kl("/oneOf/0/multipleOf"),
                ),
                ValidationError::new(
                    ValidationErrorKind::NotMultipleOf {
                        multiple_of: num(3),
                        actual: num(-1),
                    },
                    root(),
                    kl("/oneOf/1/multipleOf"),
                ),
            ],
        ),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn one_of_next_to_minimum_accepts_single_match() {
    let schema: JsonSchema = schema(r#"{"minimum":0,"oneOf":[{"multipleOf":2},{"multipleOf":3}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(4));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn all_of_next_to_type_reports_failing_branch() {
    let schema: JsonSchema = schema(r#"{"type":"number","allOf":[{"minimum":1},{"maximum":3}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::AboveMaximum {
            maximum: num(3),
            actual: num(5),
        },
        root(),
        kl("/allOf/1/maximum"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn all_of_next_to_type_reports_type_for_other_kind() {
    let schema: JsonSchema = schema(r#"{"type":"number","allOf":[{"minimum":1},{"maximum":3}]}"#);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::ExpectedNumber {
            got: "string".to_string(),
        },
        root(),
        kl("/type"),
    )]);
    assert_eq!(expected, actual);
}

// --- Typeless assertions apply to instances of their kind ---

#[test]
fn typeless_maximum_rejects_larger_number() {
    let schema: JsonSchema = schema(r#"{"maximum":3}"#);
    let actual: ValidationResult = validate(&schema, &json!(4));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::AboveMaximum {
            maximum: num(3),
            actual: num(4),
        },
        root(),
        kl("/maximum"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn typeless_maximum_ignores_string() {
    let schema: JsonSchema = schema(r#"{"maximum":3}"#);
    let actual: ValidationResult = validate(&schema, &json!("4"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn typeless_max_items_rejects_longer_array() {
    let schema: JsonSchema = schema(r#"{"maxItems":1}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooManyItems {
            max_items: 1,
            actual_count: 2,
        },
        root(),
        kl("/maxItems"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn typeless_max_items_ignores_object() {
    let schema: JsonSchema = schema(r#"{"maxItems":1}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn typeless_pattern_rejects_non_matching_string() {
    let schema: JsonSchema = schema(r#"{"pattern":"^a"}"#);
    let actual: ValidationResult = validate(&schema, &json!("b"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::PatternMismatch {
            pattern: "^a".to_string(),
            value: "b".to_string(),
        },
        root(),
        kl("/pattern"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn typeless_pattern_ignores_number() {
    let schema: JsonSchema = schema(r#"{"pattern":"^a"}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn typeless_items_rejects_wrong_item() {
    let schema: JsonSchema = schema(r#"{"items":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, "x"]));
    let expected: ValidationResult = Err(vec![expected_integer("/1", "/items/type", "string")]);
    assert_eq!(expected, actual);
}

#[test]
fn typeless_items_ignores_object() {
    let schema: JsonSchema = schema(r#"{"items":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"0": "x"}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn typeless_required_ignores_string() {
    let schema: JsonSchema = schema(r#"{"required":["a"]}"#);
    let actual: ValidationResult = validate(&schema, &json!("a"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- $ref siblings per draft ---

const REF_SIBLINGS_DEFS: &str =
    r##"{"$defs":{"int":{"type":"integer"}},"$ref":"#/$defs/int","minimum":10}"##;
const REF_SIBLINGS_DEFINITIONS: &str =
    r##"{"definitions":{"int":{"type":"integer"}},"$ref":"#/definitions/int","minimum":10}"##;

#[test]
fn ref_siblings_evaluated_in_2020_12() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFS, SpecVersion::Draft202012);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_evaluated_in_2019_09() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFS, SpecVersion::Draft201909);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFINITIONS, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_ignored_in_draft_6() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFINITIONS, SpecVersion::Draft06);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_ignored_in_draft_4() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFINITIONS, SpecVersion::Draft04);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn ref_target_still_applies_in_draft_7() {
    let schema: JsonSchema = schema_in(REF_SIBLINGS_DEFINITIONS, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Err(vec![expected_integer("", "/$ref/type", "string")]);
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_setting_evaluated_overrides_draft_7() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .default_spec_version(SpecVersion::Draft07)
        .ref_siblings(RefSiblings::Evaluated)
        .build();
    let schema: JsonSchema = schema_with(REF_SIBLINGS_DEFINITIONS, &settings);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_setting_ignored_overrides_2020_12() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .ref_siblings(RefSiblings::Ignored)
        .build();
    let schema: JsonSchema = schema_with(REF_SIBLINGS_DEFS, &settings);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_setting_ignored_overrides_2019_09() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .default_spec_version(SpecVersion::Draft201909)
        .ref_siblings(RefSiblings::Ignored)
        .build();
    let schema: JsonSchema = schema_with(REF_SIBLINGS_DEFS, &settings);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- In-place loops ---

const ANY_OF_SELF: &str = r##"{"anyOf":[{"type":"string"},{"$ref":"#"}]}"##;

#[test]
fn any_of_self_ref_rejects_non_string() {
    let schema: JsonSchema = schema(ANY_OF_SELF);
    let actual: ValidationResult = validate(&schema, &json!({"x": 1}));
    let expected: ValidationResult = Err(vec![no_subschema_matched(
        "",
        "/anyOf",
        2,
        vec![
            expected_string("", "/anyOf/0/type", "object"),
            no_subschema_matched(
                "",
                "/anyOf/1/$ref/anyOf",
                2,
                vec![
                    expected_string("", "/anyOf/1/$ref/anyOf/0/type", "object"),
                    ref_cycle("", "/anyOf/1/$ref/anyOf/1/$ref", "#"),
                ],
            ),
        ],
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn any_of_self_ref_accepts_string() {
    let schema: JsonSchema = schema(ANY_OF_SELF);
    let actual: ValidationResult = validate(&schema, &json!("hi"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

const ONE_OF_SELF: &str = r##"{"oneOf":[{"type":"string"},{"$ref":"#"}]}"##;

#[test]
fn one_of_self_ref_rejects_non_string() {
    let schema: JsonSchema = schema(ONE_OF_SELF);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![no_subschema_matched(
        "",
        "/oneOf",
        2,
        vec![
            expected_string("", "/oneOf/0/type", "number"),
            no_subschema_matched(
                "",
                "/oneOf/1/$ref/oneOf",
                2,
                vec![
                    expected_string("", "/oneOf/1/$ref/oneOf/0/type", "number"),
                    ref_cycle("", "/oneOf/1/$ref/oneOf/1/$ref", "#"),
                ],
            ),
        ],
    )]);
    assert_eq!(expected, actual);
}

/// The `$ref` branch re-enters the root once before the loop is cut, and that re-entry matches
/// through its string branch, so both branches of the outer `oneOf` match.
#[test]
fn one_of_self_ref_counts_reentered_branch_for_string() {
    let schema: JsonSchema = schema(ONE_OF_SELF);
    let actual: ValidationResult = validate(&schema, &json!("hi"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MultipleSubschemasMatched {
            subschema_count: 2,
            match_count: 2,
        },
        root(),
        kl("/oneOf"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn all_of_self_ref_reports_ref_cycle() {
    let schema: JsonSchema = schema(r##"{"allOf":[{"$ref":"#"}]}"##);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![ref_cycle("", "/allOf/0/$ref/allOf/0/$ref", "#")]);
    assert_eq!(expected, actual);
}

#[test]
fn all_of_self_ref_reports_sibling_errors_and_ref_cycle() {
    let schema: JsonSchema = schema(r##"{"type":"integer","allOf":[{"$ref":"#"}]}"##);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Err(vec![
        expected_integer("", "/type", "string"),
        expected_integer("", "/allOf/0/$ref/type", "string"),
        ref_cycle("", "/allOf/0/$ref/allOf/0/$ref", "#"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn all_of_self_ref_compiled_is_valid_is_false() {
    let schema: JsonSchema = schema(r##"{"allOf":[{"$ref":"#"}]}"##);
    let compiled: CompiledSchema = compile(&schema);
    let actual: bool = compiled.is_valid(&json!(1));
    let expected: bool = false;
    assert_eq!(expected, actual);
}

/// The inner `not` sees a `RefCycle` (a failing branch), so it passes; the outer `$ref` branch
/// therefore matches and the outer `not` fails.
#[test]
fn not_self_ref_rejects_every_instance() {
    let schema: JsonSchema = schema(r##"{"not":{"$ref":"#"}}"##);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        root(),
        kl("/not"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn mutual_defs_recursion_reports_ref_cycle() {
    let schema: JsonSchema = schema(
        r##"{"$defs":{"A":{"$ref":"#/$defs/B"},"B":{"$ref":"#/$defs/A"}},"$ref":"#/$defs/A"}"##,
    );
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![ref_cycle("", "/$ref/$ref/$ref", "#/$defs/A")]);
    assert_eq!(expected, actual);
}

const MUTUAL_ANY_OF: &str = r##"{
  "$defs": {
    "A": {"anyOf": [{"type": "string"}, {"$ref": "#/$defs/B"}]},
    "B": {"$ref": "#/$defs/A"}
  },
  "$ref": "#/$defs/A"
}"##;

#[test]
fn mutual_defs_recursion_through_any_of_rejects_non_string() {
    let schema: JsonSchema = schema(MUTUAL_ANY_OF);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![no_subschema_matched(
        "",
        "/$ref/anyOf",
        2,
        vec![
            expected_string("", "/$ref/anyOf/0/type", "number"),
            ref_cycle("", "/$ref/anyOf/1/$ref/$ref", "#/$defs/A"),
        ],
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn mutual_defs_recursion_through_any_of_accepts_string() {
    let schema: JsonSchema = schema(MUTUAL_ANY_OF);
    let actual: ValidationResult = validate(&schema, &json!("s"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

const TREE: &str = r##"{
  "type": "object",
  "properties": {
    "name": {"type": "string"},
    "children": {"type": "array", "items": {"$ref": "#"}}
  }
}"##;

#[test]
fn recursive_tree_accepts_nested_instance() {
    let schema: JsonSchema = schema(TREE);
    let instance: Value = json!({
        "name": "a",
        "children": [
            {"name": "b", "children": [{"name": "c", "children": [{"name": "d"}]}]},
            {"name": "e"}
        ]
    });
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn recursive_tree_reports_invalid_leaf_at_depth_three() {
    let schema: JsonSchema = schema(TREE);
    let instance: Value = json!({
        "children": [{"children": [{"children": [{"name": 5}]}]}]
    });
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![expected_string(
        "/children/0/children/0/children/0/name",
        "/properties/children/items/$ref/properties/children/items/$ref/properties/children/items/$ref/properties/name/type",
        "number",
    )]);
    assert_eq!(expected, actual);
}

// --- Depth guard ---

/// Each array level costs two evaluation levels under `{"items":{"$ref":"#"}}` (the root and
/// its `items` subschema), so `DEPTH_LIMIT / 2` nested arrays are the deepest that fit.
#[test]
fn depth_guard_accepts_deepest_nesting_within_limit() {
    let schema: JsonSchema = schema(r##"{"items":{"$ref":"#"}}"##);
    let instance: Value = nested_arrays(DEPTH_LIMIT / 2);
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn depth_guard_rejects_one_level_past_limit() {
    let schema: JsonSchema = schema(r##"{"items":{"$ref":"#"}}"##);
    let instance: Value = nested_arrays(DEPTH_LIMIT / 2 + 1);
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::DepthLimitExceeded { limit: DEPTH_LIMIT },
        zeros(DEPTH_LIMIT / 2),
        kl(&"/items/$ref".repeat(DEPTH_LIMIT / 2)),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn depth_guard_fits_two_mib_thread() {
    let schema: JsonSchema = schema(r##"{"items":{"$ref":"#"}}"##);
    let instance: Value = nested_arrays(DEPTH_LIMIT * 2);
    let actual: ValidationResult = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn_scoped(scope, || validate(&schema, &instance))
            .expect("spawn thread")
            .join()
            .expect("validation thread did not panic")
    });
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::DepthLimitExceeded { limit: DEPTH_LIMIT },
        zeros(DEPTH_LIMIT / 2),
        kl(&"/items/$ref".repeat(DEPTH_LIMIT / 2)),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn depth_guard_is_valid_fits_two_mib_thread() {
    let schema: JsonSchema = schema(r##"{"items":{"$ref":"#"}}"##);
    let compiled: CompiledSchema = compile(&schema);
    let instance: Value = nested_arrays(DEPTH_LIMIT * 2);
    let actual: bool = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn_scoped(scope, || compiled.is_valid(&instance))
            .expect("spawn thread")
            .join()
            .expect("validation thread did not panic")
    });
    let expected: bool = false;
    assert_eq!(expected, actual);
}

/// A schema that accepts every nested array, so `not` of it rejects every nested array.
const NOT_DEEP: &str =
    r##"{"$defs":{"deep":{"items":{"$ref":"#/$defs/deep"}}},"not":{"$ref":"#/$defs/deep"}}"##;

#[test]
fn depth_guard_inside_not_does_not_accept_instance() {
    let schema: JsonSchema = schema(NOT_DEEP);
    let instance: Value = nested_arrays(DEPTH_LIMIT);
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::DepthLimitExceeded { limit: DEPTH_LIMIT },
        zeros(DEPTH_LIMIT / 2 - 1),
        kl(&format!(
            "/not/$ref{}",
            "/items/$ref".repeat(DEPTH_LIMIT / 2 - 1)
        )),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn not_of_accept_all_rejects_shallow_nested_arrays() {
    let schema: JsonSchema = schema(NOT_DEEP);
    let actual: ValidationResult = validate(&schema, &nested_arrays(3));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        root(),
        kl("/not"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn depth_guard_inside_any_of_reports_depth_limit() {
    let schema: JsonSchema = schema(
        r##"{"$defs":{"deep":{"items":{"$ref":"#/$defs/deep"}}},"anyOf":[{"$ref":"#/$defs/deep"}]}"##,
    );
    let instance: Value = nested_arrays(DEPTH_LIMIT);
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::DepthLimitExceeded { limit: DEPTH_LIMIT },
        zeros(DEPTH_LIMIT / 2 - 1),
        kl(&format!(
            "/anyOf/0/$ref{}",
            "/items/$ref".repeat(DEPTH_LIMIT / 2 - 1)
        )),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn root_raw_keyword_with_invalid_value_is_an_invalid_schema() {
    let schema: JsonSchema = schema(r#"{"minProperties":"x"}"#);
    let actual: ValidationResult = validate(&schema, &json!({}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidSchema {
            reason: "root: the value of \"minProperties\" must be a non-negative integer"
                .to_string(),
        },
        root(),
        kl("/minProperties"),
    )]);
    assert_eq!(expected, actual);
}

// --- not ---

#[test]
fn not_rejects_matching_instance() {
    let schema: JsonSchema = schema(r#"{"not":{"type":"string"}}"#);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        root(),
        kl("/not"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn not_accepts_non_matching_instance() {
    let schema: JsonSchema = schema(r#"{"not":{"type":"string"}}"#);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn not_true_rejects_everything() {
    let schema: JsonSchema = schema(r#"{"not":true}"#);
    let actual: ValidationResult = validate(&schema, &Value::Null);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        root(),
        kl("/not"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn not_false_accepts_everything() {
    let schema: JsonSchema = schema(r#"{"not":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": [1]}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn not_reports_nested_location() {
    let schema: JsonSchema = schema(r#"{"properties":{"a":{"not":{"const":1}}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        ptr("/a"),
        kl("/properties/a/not"),
    )]);
    assert_eq!(expected, actual);
}

// --- if / then / else ---

const IF_THEN_ELSE: &str =
    r#"{"if":{"type":"integer"},"then":{"minimum":10},"else":{"type":"string"}}"#;

#[test]
fn if_passes_then_passes() {
    let schema: JsonSchema = schema(IF_THEN_ELSE);
    let actual: ValidationResult = validate(&schema, &json!(12));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_passes_then_fails() {
    let schema: JsonSchema = schema(IF_THEN_ELSE);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/then/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn if_fails_else_passes() {
    let schema: JsonSchema = schema(IF_THEN_ELSE);
    let actual: ValidationResult = validate(&schema, &json!("x"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_fails_else_fails() {
    let schema: JsonSchema = schema(IF_THEN_ELSE);
    let actual: ValidationResult = validate(&schema, &Value::Null);
    let expected: ValidationResult = Err(vec![expected_string("", "/else/type", "null")]);
    assert_eq!(expected, actual);
}

#[test]
fn if_passes_without_then_accepts() {
    let schema: JsonSchema = schema(r#"{"if":{"type":"integer"},"else":{"type":"string"}}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_fails_without_then_applies_else() {
    let schema: JsonSchema = schema(r#"{"if":{"type":"integer"},"else":{"type":"string"}}"#);
    let actual: ValidationResult = validate(&schema, &Value::Null);
    let expected: ValidationResult = Err(vec![expected_string("", "/else/type", "null")]);
    assert_eq!(expected, actual);
}

#[test]
fn if_fails_without_else_accepts() {
    let schema: JsonSchema = schema(r#"{"if":{"type":"integer"},"then":{"minimum":10}}"#);
    let actual: ValidationResult = validate(&schema, &Value::Null);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_passes_without_else_applies_then() {
    let schema: JsonSchema = schema(r#"{"if":{"type":"integer"},"then":{"minimum":10}}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/then/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

#[test]
fn then_without_if_is_ignored() {
    let schema: JsonSchema = schema(r#"{"then":{"minimum":10},"else":false}"#);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_then_ignored_in_draft_6() {
    let schema: JsonSchema = schema_in(
        r#"{"if":{"type":"integer"},"then":{"minimum":10}}"#,
        SpecVersion::Draft06,
    );
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn if_then_applies_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"if":{"type":"integer"},"then":{"minimum":10}}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Err(vec![below_minimum("", "/then/minimum", 10, 5)]);
    assert_eq!(expected, actual);
}

// --- dependentRequired / dependentSchemas / dependencies ---

#[test]
fn dependent_required_reports_each_missing_property() {
    let schema: JsonSchema = schema(r#"{"dependentRequired":{"a":["b","c"]}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::MissingDependentProperty {
                property: "b".to_string(),
                required_by: "a".to_string(),
            },
            root(),
            kl("/dependentRequired"),
        ),
        ValidationError::new(
            ValidationErrorKind::MissingDependentProperty {
                property: "c".to_string(),
                required_by: "a".to_string(),
            },
            root(),
            kl("/dependentRequired"),
        ),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn dependent_required_accepts_all_present() {
    let schema: JsonSchema = schema(r#"{"dependentRequired":{"a":["b","c"]}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2, "c": 3}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependent_required_accepts_trigger_absent() {
    let schema: JsonSchema = schema(r#"{"dependentRequired":{"a":["b","c"]}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"b": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependent_required_ignores_non_object() {
    let schema: JsonSchema = schema(r#"{"dependentRequired":{"a":["b"]}}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependent_required_ignored_in_draft_7() {
    let schema: JsonSchema =
        schema_in(r#"{"dependentRequired":{"a":["b"]}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependent_schemas_applies_when_trigger_present() {
    let schema: JsonSchema = schema(r#"{"dependentSchemas":{"a":{"required":["b"]}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MissingRequired {
            property: "b".to_string(),
        },
        root(),
        kl("/dependentSchemas/a/required"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn dependent_schemas_skipped_when_trigger_absent() {
    let schema: JsonSchema = schema(r#"{"dependentSchemas":{"a":{"required":["b"]}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"c": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependent_schemas_false_rejects_trigger() {
    let schema: JsonSchema = schema(r#"{"dependentSchemas":{"a":false}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![false_schema("", "/dependentSchemas/a")]);
    assert_eq!(expected, actual);
}

#[test]
fn dependencies_array_form_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"dependencies":{"a":["b"]}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MissingDependentProperty {
            property: "b".to_string(),
            required_by: "a".to_string(),
        },
        root(),
        kl("/dependencies"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn dependencies_array_form_accepts_present_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"dependencies":{"a":["b"]}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dependencies_schema_form_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"dependencies":{"a":{"properties":{"b":{"type":"string"}}}}}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Err(vec![expected_string(
        "/b",
        "/dependencies/a/properties/b/type",
        "number",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn dependencies_schema_form_skipped_without_trigger_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"dependencies":{"a":{"properties":{"b":{"type":"string"}}}}}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!({"b": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

/// 2020-12 keeps `dependencies` in its keyword table for compatibility with earlier drafts.
#[test]
fn dependencies_applies_in_2020_12_for_compatibility() {
    let schema: JsonSchema = schema(r#"{"dependencies":{"a":["b"]}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MissingDependentProperty {
            property: "b".to_string(),
            required_by: "a".to_string(),
        },
        root(),
        kl("/dependencies"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn dependent_schemas_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"dependentSchemas":{"a":false}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- prefixItems / items / additionalItems ---

const PREFIX_ITEMS: &str =
    r#"{"prefixItems":[{"type":"integer"},{"type":"string"}],"items":{"type":"boolean"}}"#;

#[test]
fn prefix_items_and_items_accept_matching_array() {
    let schema: JsonSchema = schema(PREFIX_ITEMS);
    let actual: ValidationResult = validate(&schema, &json!([1, "a", true, false]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn items_applies_after_prefix_items() {
    let schema: JsonSchema = schema(PREFIX_ITEMS);
    let actual: ValidationResult = validate(&schema, &json!([1, "a", 3]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::ExpectedBoolean {
            got: "number".to_string(),
        },
        ptr("/2"),
        kl("/items/type"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn prefix_items_reports_each_position() {
    let schema: JsonSchema = schema(PREFIX_ITEMS);
    let actual: ValidationResult = validate(&schema, &json!(["a", 1]));
    let expected: ValidationResult = Err(vec![
        expected_integer("/0", "/prefixItems/0/type", "string"),
        expected_string("/1", "/prefixItems/1/type", "number"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn prefix_items_accepts_shorter_array() {
    let schema: JsonSchema = schema(PREFIX_ITEMS);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn items_false_after_prefix_accepts_prefix_only() {
    let schema: JsonSchema = schema(r#"{"prefixItems":[{"type":"integer"}],"items":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn items_false_after_prefix_rejects_extra_item() {
    let schema: JsonSchema = schema(r#"{"prefixItems":[{"type":"integer"}],"items":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2, 3]));
    let expected: ValidationResult = Err(vec![
        false_schema("/1", "/items"),
        false_schema("/2", "/items"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn prefix_items_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"prefixItems":[{"type":"integer"}]}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!(["x"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn prefix_items_ignored_in_2019_09() {
    let schema: JsonSchema = schema_in(
        r#"{"prefixItems":[{"type":"integer"}]}"#,
        SpecVersion::Draft201909,
    );
    let actual: ValidationResult = validate(&schema, &json!(["x"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn array_items_in_2020_12_is_an_invalid_schema() {
    let schema: JsonSchema = schema(r#"{"items":[{"type":"integer"}]}"#);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidSchema {
            reason: "root: the value of \"items\" must be a schema (an object or a boolean)"
                .to_string(),
        },
        root(),
        kl("/items"),
    )]);
    assert_eq!(expected, actual);
}

const TUPLE_ITEMS: &str = r#"{"items":[{"type":"integer"}],"additionalItems":{"type":"string"}}"#;

#[test]
fn array_items_and_additional_items_accept_in_draft_7() {
    let schema: JsonSchema = schema_in(TUPLE_ITEMS, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!([1, "a", "b"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn additional_items_rejects_in_draft_7() {
    let schema: JsonSchema = schema_in(TUPLE_ITEMS, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!(["x", 2]));
    let expected: ValidationResult = Err(vec![
        expected_integer("/0", "/items/0/type", "string"),
        expected_string("/1", "/additionalItems/type", "number"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn additional_items_rejects_in_2019_09() {
    let schema: JsonSchema = schema_in(TUPLE_ITEMS, SpecVersion::Draft201909);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![expected_string(
        "/1",
        "/additionalItems/type",
        "number",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn additional_items_false_rejects_extra_item_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"items":[{"type":"integer"}],"additionalItems":false}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![false_schema("/1", "/additionalItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn array_items_without_additional_items_accepts_extra_item_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"items":[{"type":"integer"}]}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!([1, "x"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn additional_items_ignored_next_to_schema_items_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"items":{"type":"integer"},"additionalItems":false}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn schema_items_applies_to_every_item_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"items":{"type":"integer"}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!([1, "x"]));
    let expected: ValidationResult = Err(vec![expected_integer("/1", "/items/type", "string")]);
    assert_eq!(expected, actual);
}

// --- contains / minContains / maxContains ---

#[test]
fn contains_accepts_array_with_match() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a", 1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn contains_rejects_array_without_match() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a"]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 1,
            match_count: 0,
        },
        root(),
        kl("/contains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn contains_rejects_empty_array() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!([]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 1,
            match_count: 0,
        },
        root(),
        kl("/contains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn contains_ignores_non_array() {
    let schema: JsonSchema = schema(r#"{"contains":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_rejects_too_few_matches() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"minContains":2}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, "a"]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 2,
            match_count: 1,
        },
        root(),
        kl("/minContains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_accepts_enough_matches() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"minContains":2}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, "a", 2]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn max_contains_rejects_too_many_matches() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"maxContains":1}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2, "a"]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooManyContains {
            max_contains: 1,
            match_count: 2,
        },
        root(),
        kl("/maxContains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn max_contains_accepts_at_most_max_matches() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"maxContains":1}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, "a"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_zero_accepts_empty_array() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"minContains":0}"#);
    let actual: ValidationResult = validate(&schema, &json!([]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_zero_accepts_no_match() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"integer"},"minContains":0}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_zero_still_checks_max_contains() {
    let schema: JsonSchema =
        schema(r#"{"contains":{"type":"integer"},"minContains":0,"maxContains":1}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooManyContains {
            max_contains: 1,
            match_count: 2,
        },
        root(),
        kl("/maxContains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_without_contains_is_ignored() {
    let schema: JsonSchema = schema(r#"{"minContains":2,"maxContains":0}"#);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn contains_rejects_empty_array_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"contains":{"type":"integer"}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!([]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 1,
            match_count: 0,
        },
        root(),
        kl("/contains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn min_contains_zero_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"contains":{"type":"integer"},"minContains":0}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!([]));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 1,
            match_count: 0,
        },
        root(),
        kl("/contains"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn max_contains_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(
        r#"{"contains":{"type":"integer"},"maxContains":1}"#,
        SpecVersion::Draft07,
    );
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn contains_ignored_in_draft_4() {
    let schema: JsonSchema = schema_in(r#"{"contains":{"type":"integer"}}"#, SpecVersion::Draft04);
    let actual: ValidationResult = validate(&schema, &json!([]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- propertyNames ---

#[test]
fn property_names_reports_each_bad_name() {
    let schema: JsonSchema = schema(r#"{"propertyNames":{"maxLength":3}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"ab": 1, "abcd": 2}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidPropertyName {
            property: "abcd".to_string(),
        },
        ptr("/abcd"),
        kl("/propertyNames"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn property_names_accepts_good_names() {
    let schema: JsonSchema = schema(r#"{"propertyNames":{"maxLength":3}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"ab": 1, "abc": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn property_names_false_rejects_any_member() {
    let schema: JsonSchema = schema(r#"{"propertyNames":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidPropertyName {
            property: "a".to_string(),
        },
        ptr("/a"),
        kl("/propertyNames"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn property_names_false_accepts_empty_object() {
    let schema: JsonSchema = schema(r#"{"propertyNames":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn property_names_ignored_in_draft_4() {
    let schema: JsonSchema = schema_in(r#"{"propertyNames":false}"#, SpecVersion::Draft04);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- patternProperties / additionalProperties ---

#[test]
fn pattern_properties_rejects_matching_member() {
    let schema: JsonSchema = schema(r#"{"patternProperties":{"^s_":{"type":"string"}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"other": 1, "s_a": 1}));
    let expected: ValidationResult = Err(vec![expected_string(
        "/s_a",
        "/patternProperties/^s_/type",
        "number",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_ignores_non_matching_member() {
    let schema: JsonSchema = schema(r#"{"patternProperties":{"^s_":{"type":"string"}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"other": 1, "s_a": "x"}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_applies_every_matching_pattern() {
    let schema: JsonSchema =
        schema(r#"{"patternProperties":{"^a":{"type":"integer"},"b$":{"minimum":10}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"ab": 5}));
    let expected: ValidationResult = Err(vec![below_minimum(
        "/ab",
        "/patternProperties/b$/minimum",
        10,
        5,
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_is_unanchored() {
    let schema: JsonSchema = schema(r#"{"patternProperties":{"x":{"type":"integer"}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"axb": "s"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/axb",
        "/patternProperties/x/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

const ADDITIONAL_SCHEMA: &str = r#"{
  "properties": {"id": {}},
  "patternProperties": {"^x-": {}},
  "additionalProperties": {"type": "integer"}
}"#;

#[test]
fn additional_properties_schema_skips_known_members() {
    let schema: JsonSchema = schema(ADDITIONAL_SCHEMA);
    let actual: ValidationResult = validate(&schema, &json!({"id": "s", "n": 1, "x-a": "s"}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn additional_properties_schema_rejects_other_member() {
    let schema: JsonSchema = schema(ADDITIONAL_SCHEMA);
    let actual: ValidationResult = validate(&schema, &json!({"id": "s", "other": "s"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/other",
        "/additionalProperties/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

const ADDITIONAL_FALSE: &str = r#"{
  "properties": {"id": {}},
  "patternProperties": {"^x-": {}},
  "additionalProperties": false
}"#;

#[test]
fn additional_properties_false_with_pattern_properties_accepts_known() {
    let schema: JsonSchema = schema(ADDITIONAL_FALSE);
    let actual: ValidationResult = validate(&schema, &json!({"id": 1, "x-y": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn additional_properties_false_with_pattern_properties_rejects_each_other() {
    let schema: JsonSchema = schema(ADDITIONAL_FALSE);
    let actual: ValidationResult = validate(&schema, &json!({"id": 1, "w": 1, "x-y": 2, "z": 3}));
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::DisallowedAdditionalProperty {
                property: "w".to_string(),
            },
            ptr("/w"),
            kl("/additionalProperties"),
        ),
        ValidationError::new(
            ValidationErrorKind::DisallowedAdditionalProperty {
                property: "z".to_string(),
            },
            ptr("/z"),
            kl("/additionalProperties"),
        ),
    ]);
    assert_eq!(expected, actual);
}

// --- minProperties / maxProperties ---

#[test]
fn min_properties_rejects_too_few() {
    let schema: JsonSchema = schema(r#"{"minProperties":2}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooFewProperties {
            min_properties: 2,
            actual_count: 1,
        },
        root(),
        kl("/minProperties"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn min_properties_accepts_exact_count() {
    let schema: JsonSchema = schema(r#"{"minProperties":2}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_properties_zero_accepts_empty_object() {
    let schema: JsonSchema = schema(r#"{"minProperties":0}"#);
    let actual: ValidationResult = validate(&schema, &json!({}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn max_properties_rejects_too_many() {
    let schema: JsonSchema = schema(r#"{"maxProperties":1}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::TooManyProperties {
            max_properties: 1,
            actual_count: 2,
        },
        root(),
        kl("/maxProperties"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn max_properties_accepts_exact_count() {
    let schema: JsonSchema = schema(r#"{"maxProperties":1}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn min_max_properties_ignore_non_object() {
    let schema: JsonSchema = schema(r#"{"minProperties":2,"maxProperties":0}"#);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- $dynamicRef / $dynamicAnchor ---

const STRICT_TREE: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://example.com/strict-tree",
  "$dynamicAnchor": "node",
  "$ref": "tree",
  "unevaluatedProperties": false,
  "$defs": {
    "tree": {
      "$id": "https://example.com/tree",
      "$dynamicAnchor": "node",
      "type": "object",
      "properties": {
        "data": true,
        "children": {"type": "array", "items": {"$dynamicRef": "#node"}}
      }
    }
  }
}"##;

const GENERIC_TREE: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://example.com/tree",
  "$dynamicAnchor": "node",
  "type": "object",
  "properties": {
    "data": true,
    "children": {"type": "array", "items": {"$dynamicRef": "#node"}}
  }
}"##;

#[test]
fn dynamic_ref_strict_tree_accepts_known_members() {
    let schema: JsonSchema = schema(STRICT_TREE);
    let instance: Value = json!({"children": [{"data": 1, "children": [{"data": 2}]}], "data": 0});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

/// The nested node resolves `$dynamicRef` to the strict tree (the outermost `node` anchor), so
/// its misspelled member is unevaluated; `children` then fails, so the root's `$ref` drops its
/// annotations and `children` is unevaluated at the root too.
#[test]
fn dynamic_ref_strict_tree_rejects_nested_unknown_member() {
    let schema: JsonSchema = schema(STRICT_TREE);
    let instance: Value = json!({"children": [{"daat": 1}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![
        unevaluated(
            "/children/0/daat",
            "/$ref/properties/children/items/$dynamicRef/unevaluatedProperties",
            "daat",
        )
        .with_absolute_keyword_location("https://example.com/strict-tree#/unevaluatedProperties"),
        unevaluated("/children", "/unevaluatedProperties", "children"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn dynamic_ref_generic_tree_accepts_nested_unknown_member() {
    let schema: JsonSchema = schema(GENERIC_TREE);
    let instance: Value = json!({"children": [{"daat": 1}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn dynamic_ref_generic_tree_rejects_non_object_node() {
    let schema: JsonSchema = schema(GENERIC_TREE);
    let instance: Value = json!({"children": [{"children": [5]}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::ExpectedObject {
                got: "number".to_string(),
            },
            ptr("/children/0/children/0"),
            kl("/properties/children/items/$dynamicRef/properties/children/items/$dynamicRef/type"),
        )
        .with_absolute_keyword_location("https://example.com/tree#/type"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn dynamic_ref_to_json_pointer_acts_like_ref() {
    let schema: JsonSchema = schema(
        r##"{"$defs":{"int":{"type":"integer"}},"properties":{"a":{"$dynamicRef":"#/$defs/int"}}}"##,
    );
    let actual: ValidationResult = validate(&schema, &json!({"a": "x"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a",
        "/properties/a/$dynamicRef/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

/// The initial target declares `$anchor`, not `$dynamicAnchor`, so the outer resource's
/// `$dynamicAnchor` of the same name is not consulted: the reference stays static.
#[test]
fn dynamic_ref_to_plain_anchor_acts_like_ref() {
    let schema: JsonSchema = schema(
        r##"{
  "$id": "https://example.com/outer",
  "$dynamicAnchor": "n",
  "type": "string",
  "properties": {"a": {"$ref": "inner"}},
  "$defs": {
    "inner": {
      "$id": "https://example.com/inner",
      "$defs": {"int": {"$anchor": "n", "type": "integer"}},
      "$dynamicRef": "#n"
    }
  }
}"##,
    );
    let actual: ValidationResult = validate(&schema, &json!({"a": "x"}));
    let expected: ValidationResult = Err(vec![
        expected_string("", "/type", "object"),
        expected_integer("/a", "/properties/a/$ref/$dynamicRef/type", "string")
            .with_absolute_keyword_location("https://example.com/inner#/$defs/int/type"),
    ]);
    assert_eq!(expected, actual);
}

// --- $recursiveRef / $recursiveAnchor (2019-09) ---

const RECURSIVE_STRICT_TREE: &str = r##"{
  "$schema": "https://json-schema.org/draft/2019-09/schema",
  "$id": "https://example.com/strict-tree",
  "$recursiveAnchor": true,
  "$ref": "tree",
  "unevaluatedProperties": false,
  "$defs": {
    "tree": {
      "$id": "https://example.com/tree",
      "$recursiveAnchor": true,
      "type": "object",
      "properties": {
        "data": true,
        "children": {"type": "array", "items": {"$recursiveRef": "#"}}
      }
    }
  }
}"##;

const RECURSIVE_STRICT_TREE_UNANCHORED: &str = r##"{
  "$schema": "https://json-schema.org/draft/2019-09/schema",
  "$id": "https://example.com/strict-tree",
  "$recursiveAnchor": true,
  "$ref": "tree",
  "unevaluatedProperties": false,
  "$defs": {
    "tree": {
      "$id": "https://example.com/tree",
      "type": "object",
      "properties": {
        "data": true,
        "children": {"type": "array", "items": {"$recursiveRef": "#"}}
      }
    }
  }
}"##;

#[test]
fn recursive_ref_strict_tree_accepts_known_members() {
    let schema: JsonSchema = schema(RECURSIVE_STRICT_TREE);
    let instance: Value = json!({"children": [{"data": 1}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn recursive_ref_strict_tree_rejects_nested_unknown_member() {
    let schema: JsonSchema = schema(RECURSIVE_STRICT_TREE);
    let instance: Value = json!({"children": [{"daat": 1}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Err(vec![
        unevaluated(
            "/children/0/daat",
            "/$ref/properties/children/items/$recursiveRef/unevaluatedProperties",
            "daat",
        )
        .with_absolute_keyword_location("https://example.com/strict-tree#/unevaluatedProperties"),
        unevaluated("/children", "/unevaluatedProperties", "children"),
    ]);
    assert_eq!(expected, actual);
}

/// Without `$recursiveAnchor` on the target, `$recursiveRef` is a plain `$ref` to the generic
/// tree, so nested nodes are not strict.
#[test]
fn recursive_ref_without_anchor_acts_like_ref() {
    let schema: JsonSchema = schema(RECURSIVE_STRICT_TREE_UNANCHORED);
    let instance: Value = json!({"children": [{"daat": 1}]});
    let actual: ValidationResult = validate(&schema, &instance);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- unevaluatedProperties ---

#[test]
fn unevaluated_properties_false_rejects_unknown_member() {
    let schema: JsonSchema = schema(r#"{"properties":{"a":{}},"unevaluatedProperties":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 2}));
    let expected: ValidationResult = Err(vec![unevaluated("/b", "/unevaluatedProperties", "b")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_false_accepts_known_member() {
    let schema: JsonSchema = schema(r#"{"properties":{"a":{}},"unevaluatedProperties":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_schema_rejects_bad_member() {
    let schema: JsonSchema = schema(r#"{"unevaluatedProperties":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": "x"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a",
        "/unevaluatedProperties/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_schema_accepts_good_member() {
    let schema: JsonSchema = schema(r#"{"unevaluatedProperties":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_sees_through_all_of() {
    let schema: JsonSchema =
        schema(r#"{"allOf":[{"properties":{"a":{}}}],"unevaluatedProperties":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/b", "/unevaluatedProperties", "b")]);
    assert_eq!(expected, actual);
}

const UNEVALUATED_ANY_OF: &str = r#"{
  "anyOf": [{"properties": {"a": {"type": "integer"}}}, {"properties": {"b": {}}}],
  "unevaluatedProperties": false
}"#;

#[test]
fn unevaluated_properties_sees_every_passing_any_of_branch() {
    let schema: JsonSchema = schema(UNEVALUATED_ANY_OF);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_ignores_failing_any_of_branch() {
    let schema: JsonSchema = schema(UNEVALUATED_ANY_OF);
    let actual: ValidationResult = validate(&schema, &json!({"a": "x", "b": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/a", "/unevaluatedProperties", "a")]);
    assert_eq!(expected, actual);
}

const UNEVALUATED_ONE_OF: &str = r#"{
  "oneOf": [
    {"properties": {"a": {}}, "required": ["a"]},
    {"properties": {"b": {}}, "required": ["b"]}
  ],
  "unevaluatedProperties": false
}"#;

#[test]
fn unevaluated_properties_sees_through_one_of() {
    let schema: JsonSchema = schema(UNEVALUATED_ONE_OF);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_rejects_member_outside_one_of_match() {
    let schema: JsonSchema = schema(UNEVALUATED_ONE_OF);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "c": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/c", "/unevaluatedProperties", "c")]);
    assert_eq!(expected, actual);
}

const UNEVALUATED_IF: &str = r#"{
  "if": {"properties": {"kind": {"const": "x"}}, "required": ["kind"]},
  "then": {"properties": {"x": {}}},
  "else": {"properties": {"y": {}}},
  "unevaluatedProperties": false
}"#;

#[test]
fn unevaluated_properties_sees_through_if_and_then() {
    let schema: JsonSchema = schema(UNEVALUATED_IF);
    let actual: ValidationResult = validate(&schema, &json!({"kind": "x", "x": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_ignores_else_when_if_passes() {
    let schema: JsonSchema = schema(UNEVALUATED_IF);
    let actual: ValidationResult = validate(&schema, &json!({"kind": "x", "y": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/y", "/unevaluatedProperties", "y")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_sees_through_else() {
    let schema: JsonSchema = schema(UNEVALUATED_IF);
    let actual: ValidationResult = validate(&schema, &json!({"y": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_ignores_failing_if() {
    let schema: JsonSchema = schema(UNEVALUATED_IF);
    let actual: ValidationResult = validate(&schema, &json!({"kind": "z", "y": 1}));
    let expected: ValidationResult =
        Err(vec![unevaluated("/kind", "/unevaluatedProperties", "kind")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_sees_through_ref() {
    let schema: JsonSchema = schema(
        r##"{"$defs":{"base":{"properties":{"a":{}}}},"$ref":"#/$defs/base","unevaluatedProperties":false}"##,
    );
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/b", "/unevaluatedProperties", "b")]);
    assert_eq!(expected, actual);
}

const UNEVALUATED_DEPENDENT: &str = r#"{
  "properties": {"a": {}},
  "dependentSchemas": {"a": {"properties": {"b": {}}}},
  "unevaluatedProperties": false
}"#;

#[test]
fn unevaluated_properties_sees_through_dependent_schemas() {
    let schema: JsonSchema = schema(UNEVALUATED_DEPENDENT);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1, "b": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_ignores_untriggered_dependent_schema() {
    let schema: JsonSchema = schema(UNEVALUATED_DEPENDENT);
    let actual: ValidationResult = validate(&schema, &json!({"b": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/b", "/unevaluatedProperties", "b")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_does_not_see_through_not() {
    let schema: JsonSchema =
        schema(r#"{"not":{"not":{"properties":{"a":{}}}},"unevaluatedProperties":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/a", "/unevaluatedProperties", "a")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_sees_pattern_and_additional_properties() {
    let schema: JsonSchema = schema(
        r#"{"patternProperties":{"^p":{}},"additionalProperties":{"type":"integer"},"unevaluatedProperties":false}"#,
    );
    let actual: ValidationResult = validate(&schema, &json!({"other": 1, "pa": "s"}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_nested_reports_each_level() {
    let schema: JsonSchema = schema(
        r#"{
  "properties": {"inner": {"properties": {"x": {}}, "unevaluatedProperties": false}},
  "unevaluatedProperties": false
}"#,
    );
    let actual: ValidationResult = validate(&schema, &json!({"inner": {"x": 1, "y": 2}, "z": 3}));
    let expected: ValidationResult = Err(vec![
        unevaluated("/inner/y", "/properties/inner/unevaluatedProperties", "y"),
        unevaluated("/z", "/unevaluatedProperties", "z"),
    ]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_inner_does_not_see_outer_siblings() {
    let schema: JsonSchema =
        schema(r#"{"properties":{"a":{}},"allOf":[{"unevaluatedProperties":false}]}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![unevaluated(
        "/a",
        "/allOf/0/unevaluatedProperties",
        "a",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"unevaluatedProperties":false}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_properties_applies_in_2019_09() {
    let schema: JsonSchema = schema_in(
        r#"{"unevaluatedProperties":false}"#,
        SpecVersion::Draft201909,
    );
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    let expected: ValidationResult = Err(vec![unevaluated("/a", "/unevaluatedProperties", "a")]);
    assert_eq!(expected, actual);
}

// --- unevaluatedItems ---

#[test]
fn unevaluated_items_false_after_prefix_items_rejects_extra() {
    let schema: JsonSchema =
        schema(r#"{"prefixItems":[{"type":"integer"}],"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![false_schema("/1", "/unevaluatedItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_false_after_prefix_items_accepts_prefix() {
    let schema: JsonSchema =
        schema(r#"{"prefixItems":[{"type":"integer"}],"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_false_after_items_accepts_all() {
    let schema: JsonSchema = schema(r#"{"items":{"type":"integer"},"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_schema_rejects_bad_item() {
    let schema: JsonSchema = schema(r#"{"unevaluatedItems":{"type":"integer"}}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, "x"]));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/1",
        "/unevaluatedItems/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_sees_contains_in_2020_12() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"string"},"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a", "b"]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_rejects_non_contained_in_2020_12() {
    let schema: JsonSchema = schema(r#"{"contains":{"type":"string"},"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!(["a", 1]));
    let expected: ValidationResult = Err(vec![false_schema("/1", "/unevaluatedItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_ignores_contains_in_2019_09() {
    let schema: JsonSchema = schema_in(
        r#"{"contains":{"type":"string"},"unevaluatedItems":false}"#,
        SpecVersion::Draft201909,
    );
    let actual: ValidationResult = validate(&schema, &json!(["a"]));
    let expected: ValidationResult = Err(vec![false_schema("/0", "/unevaluatedItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_after_array_items_in_2019_09() {
    let schema: JsonSchema = schema_in(
        r#"{"items":[{"type":"integer"}],"unevaluatedItems":false}"#,
        SpecVersion::Draft201909,
    );
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![false_schema("/1", "/unevaluatedItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_sees_additional_items_in_2019_09() {
    let schema: JsonSchema = schema_in(
        r#"{"items":[{"type":"integer"}],"additionalItems":true,"unevaluatedItems":false}"#,
        SpecVersion::Draft201909,
    );
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_sees_through_all_of() {
    let schema: JsonSchema =
        schema(r#"{"allOf":[{"prefixItems":[true]}],"unevaluatedItems":false}"#);
    let actual: ValidationResult = validate(&schema, &json!([1, 2]));
    let expected: ValidationResult = Err(vec![false_schema("/1", "/unevaluatedItems")]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_items_ignored_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"unevaluatedItems":false}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!([1]));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

// --- format ---

#[test]
fn format_is_annotation_in_2020_12() {
    let schema: JsonSchema = schema(r#"{"format":"date"}"#);
    let actual: ValidationResult = validate(&schema, &json!("nope"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_is_annotation_in_2019_09() {
    let schema: JsonSchema = schema_in(r#"{"format":"date"}"#, SpecVersion::Draft201909);
    let actual: ValidationResult = validate(&schema, &json!("nope"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_is_asserted_in_draft_7() {
    let schema: JsonSchema = schema_in(r#"{"format":"date"}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!("nope"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "date".to_string(),
            value: "nope".to_string(),
        },
        root(),
        kl("/format"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn format_asserted_in_draft_7_accepts_valid_value() {
    let schema: JsonSchema = schema_in(r#"{"format":"date"}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!("2024-01-31"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_assertion_setting_asserts_in_2020_12() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .format_mode(FormatMode::Assertion)
        .build();
    let schema: JsonSchema = schema_with(r#"{"properties":{"d":{"format":"date"}}}"#, &settings);
    let actual: ValidationResult = validate(&schema, &json!({"d": "nope"}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "date".to_string(),
            value: "nope".to_string(),
        },
        ptr("/d"),
        kl("/properties/d/format"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn format_annotation_setting_ignores_in_draft_7() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .default_spec_version(SpecVersion::Draft07)
        .format_mode(FormatMode::Annotation)
        .build();
    let schema: JsonSchema = schema_with(r#"{"format":"date"}"#, &settings);
    let actual: ValidationResult = validate(&schema, &json!("nope"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn unknown_format_ignored_when_asserting() {
    let schema: JsonSchema = schema_in(r#"{"format":"no-such-format"}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!("anything"));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_ignores_non_string_when_asserting() {
    let schema: JsonSchema = schema_in(r#"{"format":"date"}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!(5));
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_in_raw_subschema_is_asserted_in_draft_7() {
    let schema: JsonSchema =
        schema_in(r#"{"not":{"not":{"format":"ipv4"}}}"#, SpecVersion::Draft07);
    let actual: ValidationResult = validate(&schema, &json!("999.1.1.1"));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NotSchemaMatched,
        root(),
        kl("/not"),
    )]);
    assert_eq!(expected, actual);
}

// --- Display of the validation errors ---

#[test]
fn display_invalid_format() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "date".to_string(),
            value: "nope".to_string(),
        },
        ptr("/d"),
        kl("/properties/d/format"),
    );
    let actual: String = error.to_string();
    let expected: String = "/d: string \"nope\" is not a valid date".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_not_schema_matched() {
    let error: ValidationError =
        ValidationError::new(ValidationErrorKind::NotSchemaMatched, root(), kl("/not"));
    let actual: String = error.to_string();
    let expected: String = "root: instance matches the schema of not".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_too_few_properties() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::TooFewProperties {
            min_properties: 2,
            actual_count: 1,
        },
        root(),
        kl("/minProperties"),
    );
    let actual: String = error.to_string();
    let expected: String = "root: object has 1 propert(ies), minimum is 2".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_too_many_properties() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::TooManyProperties {
            max_properties: 1,
            actual_count: 3,
        },
        ptr("/o"),
        kl("/properties/o/maxProperties"),
    );
    let actual: String = error.to_string();
    let expected: String = "/o: object has 3 propert(ies), maximum is 1".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_missing_dependent_property() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::MissingDependentProperty {
            property: "b".to_string(),
            required_by: "a".to_string(),
        },
        root(),
        kl("/dependentRequired"),
    );
    let actual: String = error.to_string();
    let expected: String =
        "root: missing property \"b\", required when \"a\" is present".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_invalid_property_name() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::InvalidPropertyName {
            property: "abcd".to_string(),
        },
        ptr("/abcd"),
        kl("/propertyNames"),
    );
    let actual: String = error.to_string();
    let expected: String = "/abcd: property name \"abcd\" does not match propertyNames".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_too_few_contains() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::TooFewContains {
            min_contains: 2,
            match_count: 1,
        },
        root(),
        kl("/minContains"),
    );
    let actual: String = error.to_string();
    let expected: String = "root: 1 item(s) match contains, minimum is 2".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_too_many_contains() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::TooManyContains {
            max_contains: 1,
            match_count: 2,
        },
        ptr("/list"),
        kl("/properties/list/maxContains"),
    );
    let actual: String = error.to_string();
    let expected: String = "/list: 2 item(s) match contains, maximum is 1".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_disallowed_unevaluated_property() {
    let error: ValidationError = unevaluated("/b", "/unevaluatedProperties", "b");
    let actual: String = error.to_string();
    let expected: String = "/b: unevaluated property \"b\" not allowed".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_ref_cycle() {
    let error: ValidationError = ref_cycle("", "/$ref", "#");
    let actual: String = error.to_string();
    let expected: String =
        "root: $ref \"#\" loops back to itself without consuming the instance".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_depth_limit_exceeded() {
    let error: ValidationError = ValidationError::new(
        ValidationErrorKind::DepthLimitExceeded { limit: 512 },
        ptr("/0/0"),
        kl("/items/items"),
    );
    let actual: String = error.to_string();
    let expected: String = "/0/0: evaluation nested deeper than 512 levels".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_escapes_instance_location() {
    let error: ValidationError = expected_integer(
        "/a~1b/m~0n",
        "/properties/a~1b/properties/m~0n/type",
        "string",
    );
    let actual: String = error.to_string();
    let expected: String = "/a~1b/m~0n: expected integer, got string".to_string();
    assert_eq!(expected, actual);
}

// --- Compile errors ---

/// The regular expression engine's reason for refusing `pattern`.
fn regex_reason(pattern: &str) -> String {
    regress::Regex::with_flags(pattern, "u")
        .expect_err("invalid pattern")
        .to_string()
}

#[test]
fn compile_rejects_invalid_pattern_with_location() {
    let schema: JsonSchema = schema(r#"{"properties":{"a":{"pattern":"("}}}"#);
    let actual: Result<(), CompileErrors> = compile_result(&schema);
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidPattern {
            location: "/properties/a/pattern".to_string(),
            pattern: "(".to_string(),
            reason: regex_reason("("),
        }));
    assert_eq!(expected, actual);
}

#[test]
fn compile_rejects_invalid_pattern_properties_key_with_location() {
    let schema: JsonSchema = schema(r#"{"properties":{"o":{"patternProperties":{"[":{}}}}}"#);
    let actual: Result<(), CompileErrors> = compile_result(&schema);
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidPattern {
            location: "/properties/o/patternProperties/[".to_string(),
            pattern: "[".to_string(),
            reason: regex_reason("["),
        }));
    assert_eq!(expected, actual);
}

#[test]
fn compile_reports_every_invalid_pattern() {
    let schema: JsonSchema = schema(r#"{"pattern":"(","properties":{"a":{"pattern":"["}}}"#);
    let actual: Result<(), CompileErrors> = compile_result(&schema);
    let expected: Result<(), CompileErrors> = Err(CompileErrors::from_vec(vec![
        CompileError::InvalidPattern {
            location: "/pattern".to_string(),
            pattern: "(".to_string(),
            reason: regex_reason("("),
        },
        CompileError::InvalidPattern {
            location: "/properties/a/pattern".to_string(),
            pattern: "[".to_string(),
            reason: regex_reason("["),
        },
    ])
    .expect("non-empty"));
    assert_eq!(expected, actual);
}

#[test]
fn compile_rejects_invalid_raw_keyword_value() {
    let schema: JsonSchema = schema(r#"{"not":{"minContains":"x"}}"#);
    let actual: Result<(), CompileErrors> = compile_result(&schema);
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidKeywordValue {
            location: "/not".to_string(),
            keyword: "minContains".to_string(),
            expected: "a non-negative integer",
        }));
    assert_eq!(expected, actual);
}

#[test]
fn strict_parse_accepts_invalid_raw_keyword_value() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .disallow_unknown_fields(true)
        .build();
    let actual: bool = Schema::new_from_str(r#"{"not":{"minContains":"x"}}"#, &settings).is_ok();
    let expected: bool = true;
    assert_eq!(expected, actual);
}

#[test]
fn display_compile_error_invalid_pattern_at_root() {
    let error: CompileError = CompileError::InvalidPattern {
        location: String::new(),
        pattern: "(".to_string(),
        reason: "unbalanced".to_string(),
    };
    let actual: String = error.to_string();
    let expected: String = "root: invalid regular expression \"(\": unbalanced".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_compile_error_invalid_keyword_value() {
    let error: CompileError = CompileError::InvalidKeywordValue {
        location: "/not".to_string(),
        keyword: "minContains".to_string(),
        expected: "a non-negative integer",
    };
    let actual: String = error.to_string();
    let expected: String =
        "/not: the value of \"minContains\" must be a non-negative integer".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn display_compile_error_unknown_ref() {
    let error: CompileError = CompileError::UnknownRef {
        location: "/properties/a".to_string(),
        ref_str: "other.json".to_string(),
    };
    let actual: String = error.to_string();
    let expected: String =
        "/properties/a: $ref \"other.json\" is not in the schema's registry".to_string();
    assert_eq!(expected, actual);
}

#[test]
fn validate_maps_invalid_pattern_to_invalid_pattern_in_schema() {
    let schema: JsonSchema = schema(r#"{"properties":{"a":{"pattern":"("}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a": "x"}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidPatternInSchema {
            pattern: "(".to_string(),
        },
        root(),
        kl("/properties/a/pattern"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn validate_maps_invalid_keyword_value_to_invalid_schema() {
    let schema: JsonSchema = schema(r#"{"not":{"minContains":"x"}}"#);
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidSchema {
            reason: "/not: the value of \"minContains\" must be a non-negative integer".to_string(),
        },
        root(),
        kl("/not/minContains"),
    )]);
    assert_eq!(expected, actual);
}

// --- is_valid agrees with validate ---

#[test]
fn is_valid_equals_validate_is_ok() {
    let cases: Vec<(&str, Value)> = vec![
        (r#"{"const":5,"minimum":10}"#, json!(5)),
        (r#"{"enum":[1,20],"minimum":10}"#, json!(20)),
        (ANY_OF_SELF, json!({"x": 1})),
        (ANY_OF_SELF, json!("hi")),
        (ONE_OF_SELF, json!("hi")),
        (r##"{"allOf":[{"$ref":"#"}]}"##, json!(1)),
        (r##"{"not":{"$ref":"#"}}"##, json!(1)),
        (TREE, json!({"children": [{"children": [{"name": 5}]}]})),
        (TREE, json!({"children": [{"children": [{"name": "x"}]}]})),
        (IF_THEN_ELSE, json!(5)),
        (IF_THEN_ELSE, json!("x")),
        (PREFIX_ITEMS, json!([1, "a", 3])),
        (
            r#"{"contains":{"type":"integer"},"maxContains":1}"#,
            json!([1, 2]),
        ),
        (STRICT_TREE, json!({"children": [{"daat": 1}]})),
        (STRICT_TREE, json!({"children": [{"data": 1}]})),
        (UNEVALUATED_ANY_OF, json!({"a": "x", "b": 1})),
        (UNEVALUATED_IF, json!({"kind": "x", "x": 1})),
        (ADDITIONAL_FALSE, json!({"id": 1, "z": 1})),
    ];
    let expected: Vec<bool> = cases
        .iter()
        .map(|(json, instance)| validate(&schema(json), instance).is_ok())
        .collect();
    let actual: Vec<bool> = cases
        .iter()
        .map(|(json, instance)| compile(&schema(json)).is_valid(instance))
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn one_of_three_matching_branches_reports_match_count() {
    let schema: JsonSchema =
        schema(r#"{"oneOf":[{"type":"integer"},{"minimum":0},{"multipleOf":1}]}"#);
    let actual: ValidationResult = validate(&schema, &json!(3));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MultipleSubschemasMatched {
            subschema_count: 3,
            match_count: 3,
        },
        root(),
        kl("/oneOf"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn one_of_three_matching_branches_is_not_valid() {
    let schema: JsonSchema =
        schema(r#"{"oneOf":[{"type":"integer"},{"minimum":0},{"multipleOf":1}]}"#);
    let actual: bool = compile(&schema).is_valid(&json!(3));
    let expected: bool = false;
    assert_eq!(expected, actual);
}

#[test]
fn compiled_schema_validates_many_instances() {
    let schema: JsonSchema = schema(r#"{"type":"integer","minimum":0}"#);
    let compiled: CompiledSchema = compile(&schema);
    let actual: Vec<ValidationResult> = vec![
        compiled.validate(&json!(1)),
        compiled.validate(&json!(-1)),
        compiled.validate(&json!("x")),
    ];
    let expected: Vec<ValidationResult> = vec![
        Ok(()),
        Err(vec![below_minimum("", "/minimum", 0, -1)]),
        Err(vec![expected_integer("", "/type", "string")]),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn boolean_root_schema_false_rejects() {
    let schema: Schema =
        Schema::new_from_str("false", &JsonSchemaSettings::default()).expect("parse schema");
    let actual: ValidationResult = validate(&schema, &json!(1));
    let expected: ValidationResult = Err(vec![false_schema("", "")]);
    assert_eq!(expected, actual);
}

// --- Instance paths with escaped keys ---

#[test]
fn instance_location_escapes_slash_in_key() {
    let schema: JsonSchema = schema(r#"{"properties":{"a/b":{"type":"integer"}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a/b": "x"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a~1b",
        "/properties/a~1b/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn instance_location_escapes_tilde_in_key() {
    let schema: JsonSchema = schema(r#"{"properties":{"m~n":{"type":"integer"}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"m~n": "x"}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/m~0n",
        "/properties/m~0n/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn instance_location_escapes_nested_keys() {
    let schema: JsonSchema =
        schema(r#"{"properties":{"a/b":{"properties":{"m~n":{"items":{"type":"integer"}}}}}}"#);
    let actual: ValidationResult = validate(&schema, &json!({"a/b": {"m~n": [1, "x"]}}));
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a~1b/m~0n/1",
        "/properties/a~1b/properties/m~0n/items/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn missing_required_path_escapes_key() {
    let schema: JsonSchema = schema(r#"{"required":["a/b"]}"#);
    let actual: ValidationResult = validate(&schema, &json!({}));
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MissingRequired {
            property: "a/b".to_string(),
        },
        root(),
        kl("/required"),
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn unevaluated_property_path_escapes_key() {
    let schema: JsonSchema = schema(r#"{"unevaluatedProperties":false}"#);
    let actual: ValidationResult = validate(&schema, &json!({"m~n": 1}));
    let expected: ValidationResult =
        Err(vec![unevaluated("/m~0n", "/unevaluatedProperties", "m~n")]);
    assert_eq!(expected, actual);
}

#[test]
fn escaped_pointer_segments_round_trip() {
    let actual: JsonPointer = JsonPointer::from(vec!["a/b".to_string(), "m~n".to_string()]);
    let expected: JsonPointer = ptr("/a~1b/m~0n");
    assert_eq!(expected, actual);
}

// --- Review fixes: dynamic re-entry, unknown formats, raw keyword shapes ---

/// A `$ref` re-entered at the same instance after the dynamic scope gained a resource declaring
/// the anchor resolves `$dynamicRef` elsewhere, so it is not a loop: the second visit passes.
#[test]
fn ref_reentry_with_grown_dynamic_scope_is_not_a_cycle() {
    let schema: JsonSchema = schema(
        r#"{"$id":"https://ex.com/R","$ref":"T","$defs":{
            "T":{"$id":"https://ex.com/T","if":{"$dynamicRef":"Y#y"},"else":{"$ref":"W"}},
            "Y":{"$id":"https://ex.com/Y","$dynamicAnchor":"y","not":{}},
            "W":{"$id":"https://ex.com/W","$ref":"T","$defs":{"a":{"$dynamicAnchor":"y","type":"integer"}}}}}"#,
    );
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compile(&schema).validate(&json!(1));
    assert_eq!(expected, actual);
}

/// The same chain still ends in `RefCycle` when the dynamic scope gains nothing new.
#[test]
fn ref_reentry_with_unchanged_dynamic_scope_is_a_cycle() {
    let schema: JsonSchema = schema(
        r#"{"$id":"https://ex.com/R","$ref":"T","$defs":{
            "T":{"$id":"https://ex.com/T","if":{"$dynamicRef":"Y#y"},"else":{"$ref":"W"}},
            "Y":{"$id":"https://ex.com/Y","$dynamicAnchor":"y","not":{}},
            "W":{"$id":"https://ex.com/W","$ref":"T","$defs":{"a":{"$dynamicAnchor":"y","not":{}}}}}}"#,
    );
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::RefCycle {
                ref_str: "T".to_string(),
            },
            root(),
            kl("/$ref/else/$ref/$ref/else/$ref/$ref"),
        )
        .with_absolute_keyword_location("https://ex.com/W#/$ref"),
    ]);
    let actual: ValidationResult = compile(&schema).validate(&json!(1));
    assert_eq!(expected, actual);
}

/// The 2020-12 `format-assertion` vocabulary requires failing on an unknown format.
#[test]
fn unknown_format_under_format_assertion_vocabulary_fails_to_compile() {
    let meta: Value = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://ex.com/meta",
        "$vocabulary": {
            "https://json-schema.org/draft/2020-12/vocab/core": true,
            "https://json-schema.org/draft/2020-12/vocab/format-assertion": true
        }
    });
    let schema: JsonSchema =
        schema(r#"{"$schema":"https://ex.com/meta","format":"not-a-real-format"}"#);
    let registry: Registry<'_> = Registry::builder()
        .document("https://ex.com/meta", meta)
        .build(&schema)
        .expect("build registry");
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::UnknownFormat {
            location: String::new(),
            format: "not-a-real-format".to_string(),
        }));
    let actual: Result<(), CompileErrors> = CompiledSchema::new(&registry).map(|_| ());
    assert_eq!(expected, actual);
}

/// Outside that vocabulary an unknown format is ignored, even when formats are asserted.
#[test]
fn unknown_format_is_ignored_when_asserting() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .format_mode(FormatMode::Assertion)
        .build();
    let schema: JsonSchema = schema_with(r#"{"format":"not-a-real-format"}"#, &settings);
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compile(&schema).validate(&json!("x"));
    assert_eq!(expected, actual);
}

#[test]
fn unknown_format_error_display() {
    let error: CompileError = CompileError::UnknownFormat {
        location: "/not".to_string(),
        format: "colour".to_string(),
    };
    let expected: String =
        "/not: unknown format \"colour\" under the format-assertion vocabulary".to_string();
    let actual: String = error.to_string();
    assert_eq!(expected, actual);
}

/// The compile errors for a raw keyword with a value of the wrong shape, by keyword.
fn raw_shape_error(schema_json: &str, keyword: &str, expected: &'static str) {
    let expected_result: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidKeywordValue {
            location: "/not".to_string(),
            keyword: keyword.to_string(),
            expected,
        }));
    let actual: Result<(), CompileErrors> = compile_result(&schema(schema_json));
    assert_eq!(expected_result, actual);
}

#[test]
fn raw_required_with_non_string_entry_fails_to_compile() {
    raw_shape_error(
        r#"{"not":{"required":["a",1]}}"#,
        "required",
        "an array of strings",
    );
}

#[test]
fn raw_unique_items_non_boolean_fails_to_compile() {
    raw_shape_error(
        r#"{"not":{"uniqueItems":"yes"}}"#,
        "uniqueItems",
        "a boolean",
    );
}

#[test]
fn raw_pattern_non_string_fails_to_compile() {
    raw_shape_error(r#"{"not":{"pattern":5}}"#, "pattern", "a string");
}

#[test]
fn raw_ref_non_string_fails_to_compile() {
    raw_shape_error(r#"{"not":{"$ref":5}}"#, "$ref", "a string");
}

#[test]
fn raw_format_non_string_fails_to_compile() {
    raw_shape_error(r#"{"not":{"format":5}}"#, "format", "a string");
}

#[test]
fn raw_dependencies_non_object_fails_to_compile() {
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidKeywordValue {
            location: "/not".to_string(),
            keyword: "dependencies".to_string(),
            expected: "an object whose values are schemas or arrays of strings",
        }));
    let actual: Result<(), CompileErrors> = compile_result(&schema_in(
        r#"{"not":{"dependencies":5}}"#,
        SpecVersion::Draft07,
    ));
    assert_eq!(expected, actual);
}

#[test]
fn raw_dependencies_array_with_non_string_fails_to_compile() {
    let expected: Result<(), CompileErrors> =
        Err(CompileErrors::new(CompileError::InvalidKeywordValue {
            location: "/not".to_string(),
            keyword: "dependencies".to_string(),
            expected: "an object whose values are schemas or arrays of strings",
        }));
    let actual: Result<(), CompileErrors> = compile_result(&schema_in(
        r#"{"not":{"dependencies":{"a":["b",1]}}}"#,
        SpecVersion::Draft07,
    ));
    assert_eq!(expected, actual);
}

// --- Sorted-map fast paths: `properties`, `required`, `additionalProperties: false` ---

/// `count` names `p00`, `p01`, ... each mapped to `{"type": "integer"}`.
fn integer_properties(count: usize) -> String {
    let members: Vec<String> = (0..count)
        .map(|i| format!(r#""p{i:02}":{{"type":"integer"}}"#))
        .collect();
    format!(r#"{{"properties":{{{}}}}}"#, members.join(","))
}

/// `count` names `p00`, `p01`, ... each mapped to `true`, with `additionalProperties: false`.
fn closed_properties(count: usize) -> String {
    let members: Vec<String> = (0..count).map(|i| format!(r#""p{i:02}":true"#)).collect();
    format!(
        r#"{{"properties":{{{}}},"additionalProperties":false}}"#,
        members.join(",")
    )
}

/// Validates `instance` against `schema` with the sorted-map fast paths turned off, as when
/// `serde_json`'s `preserve_order` keeps maps in insertion order.
fn validate_unsorted(schema: &JsonSchema, instance: &Value) -> ValidationResult {
    let mut compiled: CompiledSchema = compile(schema);
    compiled.sorted_maps = false;
    compiled.validate(instance)
}

fn missing_required(property: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::MissingRequired {
            property: property.to_string(),
        },
        root(),
        ptr("/required"),
    )
}

fn disallowed_additional(property: &str) -> ValidationError {
    ValidationError::new(
        ValidationErrorKind::DisallowedAdditionalProperty {
            property: property.to_string(),
        },
        root().push(property),
        ptr("/additionalProperties"),
    )
}

#[test]
fn properties_walk_with_sorted_keys_checks_named_members() {
    let s: JsonSchema = schema(&integer_properties(4));
    let instance: Value = json!({"p00": 1, "p01": "x", "p02": 2, "q": "y", "p03": "z"});
    let expected: ValidationResult = Err(vec![
        expected_integer("/p01", "/properties/p01/type", "string"),
        expected_integer("/p03", "/properties/p03/type", "string"),
    ]);
    let actual: ValidationResult = compile(&s).validate(&instance);
    assert_eq!(expected, actual);
}

#[test]
fn properties_search_when_object_is_much_smaller_than_list() {
    let s: JsonSchema = schema(&integer_properties(20));
    let instance: Value = json!({"a": "x", "p03": "y", "p17": "z"});
    let expected: ValidationResult = Err(vec![
        expected_integer("/p03", "/properties/p03/type", "string"),
        expected_integer("/p17", "/properties/p17/type", "string"),
    ]);
    let actual: ValidationResult = compile(&s).validate(&instance);
    assert_eq!(expected, actual);
}

#[test]
fn properties_lookup_when_object_is_much_larger_than_list() {
    let s: JsonSchema = schema(&integer_properties(1));
    let instance: Value =
        json!({"a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "p00": "x", "z": 7});
    let expected: ValidationResult = Err(vec![expected_integer(
        "/p00",
        "/properties/p00/type",
        "string",
    )]);
    let actual: ValidationResult = compile(&s).validate(&instance);
    assert_eq!(expected, actual);
}

#[test]
fn properties_unsorted_maps_give_the_same_errors() {
    let s: JsonSchema = schema(&integer_properties(4));
    let instance: Value = json!({"p00": 1, "p01": "x", "p02": 2, "q": "y", "p03": "z"});
    let expected: ValidationResult = compile(&s).validate(&instance);
    let actual: ValidationResult = validate_unsorted(&s, &instance);
    assert_eq!(expected, actual);
}

#[test]
fn properties_walk_annotates_member_positions() {
    let s: JsonSchema =
        schema(r#"{"properties":{"a":true,"c":true,"e":true},"unevaluatedProperties":false}"#);
    let instance: Value = json!({"a": 1, "b": 2, "c": 3, "d": 4, "e": 5});
    let expected: ValidationResult = Err(vec![
        unevaluated("/b", "/unevaluatedProperties", "b"),
        unevaluated("/d", "/unevaluatedProperties", "d"),
    ]);
    let actual: ValidationResult = compile(&s).validate(&instance);
    assert_eq!(expected, actual);
}

#[test]
fn properties_search_annotates_member_positions() {
    let s: JsonSchema = schema(
        r#"{"properties":{"p0":true,"p1":true,"p2":true,"p3":true,"p4":true,"p5":true,
            "p6":true,"p7":true,"p8":true,"p9":true},"unevaluatedProperties":false}"#,
    );
    let instance: Value = json!({"a": 1, "p5": 2});
    let expected: ValidationResult = Err(vec![unevaluated("/a", "/unevaluatedProperties", "a")]);
    let actual: ValidationResult = compile(&s).validate(&instance);
    assert_eq!(expected, actual);
}

#[test]
fn required_all_present_passes() {
    let s: JsonSchema = schema(r#"{"required":["c","a","b","a"]}"#);
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 1, "b": 2, "c": 3, "d": 4}));
    assert_eq!(expected, actual);
}

#[test]
fn required_more_names_than_members_reports_in_schema_order() {
    let s: JsonSchema = schema(r#"{"required":["b","a","b","z"]}"#);
    let expected: ValidationResult = Err(vec![
        missing_required("b"),
        missing_required("b"),
        missing_required("z"),
    ]);
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 1}));
    assert_eq!(expected, actual);
}

#[test]
fn required_sorted_walk_finds_missing_name() {
    let s: JsonSchema = schema(r#"{"required":["c","a"]}"#);
    let expected: ValidationResult = Err(vec![missing_required("c")]);
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 1, "b": 2, "d": 3}));
    assert_eq!(expected, actual);
}

#[test]
fn required_lookup_when_object_is_much_larger_than_list() {
    let s: JsonSchema = schema(r#"{"required":["a"]}"#);
    let expected: ValidationResult = Err(vec![missing_required("a")]);
    let actual: ValidationResult =
        compile(&s).validate(&json!({"b": 1, "c": 2, "d": 3, "e": 4, "f": 5, "g": 6}));
    assert_eq!(expected, actual);
}

#[test]
fn required_verdicts_agree_with_and_without_sorted_maps() {
    let s: JsonSchema = schema(r#"{"required":["c","a"]}"#);
    let instances: [Value; 4] = [
        json!({"a": 1, "c": 2}),
        json!({"a": 1, "b": 2, "d": 3}),
        json!({"a": 1}),
        json!({"a": 1, "b": 1, "c": 1, "d": 1, "e": 1, "f": 1, "g": 1, "h": 1, "i": 1}),
    ];
    let sorted: CompiledSchema = compile(&s);
    let mut unsorted: CompiledSchema = compile(&s);
    unsorted.sorted_maps = false;
    let expected: (Vec<bool>, Vec<bool>) = (
        vec![true, false, false, true],
        vec![true, false, false, true],
    );
    let actual: (Vec<bool>, Vec<bool>) = (
        instances.iter().map(|i| sorted.is_valid(i)).collect(),
        instances.iter().map(|i| unsorted.is_valid(i)).collect(),
    );
    assert_eq!(expected, actual);
}

#[test]
fn required_unsorted_maps_give_the_same_errors() {
    let s: JsonSchema = schema(r#"{"required":["b","a","b","z"]}"#);
    let instance: Value = json!({"a": 1});
    let expected: ValidationResult = compile(&s).validate(&instance);
    let actual: ValidationResult = validate_unsorted(&s, &instance);
    assert_eq!(expected, actual);
}

#[test]
fn no_additional_properties_sorted_walk_passes_known_members() {
    let s: JsonSchema = schema(&closed_properties(3));
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compile(&s).validate(&json!({"p00": 1, "p02": 2}));
    assert_eq!(expected, actual);
}

#[test]
fn no_additional_properties_sorted_walk_reports_unknown_members() {
    let s: JsonSchema = schema(&closed_properties(3));
    let expected: ValidationResult = Err(vec![
        disallowed_additional("a"),
        disallowed_additional("p01x"),
    ]);
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 0, "p00": 1, "p01x": 2}));
    assert_eq!(expected, actual);
}

#[test]
fn no_additional_properties_lookup_when_object_is_much_smaller_than_list() {
    let s: JsonSchema = schema(&closed_properties(20));
    let expected: ValidationResult = Err(vec![disallowed_additional("zz")]);
    let actual: ValidationResult = compile(&s).validate(&json!({"p03": 1, "zz": 2}));
    assert_eq!(expected, actual);
}

#[test]
fn no_additional_properties_without_properties_rejects_every_member() {
    let s: JsonSchema = schema(r#"{"additionalProperties":false}"#);
    let expected: ValidationResult = Err(vec![disallowed_additional("a")]);
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 1}));
    assert_eq!(expected, actual);
}

#[test]
fn no_additional_properties_unsorted_maps_give_the_same_errors() {
    let s: JsonSchema = schema(&closed_properties(20));
    let instance: Value = json!({"a": 0, "p03": 1, "zz": 2});
    let expected: ValidationResult = compile(&s).validate(&instance);
    let actual: ValidationResult = validate_unsorted(&s, &instance);
    assert_eq!(expected, actual);
}

// --- Fast paths of string keywords ---

#[test]
fn min_length_decided_from_byte_length_and_by_counting() {
    let s: JsonSchema = schema(r#"{"minLength":2}"#);
    let compiled: CompiledSchema = compile(&s);
    let expected: Vec<ValidationResult> = vec![
        Ok(()),
        Ok(()),
        Err(vec![ValidationError::new(
            ValidationErrorKind::TooShort {
                min_length: 2,
                actual_length: 1,
            },
            root(),
            kl("/minLength"),
        )]),
    ];
    let actual: Vec<ValidationResult> = vec![
        compiled.validate(&json!("abcdefgh")),
        compiled.validate(&json!("éé")),
        compiled.validate(&json!("😀")),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn simple_pattern_and_regex_fallback_report_alike() {
    let s: JsonSchema = schema(r#"{"items":{"pattern":"^[a-z]+$"}}"#);
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::PatternMismatch {
                pattern: "^[a-z]+$".to_string(),
                value: "ab1".to_string(),
            },
            ptr("/1"),
            kl("/items/pattern"),
        ),
        ValidationError::new(
            ValidationErrorKind::PatternMismatch {
                pattern: "^[a-z]+$".to_string(),
                value: "abé".to_string(),
            },
            ptr("/3"),
            kl("/items/pattern"),
        ),
    ]);
    let actual: ValidationResult = compile(&s).validate(&json!(["abc", "ab1", "ab", "abé"]));
    assert_eq!(expected, actual);
}

#[test]
fn property_names_reuses_its_buffer_for_each_name() {
    let s: JsonSchema = schema(r#"{"propertyNames":{"maxLength":2}}"#);
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::InvalidPropertyName {
                property: "abc".to_string(),
            },
            ptr("/abc"),
            kl("/propertyNames"),
        ),
        ValidationError::new(
            ValidationErrorKind::InvalidPropertyName {
                property: "long".to_string(),
            },
            ptr("/long"),
            kl("/propertyNames"),
        ),
    ]);
    let actual: ValidationResult =
        compile(&s).validate(&json!({"abc": 1, "ab": 2, "long": 3, "x": 4}));
    assert_eq!(expected, actual);
}

#[test]
fn shared_enum_list_reports_each_schema_alike() {
    let s: JsonSchema = schema(r#"{"properties":{"a":{"enum":["x",1]},"b":{"enum":["x",1]}}}"#);
    let not_in_enum = |path: &str| {
        ValidationError::new(
            ValidationErrorKind::NotInEnum {
                invalid_value: "2".to_string(),
                allowed: vec!["\"x\"".to_string(), "1".to_string()],
            },
            ptr(path),
            kl(&format!("/properties{path}/enum")),
        )
    };
    let expected: ValidationResult = Err(vec![not_in_enum("/a"), not_in_enum("/b")]);
    let actual: ValidationResult = compile(&s).validate(&json!({"a": 2, "b": 2}));
    assert_eq!(expected, actual);
}

// --- ECMA-262 patterns the `u` grammar refuses ---

/// Parses the schema `value` with the default settings.
fn schema_value(value: &Value) -> JsonSchema {
    schema(&value.to_string())
}

/// `{"pattern": pattern}` validating the string `text`.
fn pattern_result(pattern: &str, text: &str) -> ValidationResult {
    validate(&schema_value(&json!({ "pattern": pattern })), &json!(text))
}

/// `{"patternProperties": {pattern: {"type": "integer"}}}` validating `{key: "x"}`.
fn pattern_properties_result(pattern: &str, key: &str) -> ValidationResult {
    let schema: JsonSchema =
        schema_value(&json!({ "patternProperties": { pattern: { "type": "integer" } } }));
    validate(&schema, &json!({ key: "x" }))
}

/// A draft 7 `{"format": "regex"}` (asserted by default) validating the string `value`.
fn format_regex_result(value: &str) -> ValidationResult {
    validate(
        &schema_in(r#"{"format":"regex"}"#, SpecVersion::Draft07),
        &json!(value),
    )
}

/// The error of a `pattern` (`keyword_location` `/pattern`) or `patternProperties` key that does
/// not compile.
fn invalid_pattern_in_schema(pattern: &str, keyword_location: JsonPointer) -> ValidationResult {
    Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidPatternInSchema {
            pattern: pattern.to_string(),
        },
        root(),
        keyword_location,
    )])
}

/// The keyword location of the `patternProperties` key `pattern`.
fn pattern_properties_key(pattern: &str) -> JsonPointer {
    root().push("patternProperties").push(pattern)
}

fn invalid_regex_format(value: &str) -> ValidationResult {
    Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "regex".to_string(),
            value: value.to_string(),
        },
        root(),
        kl("/format"),
    )])
}

#[test]
fn pattern_accepts_escaped_hyphen() {
    let actual: ValidationResult = pattern_result(r"^a\-b$", "a-b");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_accepts_escaped_colon() {
    let actual: ValidationResult = pattern_result(r"\:", "x:y");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_accepts_escaped_quote() {
    let actual: ValidationResult = pattern_result(r#"\""#, r#"say "hi""#);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_accepts_lone_brace() {
    let actual: ValidationResult = pattern_result("^a{$", "a{");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_accepts_lone_bracket() {
    let actual: ValidationResult = pattern_result("^]$", "]");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn pattern_rejects_escaped_underscore() {
    let actual: ValidationResult = pattern_result(r"a\_b", "a_b");
    let expected: ValidationResult = invalid_pattern_in_schema(r"a\_b", ptr("/pattern"));
    assert_eq!(expected, actual);
}

#[test]
fn pattern_rejects_escaped_letter_a() {
    let actual: ValidationResult = pattern_result(r"\a", "a");
    let expected: ValidationResult = invalid_pattern_in_schema(r"\a", ptr("/pattern"));
    assert_eq!(expected, actual);
}

#[test]
fn pattern_rejects_escaped_letter_e() {
    let actual: ValidationResult = pattern_result(r"\e", "e");
    let expected: ValidationResult = invalid_pattern_in_schema(r"\e", ptr("/pattern"));
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_accepts_escaped_hyphen() {
    let actual: ValidationResult = pattern_properties_result(r"^a\-b$", "a-b");
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a-b",
        "/patternProperties/^a\\-b$/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_accepts_escaped_colon() {
    let actual: ValidationResult = pattern_properties_result(r"\:", "x:y");
    let expected: ValidationResult = Err(vec![expected_integer(
        "/x:y",
        "/patternProperties/\\:/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_accepts_escaped_quote() {
    let actual: ValidationResult = pattern_properties_result(r#"\""#, r#"say "hi""#);
    let expected: ValidationResult = Err(vec![expected_integer(
        r#"/say "hi""#,
        "/patternProperties/\\\"/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_accepts_lone_brace() {
    let actual: ValidationResult = pattern_properties_result("^a{$", "a{");
    let expected: ValidationResult = Err(vec![expected_integer(
        "/a{",
        "/patternProperties/^a{$/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_accepts_lone_bracket() {
    let actual: ValidationResult = pattern_properties_result("^]$", "]");
    let expected: ValidationResult = Err(vec![expected_integer(
        "/]",
        "/patternProperties/^]$/type",
        "string",
    )]);
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_rejects_escaped_underscore() {
    let actual: ValidationResult = pattern_properties_result(r"a\_b", "a_b");
    let expected: ValidationResult =
        invalid_pattern_in_schema(r"a\_b", pattern_properties_key(r"a\_b"));
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_rejects_escaped_letter_a() {
    let actual: ValidationResult = pattern_properties_result(r"\a", "a");
    let expected: ValidationResult =
        invalid_pattern_in_schema(r"\a", pattern_properties_key(r"\a"));
    assert_eq!(expected, actual);
}

#[test]
fn pattern_properties_rejects_escaped_letter_e() {
    let actual: ValidationResult = pattern_properties_result(r"\e", "e");
    let expected: ValidationResult =
        invalid_pattern_in_schema(r"\e", pattern_properties_key(r"\e"));
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_accepts_escaped_hyphen() {
    let actual: ValidationResult = format_regex_result(r"^a\-b$");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_accepts_escaped_colon() {
    let actual: ValidationResult = format_regex_result(r"\:");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_accepts_escaped_quote() {
    let actual: ValidationResult = format_regex_result(r#"\""#);
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_accepts_lone_brace() {
    let actual: ValidationResult = format_regex_result("^a{$");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_accepts_lone_bracket() {
    let actual: ValidationResult = format_regex_result("^]$");
    let expected: ValidationResult = Ok(());
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_rejects_escaped_underscore() {
    let actual: ValidationResult = format_regex_result(r"a\_b");
    let expected: ValidationResult = invalid_regex_format(r"a\_b");
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_rejects_escaped_letter_a() {
    let actual: ValidationResult = format_regex_result(r"\a");
    let expected: ValidationResult = invalid_regex_format(r"\a");
    assert_eq!(expected, actual);
}

#[test]
fn format_regex_rejects_escaped_letter_e() {
    let actual: ValidationResult = format_regex_result(r"\e");
    let expected: ValidationResult = invalid_regex_format(r"\e");
    assert_eq!(expected, actual);
}

// --- User-defined formats ---

/// Compiles `json` (formats asserted when `assert` is set) with the user format `even-length`
/// (a string of an even number of characters).
fn compile_with_even_length(json: &str, assert: bool) -> CompiledSchema {
    let settings: JsonSchemaSettings = if assert {
        JsonSchemaSettings::builder()
            .format_mode(FormatMode::Assertion)
            .build()
    } else {
        JsonSchemaSettings::default()
    };
    let schema: JsonSchema = schema_with(json, &settings);
    let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
    CompiledSchema::builder()
        .format("even-length", |s: &str| s.chars().count().is_multiple_of(2))
        .build(&registry)
        .expect("compile")
}

#[test]
fn user_format_rejects_non_member_when_asserted() {
    let compiled: CompiledSchema =
        compile_with_even_length(r#"{"properties":{"a":{"format":"even-length"}}}"#, true);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "even-length".to_string(),
            value: "abc".to_string(),
        },
        ptr("/a"),
        kl("/properties/a/format"),
    )]);
    let actual: ValidationResult = compiled.validate(&json!({"a": "abc"}));
    assert_eq!(expected, actual);
}

#[test]
fn user_format_accepts_member_when_asserted() {
    let compiled: CompiledSchema = compile_with_even_length(r#"{"format":"even-length"}"#, true);
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compiled.validate(&json!("ab"));
    assert_eq!(expected, actual);
}

#[test]
fn user_format_ignores_non_string() {
    let compiled: CompiledSchema = compile_with_even_length(r#"{"format":"even-length"}"#, true);
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compiled.validate(&json!(123));
    assert_eq!(expected, actual);
}

/// 2020-12 records formats as annotations by default, so a user format is not checked.
#[test]
fn user_format_is_not_checked_when_formats_are_annotations() {
    let compiled: CompiledSchema = compile_with_even_length(r#"{"format":"even-length"}"#, false);
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compiled.validate(&json!("abc"));
    assert_eq!(expected, actual);
}

#[test]
fn user_format_replaces_builtin_format_of_same_name() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .format_mode(FormatMode::Assertion)
        .build();
    let schema: JsonSchema = schema_with(r#"{"format":"email"}"#, &settings);
    let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
    let compiled: CompiledSchema = CompiledSchema::builder()
        .format("email", |_: &str| true)
        .build(&registry)
        .expect("compile");
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compiled.validate(&json!("not an email"));
    assert_eq!(expected, actual);
}

#[test]
fn user_format_registered_twice_keeps_last_check() {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .format_mode(FormatMode::Assertion)
        .build();
    let schema: JsonSchema = schema_with(r#"{"format":"x"}"#, &settings);
    let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
    let compiled: CompiledSchema = CompiledSchema::builder()
        .format("x", |_: &str| false)
        .format("x", |_: &str| true)
        .build(&registry)
        .expect("compile");
    let expected: ValidationResult = Ok(());
    let actual: ValidationResult = compiled.validate(&json!("anything"));
    assert_eq!(expected, actual);
}

/// The `format-assertion` vocabulary rejects unknown formats, but a user format is known.
#[test]
fn user_format_is_known_under_format_assertion_vocabulary() {
    let meta: Value = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://ex.com/meta",
        "$vocabulary": {
            "https://json-schema.org/draft/2020-12/vocab/core": true,
            "https://json-schema.org/draft/2020-12/vocab/format-assertion": true
        }
    });
    let schema: JsonSchema = schema(r#"{"$schema":"https://ex.com/meta","format":"even-length"}"#);
    let registry: Registry<'_> = Registry::builder()
        .document("https://ex.com/meta", meta)
        .build(&schema)
        .expect("build registry");
    let compiled: CompiledSchema = CompiledSchema::builder()
        .format("even-length", |s: &str| s.chars().count().is_multiple_of(2))
        .build(&registry)
        .expect("compile");
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::InvalidFormat {
            format: "even-length".to_string(),
            value: "abc".to_string(),
        },
        root(),
        kl("/format"),
    )]);
    let actual: ValidationResult = compiled.validate(&json!("abc"));
    assert_eq!(expected, actual);
}

// --- Keyword locations ---

#[test]
fn absolute_location_is_omitted_without_an_absolute_id() {
    let schema: JsonSchema = schema(r##"{"$ref":"#/$defs/a","$defs":{"a":{"minimum":2}}}"##);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::BelowMinimum {
            minimum: num(2),
            actual: num(1),
        },
        root(),
        kl("/$ref/minimum"),
    )]);
    let actual: ValidationResult = validate(&schema, &json!(1));
    assert_eq!(expected, actual);
}

#[test]
fn absolute_location_uses_the_target_resource_id() {
    let schema: JsonSchema = schema(
        r#"{"$id":"https://ex.com/root","$ref":"item","$defs":{"item":{"$id":"item","minimum":2}}}"#,
    );
    let expected: ValidationResult = Err(vec![
        ValidationError::new(
            ValidationErrorKind::BelowMinimum {
                minimum: num(2),
                actual: num(1),
            },
            root(),
            kl("/$ref/minimum"),
        )
        .with_absolute_keyword_location("https://ex.com/item#/minimum"),
    ]);
    let actual: ValidationResult = validate(&schema, &json!(1));
    assert_eq!(expected, actual);
}

#[test]
fn one_of_without_match_reports_causes() {
    let schema: JsonSchema = schema(r#"{"oneOf":[{"type":"string"},{"minimum":5}]}"#);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::NoSubschemaMatched {
            subschema_count: 2,
            causes: vec![
                expected_string("", "/oneOf/0/type", "number"),
                below_minimum("", "/oneOf/1/minimum", 5, 1),
            ],
        },
        root(),
        kl("/oneOf"),
    )]);
    let actual: ValidationResult = validate(&schema, &json!(1));
    assert_eq!(expected, actual);
}

#[test]
fn dependencies_schema_form_location_names_dependencies_in_2020_12() {
    let schema: JsonSchema = schema(r#"{"dependencies":{"a":{"required":["b"]}}}"#);
    let expected: ValidationResult = Err(vec![ValidationError::new(
        ValidationErrorKind::MissingRequired {
            property: "b".to_string(),
        },
        root(),
        kl("/dependencies/a/required"),
    )]);
    let actual: ValidationResult = validate(&schema, &json!({"a": 1}));
    assert_eq!(expected, actual);
}
