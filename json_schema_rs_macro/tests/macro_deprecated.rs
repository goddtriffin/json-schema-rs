//! Integration test: `json_schema_to_rust!` with a schema containing a deprecated property.

#![expect(deprecated)]

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"legacy":{"type":"string","deprecated":true}}}"#
);

#[test]
fn expanded_code_compiles_and_contains_deprecated_attr() {
    let root = schema_0::Root {
        legacy: Some(String::from("still-valid")),
        additional: BTreeMap::new(),
    };
    let expected: Option<&str> = Some("still-valid");
    let actual: Option<&str> = root.legacy.as_deref();
    assert_eq!(expected, actual);
}
