//! Integration test: `json_schema_to_rust!` with multiple file paths.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!("tests/fixtures/simple.json", "tests/fixtures/other.json");

#[test]
fn expanded_code_compiles_and_both_modules_exist() {
    let root_simple = simple::Root {
        id: String::from("1"),
        name: Some(String::from("foo")),
        additional: BTreeMap::new(),
    };
    let root_other = other::Root {
        value: Some(String::from("bar")),
        additional: BTreeMap::new(),
    };
    let expected: (&str, Option<&str>, Option<&str>) = ("1", Some("foo"), Some("bar"));
    let actual: (&str, Option<&str>, Option<&str>) = (
        root_simple.id.as_str(),
        root_simple.name.as_deref(),
        root_other.value.as_deref(),
    );
    assert_eq!(expected, actual);
}
