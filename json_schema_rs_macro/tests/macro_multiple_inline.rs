//! Integration test: `json_schema_to_rust!` with multiple inline JSON Schema strings.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"a":{"type":"string"}}}"#,
    r#"{"type":"object","properties":{"b":{"type":"string"}},"required":["b"]}"#
);

#[test]
fn expanded_code_compiles_and_both_modules_exist() {
    let first = schema_0::Root {
        a: Some(String::from("x")),
        additional: BTreeMap::new(),
    };
    let second = schema_1::Root {
        b: String::from("y"),
        additional: BTreeMap::new(),
    };
    let expected: (Option<&str>, &str) = (Some("x"), "y");
    let actual: (Option<&str>, &str) = (first.a.as_deref(), second.b.as_str());
    assert_eq!(expected, actual);
}
