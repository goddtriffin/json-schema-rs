//! Integration test: `json_schema_to_rust!` with multiple inline JSON Schema strings.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"a":{"type":"string"}}}"#,
    r#"{"type":"object","properties":{"b":{"type":"string"}},"required":["b"]}"#
);

#[test]
fn expanded_code_compiles_and_both_modules_exist() {
    let first = schema_0::Root {
        a: Some(String::from("x")),
    };
    let second = schema_1::Root {
        b: String::from("y"),
    };
    assert_eq!(first.a.as_deref(), Some("x"));
    assert_eq!(second.b, "y");
}
