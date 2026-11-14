//! Integration test: the `Deserialize` of generated structs with an additional-properties map
//! (the `json_schema_rs_macro::Deserialize` derive with `#[serde(flatten)]`) and of untagged
//! unions (`deserialize_untagged!` and the self-validating union impl). None reads through
//! serde's `Content` buffer, so numbers deserialize with `serde_json/arbitrary_precision` on
//! (`--features arbitrary-precision`) as well as off, and the struct keeps serde's derive
//! semantics for missing, repeated and unknown keys.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"plain":{"type":"number"},"opt":{"type":"integer"},"req_null":{"type":["number","null"]},"opt_null":{"type":["number","null"]},"Renamed-Key":{"type":"string"}},"required":["plain","req_null"]}"#,
    r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":{"type":"number"}}"#,
    r#"{"type":"object","properties":{"amount":{"type":["integer","number","string"]},"choice":{"oneOf":[{"type":"number"},{"type":"string"}]},"either":{"anyOf":[{"type":"integer"},{"type":"number"}]},"point":{"anyOf":[{"type":"number"},{"type":"object","properties":{"x":{"type":"number"}},"required":["x"]}]}}}"#
);

/// A number as `serde_json` itself reads it (exact text with `arbitrary_precision`, else the
/// nearest `f64`).
fn number(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap()
}

/// The error of deserializing `json` into `T`, as text.
fn error<T: serde::de::DeserializeOwned + std::fmt::Debug>(json: &str) -> String {
    serde_json::from_str::<T>(json).unwrap_err().to_string()
}

#[test]
fn struct_with_additional_map_reads_float_fields() {
    let root: schema_0::Root =
        serde_json::from_str(r#"{"plain": 1.5, "req_null": 2.25, "opt_null": 0.5}"#).unwrap();
    let expected: (f64, Option<f64>, json_schema_rs::Nullable<f64>) =
        (1.5, Some(2.25), json_schema_rs::Nullable::Value(0.5));
    let actual: (f64, Option<f64>, json_schema_rs::Nullable<f64>) =
        (root.plain, root.req_null, root.opt_null);
    assert_eq!(expected, actual);
}

#[test]
fn struct_with_additional_map_reads_float_fields_from_value() {
    let value: serde_json::Value =
        serde_json::from_str(r#"{"plain": 1.50, "req_null": 1e2}"#).unwrap();
    let root: schema_0::Root = serde_json::from_value(value).unwrap();
    let expected: (f64, Option<f64>) = (1.5, Some(100.0));
    let actual: (f64, Option<f64>) = (root.plain, root.req_null);
    assert_eq!(expected, actual);
}

#[test]
fn struct_with_additional_map_keeps_unknown_bignum_and_float() {
    let root: schema_0::Root = serde_json::from_str(
        r#"{"plain": 1, "req_null": null, "big": 123456789012345678901234567890, "ratio": 1.50}"#,
    )
    .unwrap();
    let expected: BTreeMap<String, serde_json::Value> = BTreeMap::from([
        ("big".to_string(), number("123456789012345678901234567890")),
        ("ratio".to_string(), number("1.50")),
    ]);
    let actual: BTreeMap<String, serde_json::Value> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn struct_with_typed_additional_map_reads_floats() {
    let root: schema_1::Root = serde_json::from_str(
        r#"{"name": "a", "x": 1.5, "y": 1e2, "z": 123456789012345678901234567890}"#,
    )
    .unwrap();
    let expected: BTreeMap<String, f64> = BTreeMap::from([
        ("x".to_string(), 1.5),
        ("y".to_string(), 100.0),
        ("z".to_string(), 1.234_567_890_123_456_8e29),
    ]);
    let actual: BTreeMap<String, f64> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn struct_with_typed_additional_map_rejects_wrong_type() {
    let expected: &str = "invalid type: string \"no\", expected f64 at line 1 column 10";
    let actual: String = error::<schema_1::Root>(r#"{"x": "no"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn struct_rejects_missing_required_field() {
    let expected: &str = "missing field `plain` at line 1 column 15";
    let actual: String = error::<schema_0::Root>(r#"{"req_null": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn struct_rejects_missing_required_nullable_field() {
    let expected: &str = "missing field `req_null` at line 1 column 12";
    let actual: String = error::<schema_0::Root>(r#"{"plain": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn struct_rejects_repeated_known_key() {
    let expected: &str = "duplicate field `plain` at line 1 column 20";
    let actual: String = error::<schema_0::Root>(r#"{"plain": 1, "plain": 2, "req_null": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn struct_keeps_last_value_of_repeated_unknown_key() {
    let root: schema_0::Root =
        serde_json::from_str(r#"{"plain": 1, "req_null": 1, "x": 1, "x": "last"}"#).unwrap();
    let expected: BTreeMap<String, serde_json::Value> =
        BTreeMap::from([("x".to_string(), serde_json::json!("last"))]);
    let actual: BTreeMap<String, serde_json::Value> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn struct_rejects_non_object() {
    let expected: &str = "invalid type: sequence, expected struct Root at line 1 column 0";
    let actual: String = error::<schema_0::Root>("[]");
    assert_eq!(expected, actual);
}

#[test]
fn optional_nullable_field_tells_null_from_absent() {
    let absent: schema_0::Root = serde_json::from_str(r#"{"plain": 1, "req_null": null}"#).unwrap();
    let null: schema_0::Root =
        serde_json::from_str(r#"{"plain": 1, "req_null": null, "opt_null": null}"#).unwrap();
    let expected: (json_schema_rs::Nullable<f64>, json_schema_rs::Nullable<f64>) = (
        json_schema_rs::Nullable::Absent,
        json_schema_rs::Nullable::Null,
    );
    let actual: (json_schema_rs::Nullable<f64>, json_schema_rs::Nullable<f64>) =
        (absent.opt_null, null.opt_null);
    assert_eq!(expected, actual);
}

#[test]
fn optional_field_is_none_when_absent() {
    let root: schema_0::Root = serde_json::from_str(r#"{"plain": 1, "req_null": null}"#).unwrap();
    let expected: (Option<i64>, Option<String>) = (None, None);
    let actual: (Option<i64>, Option<String>) = (root.opt, root.renamed_key);
    assert_eq!(expected, actual);
}

#[test]
fn renamed_field_reads_its_json_key_and_leaves_its_rust_name_unknown() {
    let root: schema_0::Root = serde_json::from_str(
        r#"{"plain": 1, "req_null": null, "Renamed-Key": "a", "renamed_key": "b"}"#,
    )
    .unwrap();
    let expected: (Option<String>, BTreeMap<String, serde_json::Value>) = (
        Some("a".to_string()),
        BTreeMap::from([("renamed_key".to_string(), serde_json::json!("b"))]),
    );
    let actual: (Option<String>, BTreeMap<String, serde_json::Value>) =
        (root.renamed_key, root.additional);
    assert_eq!(expected, actual);
}

#[test]
fn escaped_known_key_reads_into_its_field() {
    let root: schema_0::Root = serde_json::from_str(r#"{"plain": 2.5, "req_null": null}"#).unwrap();
    let expected: f64 = 2.5;
    let actual: f64 = root.plain;
    assert_eq!(expected, actual);
}

#[test]
fn struct_round_trips_with_unknown_keys() {
    let expected: serde_json::Value = serde_json::json!({
        "Renamed-Key": "a", "opt_null": null, "plain": 1.5, "req_null": null, "x": [1, 2.5]
    });
    let root: schema_0::Root = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

/// `amount` of schema 2: which variant `json` deserializes into, and its value as JSON.
fn amount(json: &str) -> (&'static str, serde_json::Value) {
    let root: schema_2::Root = serde_json::from_str(&format!(r#"{{"amount": {json}}}"#)).unwrap();
    match root.amount.unwrap() {
        schema_2::Amount::Integer(n) => ("integer", serde_json::json!(n)),
        schema_2::Amount::Number(n) => ("number", serde_json::json!(n)),
        schema_2::Amount::String(s) => ("string", serde_json::json!(s)),
    }
}

#[test]
fn type_array_union_reads_float_into_number_variant() {
    let expected: (&str, serde_json::Value) = ("number", serde_json::json!(1.5));
    let actual: (&str, serde_json::Value) = amount("1.5");
    assert_eq!(expected, actual);
}

#[test]
fn type_array_union_reads_integer_into_integer_variant() {
    let expected: (&str, serde_json::Value) = ("integer", serde_json::json!(7));
    let actual: (&str, serde_json::Value) = amount("7");
    assert_eq!(expected, actual);
}

#[test]
fn type_array_union_reads_bignum_into_number_variant() {
    let expected: (&str, serde_json::Value) =
        ("number", serde_json::json!(1.234_567_890_123_456_8e29));
    let actual: (&str, serde_json::Value) = amount("123456789012345678901234567890");
    assert_eq!(expected, actual);
}

#[test]
fn type_array_union_reads_string_into_string_variant() {
    let expected: (&str, serde_json::Value) = ("string", serde_json::json!("x"));
    let actual: (&str, serde_json::Value) = amount(r#""x""#);
    assert_eq!(expected, actual);
}

#[test]
fn type_array_union_rejects_value_matching_no_variant() {
    let expected: &str =
        "data did not match any variant of untagged enum Amount at line 1 column 16";
    let actual: String = error::<schema_2::Root>(r#"{"amount": true}"#);
    assert_eq!(expected, actual);
}

#[test]
fn self_validating_one_of_reads_float_into_number_branch() {
    let root: schema_2::Root = serde_json::from_str(r#"{"choice": 1.5}"#).unwrap();
    let expected: Option<f64> = Some(1.5);
    let actual: Option<f64> = match root.choice.unwrap() {
        schema_2::ChoiceOneOf::Number(n) => Some(n),
        schema_2::ChoiceOneOf::String(_) => None,
    };
    assert_eq!(expected, actual);
}

#[test]
fn self_validating_any_of_reads_float_into_number_branch() {
    let root: schema_2::Root = serde_json::from_str(r#"{"either": 2.5}"#).unwrap();
    let expected: Option<f64> = Some(2.5);
    let actual: Option<f64> = match root.either.unwrap() {
        schema_2::EitherAnyOf::Integer(_) => None,
        schema_2::EitherAnyOf::Number(n) => Some(n),
    };
    assert_eq!(expected, actual);
}

#[test]
fn self_validating_any_of_reads_float_into_struct_branch() {
    let root: schema_2::Root = serde_json::from_str(r#"{"point": {"x": 1.25, "y": 0.5}}"#).unwrap();
    let expected: Option<(f64, BTreeMap<String, serde_json::Value>)> =
        Some((1.25, BTreeMap::from([("y".to_string(), number("0.5"))])));
    let actual: Option<(f64, BTreeMap<String, serde_json::Value>)> = match root.point.unwrap() {
        schema_2::PointAnyOf::Number(_) => None,
        schema_2::PointAnyOf::Variant1(point) => Some((point.x, point.additional)),
    };
    assert_eq!(expected, actual);
}
