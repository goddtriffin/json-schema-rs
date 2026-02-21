//! Integration test: `json_schema_to_rust!` with a single inline schema with hyphenated property key.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(r#"{"type":"object","properties":{"foo-bar":{"type":"string"}}}"#);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        foo_bar: Some(String::from("baz")),
    };
    assert_eq!(root.foo_bar.as_deref(), Some("baz"));
}

#[test]
fn hyphenated_property_deserializes_with_rename() {
    let json = r#"{"foo-bar":"value"}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected = Some("value");
    let actual = root.foo_bar.as_deref();
    assert_eq!(expected, actual);
}
