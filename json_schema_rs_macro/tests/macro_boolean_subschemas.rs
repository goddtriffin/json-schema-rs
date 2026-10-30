//! Integration test: `json_schema_to_rust!` with boolean subschemas (`true` / `false` properties and `items: true`).

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"anything":true,"never":false,"tags":{"type":"array","items":true}}}"#
);

#[test]
fn boolean_subschemas_struct_has_anything_and_tags_fields_and_omits_never() {
    let root = schema_0::Root {
        anything: None,
        tags: None,
        additional: BTreeMap::new(),
    };
    let expected: (Option<serde_json::Value>, Option<Vec<serde_json::Value>>) = (None, None);
    let actual: (Option<serde_json::Value>, Option<Vec<serde_json::Value>>) =
        (root.anything, root.tags);
    assert_eq!(expected, actual);
}

#[test]
fn true_property_and_items_true_deserialize_any_values() {
    let json = r#"{"tags":[1,"a",null],"anything":5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: (Option<serde_json::Value>, Option<Vec<serde_json::Value>>) = (
        Some(serde_json::json!(5)),
        Some(vec![
            serde_json::json!(1),
            serde_json::json!("a"),
            serde_json::Value::Null,
        ]),
    );
    let actual: (Option<serde_json::Value>, Option<Vec<serde_json::Value>>) =
        (root.anything, root.tags);
    assert_eq!(expected, actual);
}

#[test]
fn true_property_holds_null_apart_from_absence() {
    let json = r#"{"anything":null}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<serde_json::Value> = Some(serde_json::Value::Null);
    let actual: Option<serde_json::Value> = root.anything;
    assert_eq!(expected, actual);
}
