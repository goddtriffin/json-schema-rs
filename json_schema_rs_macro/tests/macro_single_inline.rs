//! Integration test: `json_schema_to_rust!` with a single inline JSON Schema string.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        id: String::from("test"),
    };
    assert_eq!(root.id, "test");
}
