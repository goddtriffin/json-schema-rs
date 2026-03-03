//! Integration test: `json_schema_to_rust!` with a schema that has boolean properties.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"enabled":{"type":"boolean"},"flag":{"type":"boolean"}},"required":["enabled"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        enabled: true,
        flag: Some(false),
    };
    assert!(root.enabled);
    assert_eq!(root.flag, Some(false));
}

#[test]
fn boolean_properties_deserialize() {
    let json = r#"{"enabled":true,"flag":false}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    assert!(root.enabled);
    assert_eq!(root.flag, Some(false));
}

#[test]
fn optional_boolean_absent_deserializes_as_none() {
    let json = r#"{"enabled":false}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    assert!(!root.enabled);
    assert_eq!(root.flag, None);
}
