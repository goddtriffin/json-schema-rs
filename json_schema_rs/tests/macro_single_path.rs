//! Integration test: `generate_rust_schema!` with a single file path.

use json_schema_rs_macro::generate_rust_schema;

generate_rust_schema!("tests/fixtures/simple.json");

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = simple::Root {
        id: String::from("id1"),
        name: Some(String::from("alice")),
    };
    assert_eq!(root.id, "id1");
    assert_eq!(root.name.as_deref(), Some("alice"));
}
