//! Integration test: `json_schema_to_rust!` with boolean root schemas (`true` and `false`).

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!("true", "false");

#[test]
fn true_root_deserializes_any_value() {
    let expected: serde_json::Value = serde_json::json!({"a": [1, null]});
    let actual: schema_0::Root = serde_json::from_str(r#"{"a":[1,null]}"#).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn false_root_deserializes_no_value() {
    let expected: bool = false;
    let actual: bool = serde_json::from_str::<schema_1::Root>("1").is_ok();
    assert_eq!(expected, actual);
}
