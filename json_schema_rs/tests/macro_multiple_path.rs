//! Integration test: `generate_rust_schema!` with multiple file paths.

use json_schema_rs_macro::generate_rust_schema;

generate_rust_schema!("tests/fixtures/simple.json", "tests/fixtures/other.json");

#[test]
fn expanded_code_compiles_and_both_modules_exist() {
    let root_simple = simple::Root {
        id: String::from("1"),
        name: Some(String::from("foo")),
    };
    let root_other = other::Root {
        value: Some(String::from("bar")),
    };
    assert_eq!(root_simple.id, "1");
    assert_eq!(root_other.value.as_deref(), Some("bar"));
}
