//! Integration test: `json_schema_to_rust!` generates a root map or array as a newtype that
//! validates itself, since no parent checks it.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"array","maxItems":2,"items":{"type":"string"}}"#,
    r#"{"type":"object","maxProperties":1,"additionalProperties":{"type":"integer"}}"#
);

#[test]
fn root_array_try_from_accepts_conforming_value() {
    let expected: bool = true;
    let actual: bool = schema_0::Root::try_from(serde_json::json!(["a", "b"])).is_ok();
    assert_eq!(expected, actual);
}

#[test]
fn root_array_try_from_rejects_too_many_items() {
    let expected: bool = true;
    let actual: bool = schema_0::Root::try_from(serde_json::json!(["a", "b", "c"])).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn root_array_validate_reports_too_many_items() {
    let root: schema_0::Root = schema_0::Root(vec!["a".into(), "b".into(), "c".into()]);
    let expected: bool = true;
    let actual: bool = root.validate().is_err();
    assert_eq!(expected, actual);
}

#[test]
fn root_map_reads_zero_fraction_integer() {
    let root: schema_1::Root = serde_json::from_value(serde_json::json!({"a": 1.0})).unwrap();
    let expected: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::from([("a".to_string(), 1)]);
    let actual: std::collections::BTreeMap<String, i64> = root.0;
    assert_eq!(expected, actual);
}

#[test]
fn root_map_try_from_rejects_too_many_properties() {
    let expected: bool = true;
    let actual: bool = schema_1::Root::try_from(serde_json::json!({"a": 1, "b": 2})).is_err();
    assert_eq!(expected, actual);
}
