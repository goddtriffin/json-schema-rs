//! Integration test: `json_schema_to_rust!` with a schema containing a deprecated property.

#![allow(deprecated)]

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"legacy":{"type":"string","deprecated":true}}}"#
);

#[test]
fn expanded_code_compiles_and_contains_deprecated_attr() {
    let root = schema_0::Root {
        legacy: Some(String::from("still-valid")),
    };
    assert_eq!(root.legacy.as_deref(), Some("still-valid"));
}
