//! Integration test: `json_schema_to_rust!` integer types follow the dialect's integer rule.
//! From draft 6 on an integer is any number with a zero fraction, so `1.0` and `2e0` deserialize
//! into integer fields, items, map values, union variants and newtypes; draft 4 counts only
//! numbers written as integers.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

mod numeric {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"n":{"type":"integer"},"small":{"type":"integer","minimum":0,"maximum":255},"odd":{"type":"integer","multipleOf":3},"list":{"type":"array","items":{"type":"integer"}},"either":{"type":["integer","string"]},"counts":{"type":"object","additionalProperties":{"type":"integer"}}},"required":["n"]}"#,
        constrained_newtypes = true,
    );
}

mod lexical {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"n":{"type":"integer"}},"required":["n"]}"#
    );
}

#[test]
fn zero_fraction_number_deserializes_into_every_integer_position() {
    let json: &str =
        r#"{"n":1.0,"small":2e0,"odd":6.0,"list":[1,2.0],"either":3.0,"counts":{"a":4.0}}"#;
    let root: numeric::schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: serde_json::Value = serde_json::json!({
        "n": 1, "small": 2, "odd": 6, "list": [1, 2], "either": 3, "counts": {"a": 4}
    });
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn zero_fraction_number_deserializes_into_map_values() {
    let root: numeric::schema_0::Root =
        serde_json::from_str(r#"{"n":1,"counts":{"a":4.0,"b":5}}"#).unwrap();
    let expected: Option<BTreeMap<String, i64>> =
        Some(BTreeMap::from([("a".to_string(), 4), ("b".to_string(), 5)]));
    let actual: Option<BTreeMap<String, i64>> = root.counts;
    assert_eq!(expected, actual);
}

#[test]
fn fraction_is_rejected_for_integer_field() {
    let expected: bool = true;
    let actual: bool = serde_json::from_str::<numeric::schema_0::Root>(r#"{"n":1.5}"#).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn zero_fraction_number_out_of_range_is_rejected() {
    let expected: bool = true;
    let actual: bool =
        serde_json::from_str::<numeric::schema_0::Root>(r#"{"n":1,"small":256.0}"#).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn constrained_integer_newtype_still_validates_zero_fraction_number() {
    let expected: bool = true;
    let actual: bool =
        serde_json::from_str::<numeric::schema_0::Root>(r#"{"n":1,"odd":4.0}"#).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn try_from_accepts_zero_fraction_number() {
    let root: numeric::schema_0::Root =
        numeric::schema_0::Root::try_from(serde_json::json!({"n": 7.0, "odd": 9.0})).unwrap();
    let expected: serde_json::Value = serde_json::json!({"n": 7, "odd": 9});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn draft_4_rejects_zero_fraction_number() {
    let expected: (bool, bool) = (true, true);
    let actual: (bool, bool) = (
        serde_json::from_str::<lexical::schema_0::Root>(r#"{"n":1}"#).is_ok(),
        serde_json::from_str::<lexical::schema_0::Root>(r#"{"n":1.0}"#).is_err(),
    );
    assert_eq!(expected, actual);
}

json_schema_to_rust!(r#"{"type":"array","items":{"type":"integer"}}"#);

#[test]
fn root_alias_of_integers_reads_through_integer_wrapper() {
    let expected: schema_0::Root = vec![3, 4];
    let actual: schema_0::Root =
        serde_json::from_str::<json_schema_rs::integer::Integer<schema_0::Root>>("[3.0, 4e0]")
            .unwrap()
            .0;
    assert_eq!(expected, actual);
}
