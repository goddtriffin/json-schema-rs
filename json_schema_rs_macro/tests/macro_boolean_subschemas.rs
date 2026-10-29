//! Integration test: `json_schema_to_rust!` with boolean subschemas (`true` / `false` properties and `items: true`).

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"anything":true,"never":false,"tags":{"type":"array","items":true}}}"#
);

#[test]
fn boolean_subschemas_struct_has_only_tags_field() {
    let root = schema_0::Root { tags: None };
    let expected: Option<Vec<serde_json::Value>> = None;
    let actual: Option<Vec<serde_json::Value>> = root.tags;
    assert_eq!(expected, actual);
}

#[test]
fn items_true_deserializes_any_values_and_ignores_omitted_properties() {
    let json = r#"{"tags":[1,"a",null],"anything":5}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Option<Vec<serde_json::Value>> = Some(vec![
        serde_json::json!(1),
        serde_json::json!("a"),
        serde_json::Value::Null,
    ]);
    let actual: Option<Vec<serde_json::Value>> = root.tags;
    assert_eq!(expected, actual);
}
