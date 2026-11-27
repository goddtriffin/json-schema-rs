//! `Validated` and `self_validating!` used outside this crate, as generated code uses them.
//! They live here because clippy's `allow_attributes` (forbidden in this crate) fires on an
//! in-crate expansion's `#[allow(deprecated)]` but skips external macros; that `#[allow]` is
//! covered by the generated-code gate's `deprecated_types` scenario.

use json_schema_rs::{
    EmbeddedDocument, EmbeddedDocumentKind, TypeValidationError, TypeValidators, Validated,
};
use serde_json::json;

static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
    None,
    &[EmbeddedDocument {
        uri: "json-schema:///",
        json: r#"{"properties":{"tags":{"type":"array","minItems":1,"uniqueItems":true}}}"#,
        kind: EmbeddedDocumentKind::Parsed(None),
    }],
    &[&["json-schema:///#"]],
);

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Pet {
    tags: Vec<String>,
}

json_schema_rs::self_validating!(Pet, VALIDATORS, 0);

/// A `Pet` that validated, made through `Validated`'s only public ways in.
fn validated(tags: &[&str]) -> Validated<Pet> {
    Validated::try_from(json!({ "tags": tags })).expect("valid instance")
}

fn errors(result: Result<Validated<Pet>, serde_json::Error>) -> String {
    result.map_or_else(|error| error.to_string(), |_| String::new())
}

#[test]
fn deserializes_a_valid_instance() {
    let expected: Validated<Pet> = validated(&["a"]);
    let actual: Validated<Pet> =
        serde_json::from_str(r#"{"tags": ["a"]}"#).expect("valid instance");
    assert_eq!(expected, actual);
}

#[test]
fn rejects_what_plain_deserialize_accepts_with_every_error() {
    let expected: (bool, String) = (
        true,
        Pet::try_from(json!({"tags": []}))
            .map_or_else(|error| error.to_string(), |_| String::new()),
    );
    let actual: (bool, String) = (
        serde_json::from_str::<Pet>(r#"{"tags": []}"#).is_ok(),
        errors(serde_json::from_value(json!({"tags": []}))),
    );
    assert_eq!(expected, actual);
}

#[test]
fn rejects_a_duplicate_with_the_validator_error() {
    let expected: String = r#"/tags: array has duplicate items (value: "a")"#.to_string();
    let actual: String = errors(serde_json::from_value(json!({"tags": ["a", "a"]})));
    assert_eq!(expected, actual);
}

#[test]
fn try_from_value_validates() {
    let expected: bool = true;
    let actual: bool = matches!(
        Validated::<Pet>::try_from(json!({"tags": []})),
        Err(TypeValidationError::Invalid(_))
    );
    assert_eq!(expected, actual);
}

#[test]
fn derefs_to_the_value() {
    let expected: bool = true;
    let actual: bool = validated(&["a"]).validate().is_ok();
    assert_eq!(expected, actual);
}

#[test]
fn serializes_as_the_value() {
    let expected: serde_json::Value = json!({"tags": ["a"]});
    let actual: serde_json::Value = serde_json::to_value(validated(&["a"])).expect("serializes");
    assert_eq!(expected, actual);
}
