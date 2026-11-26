//! The `deserialize_union!` and `constrained_newtype!` macros expanded outside this crate, as
//! generated code expands them: clippy's `allow_attributes` (forbidden in this crate) skips
//! external macros, so the expansions' `#[allow(deprecated)]` is checked here as consumers see it.

use json_schema_rs::{
    EmbeddedDocument, EmbeddedDocumentKind, Schema, TypeValidationError, TypeValidators,
    ValidateError, ValidationError, validate,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A `oneOf` of two object shapes (type 0), each branch also a type (1, 2).
const SHAPES: &str = r#"{"oneOf":[{"type":"object","properties":{"r":{"type":"number"}},"required":["r"]},{"type":"object","properties":{"side":{"type":"number"}},"required":["side"]}]}"#;

static SHAPE_VALIDATORS: TypeValidators<'static> = TypeValidators::new(
    None,
    &[EmbeddedDocument {
        uri: "json-schema:///",
        json: SHAPES,
        kind: EmbeddedDocumentKind::Parsed(None),
    }],
    &[
        &["json-schema:///#"],
        &["json-schema:///#/oneOf/0"],
        &["json-schema:///#/oneOf/1"],
    ],
);

#[derive(Debug, PartialEq, Deserialize)]
struct Circle {
    r: f64,
}

#[derive(Debug, PartialEq, Deserialize)]
struct Square {
    side: f64,
}

#[derive(Debug, PartialEq)]
enum Shape {
    Circle(Circle),
    Square(Square),
}

json_schema_rs::deserialize_union!(Shape, SHAPE_VALIDATORS, OneOf, Some(0), [1, 2], |value| {
    0 => Circle::deserialize(value).map(Self::Circle),
    _ => Square::deserialize(value).map(Self::Square),
});

#[test]
fn try_from_reads_the_recorded_variant() {
    let expected: Result<Shape, String> = Ok(Shape::Square(Square { side: 2.0 }));
    let actual: Result<Shape, String> = SHAPE_VALIDATORS
        .deserialize(0, json!({"side": 2}))
        .map_err(|error: TypeValidationError| error.to_string());
    assert_eq!(expected, actual);
}

#[test]
fn plain_deserialization_of_a_union_takes_the_buffered_path() {
    let expected: Result<Shape, String> = Ok(Shape::Circle(Circle { r: 1.0 }));
    let actual: Result<Shape, String> = serde_json::from_str::<Shape>(r#"{"r": 1}"#)
        .map_err(|error: serde_json::Error| error.to_string());
    assert_eq!(expected, actual);
}

/// A parent whose schema allows any string name, and a `Name` newtype that needs five
/// characters: narrower than its position, which codegen never generates, so trusting the
/// parent's validation is observable.
const TRUST: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"$defs":{"Name":{"type":"string","minLength":5}}}"#;

static TRUST_VALIDATORS: TypeValidators<'static> = TypeValidators::new(
    None,
    &[EmbeddedDocument {
        uri: "json-schema:///",
        json: TRUST,
        kind: EmbeddedDocumentKind::Parsed(None),
    }],
    &[&["json-schema:///#"], &["json-schema:///#/$defs/Name"]],
);

#[derive(Debug, PartialEq, Serialize)]
#[serde(transparent)]
struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TRUST_VALIDATORS, 1);

#[derive(Debug, PartialEq, Deserialize)]
struct Named {
    name: Name,
}

#[test]
fn deserialize_reads_newtypes_trusted_after_validating_the_instance() {
    let expected: Result<String, String> = Ok("ab".to_string());
    let actual: Result<String, String> = TRUST_VALIDATORS
        .deserialize::<Named>(0, json!({"name": "ab"}))
        .map(|named: Named| named.name.into_inner())
        .map_err(|error: TypeValidationError| error.to_string());
    assert_eq!(expected, actual);
}

#[test]
fn plain_deserialize_checks_each_newtype() {
    let schema: Schema = Schema::try_from(r#"{"type":"string","minLength":5}"#).unwrap();
    let name_errors: Vec<ValidationError> = match validate(&schema, &Value::from("ab")) {
        Err(ValidateError::Instance(errors)) => errors,
        other => panic!("expected instance errors, got {other:?}"),
    };
    let expected: Result<Named, String> =
        Err(TypeValidationError::Invalid(name_errors).to_string());
    let actual: Result<Named, String> = serde_json::from_value::<Named>(json!({"name": "ab"}))
        .map_err(|error: serde_json::Error| error.to_string());
    assert_eq!(expected, actual);
}
