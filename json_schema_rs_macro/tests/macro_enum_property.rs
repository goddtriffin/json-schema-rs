//! Integration test: `json_schema_to_rust!` with a schema that has an enum property.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}},"required":["status"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        status: schema_0::Status::Open,
    };
    assert!(matches!(root.status, schema_0::Status::Open));
}

#[test]
fn enum_property_deserializes() {
    let json = r#"{"status":"open"}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected = schema_0::Status::Open;
    let actual = root.status;
    assert_eq!(expected, actual);
}

#[test]
fn enum_property_deserializes_closed() {
    let json = r#"{"status":"closed"}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected = schema_0::Status::Closed;
    let actual = root.status;
    assert_eq!(expected, actual);
}
