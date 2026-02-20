//! Integration test: `generate_rust_schema!` with a single inline JSON Schema string.

use json_schema_rs_macro::generate_rust_schema;

generate_rust_schema!(
    r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#
);

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = schema_0::Root {
        id: String::from("test"),
    };
    assert_eq!(root.id, "test");
}
