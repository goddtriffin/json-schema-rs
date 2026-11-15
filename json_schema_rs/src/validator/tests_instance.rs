//! Validating instances held in a representation other than `serde_json::Value`: a small tree
//! type whose objects keep their members in the order given (unsorted), checked against the
//! same instance as a `Value`.

use super::{
    Annotation, CompiledSchema, Instance, InstanceObject, InstanceRef, Output, OutputFormat,
    ValidationError,
};
use crate::json_schema::{JsonSchemaSettings, Registry, Schema};
use crate::number::json_equal;
use serde_json::{Map, Number, Value, json};
use std::borrow::Cow;

/// A JSON tree with objects as member lists in insertion order.
#[derive(Debug)]
enum Doc {
    Null,
    Bool(bool),
    Number(Number),
    Text(String),
    List(Vec<Doc>),
    Record(Members),
}

#[derive(Debug)]
struct Members(Vec<(String, Doc)>);

impl Instance for Doc {
    type Object = Members;

    fn view(&self) -> InstanceRef<'_, Doc> {
        match self {
            Doc::Null => InstanceRef::Null,
            Doc::Bool(b) => InstanceRef::Bool(*b),
            Doc::Number(n) => InstanceRef::Number(Cow::Borrowed(n)),
            Doc::Text(s) => InstanceRef::String(s),
            Doc::List(items) => InstanceRef::Array(items),
            Doc::Record(members) => InstanceRef::Object(members),
        }
    }
}

impl InstanceObject<Doc> for Members {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn get(&self, name: &str) -> Option<&Doc> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a str, &'a Doc)>
    where
        Doc: 'a,
    {
        self.0.iter().map(|(key, value)| (key.as_str(), value))
    }
}

/// `value` as a [`Doc`], each object's members in reverse sorted order, so no keyword can
/// lean on sorted members. Recursive: test values are shallow.
fn doc(value: &Value) -> Doc {
    match value {
        Value::Null => Doc::Null,
        Value::Bool(b) => Doc::Bool(*b),
        Value::Number(n) => Doc::Number(n.clone()),
        Value::String(s) => Doc::Text(s.clone()),
        Value::Array(items) => Doc::List(items.iter().map(doc).collect()),
        Value::Object(object) => Doc::Record(Members(
            object
                .iter()
                .rev()
                .map(|(key, value)| (key.clone(), doc(value)))
                .collect(),
        )),
    }
}

fn compile(schema: &Value) -> CompiledSchema {
    compile_with(schema, false)
}

fn compile_with(schema: &Value, annotations: bool) -> CompiledSchema {
    let schema: Schema =
        Schema::new_from_serde_value(schema, &JsonSchemaSettings::default()).expect("parse");
    let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
    CompiledSchema::builder()
        .annotations(annotations)
        .build(&registry)
        .expect("compile schema")
}

/// The verdict and the sorted error messages of `instance`: errors over unsorted members come
/// in member order, so the comparison is as sets.
fn outcome<I: Instance>(compiled: &CompiledSchema, instance: &I) -> (bool, Vec<String>) {
    let mut errors: Vec<String> = compiled
        .validate(instance)
        .err()
        .unwrap_or_default()
        .iter()
        .map(ToString::to_string)
        .collect();
    errors.sort();
    (compiled.is_valid(instance), errors)
}

/// Schemas and instances that exercise every kind of keyword, valid and invalid.
fn cases() -> Vec<(Value, Value)> {
    vec![
        (json!({"type": "integer"}), json!(1.0)),
        (json!({"type": "integer"}), json!(1.5)),
        (json!({"type": ["string", "null"]}), json!(null)),
        (json!({"const": {"a": [1, 2.0]}}), json!({"a": [1.0, 2]})),
        (json!({"enum": ["x", 1, {"b": null}]}), json!({"b": null})),
        (json!({"enum": ["x", "y"]}), json!("z")),
        (json!({"minimum": 2, "multipleOf": 0.5}), json!(1.25)),
        (json!({"maxLength": 2, "pattern": "^a"}), json!("bcd")),
        (json!({"format": "email"}), json!("not an email")),
        (json!({"minItems": 2, "uniqueItems": true}), json!([1, 1.0])),
        (
            json!({"prefixItems": [{"type": "string"}], "items": {"type": "integer"}}),
            json!(["a", 1, "b"]),
        ),
        (
            json!({"contains": {"type": "string"}, "minContains": 2}),
            json!([1, "a"]),
        ),
        (
            json!({"required": ["a", "b", "c"], "properties": {"a": {"type": "string"}}}),
            json!({"c": 1, "a": 2}),
        ),
        (
            json!({"properties": {"a": true}, "additionalProperties": false}),
            json!({"z": 1, "a": 1, "y": 2}),
        ),
        (
            json!({"patternProperties": {"^x": {"type": "number"}}}),
            json!({"xa": "s", "xb": 1}),
        ),
        (
            json!({"propertyNames": {"maxLength": 1}, "maxProperties": 1}),
            json!({"ab": 1, "c": 2}),
        ),
        (
            json!({"dependentRequired": {"a": ["b"]}, "dependentSchemas": {"c": {"required": ["d"]}}}),
            json!({"a": 1, "c": 1}),
        ),
        (
            json!({"allOf": [{"properties": {"a": true}}], "unevaluatedProperties": false}),
            json!({"a": 1, "b": 2}),
        ),
        (
            json!({"anyOf": [{"type": "string"}, {"minimum": 5}]}),
            json!(3),
        ),
        (
            json!({"oneOf": [{"type": "number"}, {"type": "integer"}]}),
            json!(3),
        ),
        (
            json!({"if": {"type": "string"}, "then": {"minLength": 3}, "else": {"not": {"type": "null"}}}),
            json!(null),
        ),
        (
            json!({"$defs": {"node": {"type": "object", "properties": {"next": {"$ref": "#/$defs/node"}}}}, "$ref": "#/$defs/node"}),
            json!({"next": {"next": {"next": 1}}}),
        ),
        (
            json!({"prefixItems": [true], "unevaluatedItems": false}),
            json!([1, 2]),
        ),
    ]
}

#[test]
fn doc_instances_get_the_same_verdicts_and_errors_as_values() {
    let compiled: Vec<(CompiledSchema, Value)> = cases()
        .into_iter()
        .map(|(schema, instance)| (compile(&schema), instance))
        .collect();
    let expected: Vec<(bool, Vec<String>)> = compiled
        .iter()
        .map(|(schema, instance)| outcome(schema, instance))
        .collect();
    let actual: Vec<(bool, Vec<String>)> = compiled
        .iter()
        .map(|(schema, instance)| outcome(schema, &doc(instance)))
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_errors_carry_locations_and_values() {
    let compiled: CompiledSchema = compile(&json!({
        "properties": {"tags": {"items": {"enum": ["a", "b"]}}},
        "required": ["id"]
    }));
    let instance: Doc = doc(&json!({"tags": ["a", {"c": [1]}]}));
    let expected: Vec<String> = vec![
        "root: missing required property \"id\"".to_string(),
        "/tags/1: value {\"c\":[1]} not in enum (allowed: \"a\", \"b\")".to_string(),
    ];
    let actual: Vec<String> = compiled
        .validate(&instance)
        .expect_err("invalid")
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_iter_errors_stops_where_asked() {
    let compiled: CompiledSchema = compile(&json!({"items": {"type": "string"}}));
    let instance: Doc = doc(&json!([1, 2, 3]));
    let expected: Vec<String> = vec!["/0: expected string, got number".to_string()];
    let actual: Vec<String> = compiled
        .iter_errors(&instance)
        .take(1)
        .map(|error: ValidationError| error.to_string())
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_annotations_match_the_values() {
    let compiled: CompiledSchema = compile_with(
        &json!({"properties": {"a": {"title": "A"}}, "unevaluatedProperties": true}),
        true,
    );
    let value: Value = json!({"a": 1, "b": 2});
    let expected: Result<Vec<Annotation>, Vec<ValidationError>> = compiled.annotations(&value);
    let actual: Result<Vec<Annotation>, Vec<ValidationError>> = compiled.annotations(&doc(&value));
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_basic_output_matches_the_value() {
    let compiled: CompiledSchema = compile(&json!({"items": {"type": "string"}}));
    let value: Value = json!(["a", 1, null]);
    let expected: Output = compiled.evaluate(&value, OutputFormat::Basic);
    let actual: Output = compiled.evaluate(&doc(&value), OutputFormat::Basic);
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_to_value_rebuilds_the_value() {
    let expected: Value = json!({"b": [1, 2.5, {"c": null, "a": [true, "s"]}], "a": {}});
    let actual: Value = doc(&expected).to_value().into_owned();
    assert_eq!(expected, actual);
}

#[test]
fn doc_instance_to_value_rebuilds_empty_containers() {
    let expected: Value = json!([[], {}, [[]]]);
    let actual: Value = doc(&expected).to_value().into_owned();
    assert_eq!(expected, actual);
}

#[test]
fn json_equal_compares_numbers_across_representations() {
    let expected: bool = true;
    let actual: bool = json_equal(&doc(&json!({"n": [1]})), &json!({"n": [1.0]}));
    assert_eq!(expected, actual);
}

#[test]
fn json_equal_tells_different_members_apart_across_representations() {
    let expected: bool = false;
    let actual: bool = json_equal(&doc(&json!({"a": 1, "b": 2})), &json!({"a": 1, "c": 2}));
    assert_eq!(expected, actual);
}

#[test]
fn value_instance_lends_itself() {
    let value: Value = json!({"a": 1});
    let expected: bool = true;
    let actual: bool = matches!(value.to_value(), Cow::Borrowed(borrowed) if std::ptr::eq(borrowed, &raw const value));
    assert_eq!(expected, actual);
}

#[test]
fn map_object_claims_sorted_keys() {
    let expected: bool = true;
    let actual: bool = <Map<String, Value> as InstanceObject<Value>>::SORTED_KEYS;
    assert_eq!(expected, actual);
}
