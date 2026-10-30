//! Integration test: `json_schema_to_rust!` with a single file path.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!("tests/fixtures/simple.json");

#[test]
fn expanded_code_compiles_and_root_exists() {
    let root = simple::Root {
        id: String::from("id1"),
        name: Some(String::from("alice")),
        additional: BTreeMap::new(),
    };
    let expected: (&str, Option<&str>) = ("id1", Some("alice"));
    let actual: (&str, Option<&str>) = (root.id.as_str(), root.name.as_deref());
    assert_eq!(expected, actual);
}
