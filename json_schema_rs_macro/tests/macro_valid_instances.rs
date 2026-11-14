//! Integration test: `json_schema_to_rust!` generates types that accept every schema-valid
//! instance when a keyword constrains only some instances: a property only `then` defines, one
//! both `then` and `else` define, an `additionalProperties` schema or `false` beside
//! `patternProperties`, and `items` beside `prefixItems`. Self-validation still enforces those
//! keywords where they apply.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"kind":{"type":"string"}},"if":{"properties":{"kind":{"const":"a"}}},"then":{"properties":{"x":{"type":"string","minLength":3}}}}"#,
    r#"{"type":"object","patternProperties":{"^x-":{}},"additionalProperties":{"type":"string"}}"#,
    r#"{"type":"object","properties":{"a":{"type":"string"}},"patternProperties":{"^x-":{}},"additionalProperties":false}"#,
    r#"{"type":"array","prefixItems":[{"type":"integer"}],"items":{"type":"string"}}"#,
    r#"{"type":"object","properties":{"kind":{"type":"string"}},"if":{"properties":{"kind":{"const":"a"}}},"then":{"properties":{"v":{"type":"integer","minimum":1}}},"else":{"properties":{"v":{"type":"integer","maximum":0}}}}"#
);

#[test]
fn then_only_property_deserializes_a_value_of_another_type() {
    let root: schema_0::Root = serde_json::from_str(r#"{"kind":"b","x":5}"#).unwrap();
    let expected: serde_json::Value = serde_json::json!({"kind": "b", "x": 5});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn then_only_property_holds_the_value_as_json() {
    let root: schema_0::Root = serde_json::from_str(r#"{"kind":"b","x":5}"#).unwrap();
    let expected: Option<serde_json::Value> = Some(serde_json::json!(5));
    let actual: Option<serde_json::Value> = root.x;
    assert_eq!(expected, actual);
}

#[test]
fn then_only_property_try_from_accepts_value_when_if_fails() {
    let root: schema_0::Root =
        schema_0::Root::try_from(serde_json::json!({"kind": "b", "x": "a"})).unwrap();
    let expected: serde_json::Value = serde_json::json!({"kind": "b", "x": "a"});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn then_only_property_try_from_rejects_value_violating_then_when_if_holds() {
    let expected: bool = true;
    let actual: bool =
        schema_0::Root::try_from(serde_json::json!({"kind": "a", "x": "ab"})).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn pattern_property_beside_additional_properties_schema_deserializes() {
    let root: schema_1::Root = serde_json::from_str(r#"{"x-a":5}"#).unwrap();
    let expected: BTreeMap<String, serde_json::Value> =
        BTreeMap::from([("x-a".to_string(), serde_json::json!(5))]);
    let actual: BTreeMap<String, serde_json::Value> = root;
    assert_eq!(expected, actual);
}

#[test]
fn pattern_property_beside_additional_properties_false_deserializes_into_additional() {
    let root: schema_2::Root = serde_json::from_str(r#"{"x-a":1}"#).unwrap();
    let expected: BTreeMap<String, serde_json::Value> =
        BTreeMap::from([("x-a".to_string(), serde_json::json!(1))]);
    let actual: BTreeMap<String, serde_json::Value> = root.additional;
    assert_eq!(expected, actual);
}

#[test]
fn pattern_property_beside_additional_properties_false_try_from_accepts_matching_key() {
    let root: schema_2::Root = schema_2::Root::try_from(serde_json::json!({"x-a": 1})).unwrap();
    let expected: serde_json::Value = serde_json::json!({"x-a": 1});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn pattern_property_beside_additional_properties_false_try_from_rejects_unmatched_key() {
    let expected: bool = true;
    let actual: bool = schema_2::Root::try_from(serde_json::json!({"b": 1})).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn items_beside_prefix_items_deserializes_each_position() {
    let root: schema_3::Root = serde_json::from_str(r#"[1,"a"]"#).unwrap();
    let expected: Vec<serde_json::Value> = vec![serde_json::json!(1), serde_json::json!("a")];
    let actual: Vec<serde_json::Value> = root;
    assert_eq!(expected, actual);
}

#[test]
fn then_else_shared_property_deserializes() {
    let root: schema_4::Root = serde_json::from_str(r#"{"kind":"a","v":5}"#).unwrap();
    let expected: serde_json::Value = serde_json::json!({"kind": "a", "v": 5});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn then_else_shared_property_try_from_rejects_value_violating_the_branch_that_applies() {
    let expected: bool = true;
    let actual: bool = schema_4::Root::try_from(serde_json::json!({"kind": "a", "v": 0})).is_err();
    assert_eq!(expected, actual);
}
