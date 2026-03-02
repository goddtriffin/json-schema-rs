//! Integration test: `json_schema_to_rust!` with a single file path.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!("tests/fixtures/simple.json");

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = simple::Root {
        id: String::from("id1"),
        name: Some(String::from("alice")),
    };
    assert_eq!(root.id, "id1");
    assert_eq!(root.name.as_deref(), Some("alice"));
}
