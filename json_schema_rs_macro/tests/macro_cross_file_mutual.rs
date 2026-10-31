//! Integration test: `json_schema_to_rust!` with two path schemas that refer to each other
//! (`tree.json` → `parent.json` → `tree.json`), passed together.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    "tests/fixtures/cross_file/tree.json",
    "tests/fixtures/cross_file/parent.json"
);

#[test]
fn files_referring_to_each_other_reach_through_both_types() {
    let json =
        r#"{"name":"leaf","parent":{"label":"p","tree":{"name":"root","parent":{"label":"q"}}}}"#;
    let tree: tree::Tree = serde_json::from_str(json).unwrap();
    let expected: Option<String> = Some("q".to_string());
    let actual: Option<String> = tree
        .parent
        .and_then(|parent: parent::Parent| parent.tree)
        .and_then(|tree: Box<tree::Tree>| tree.parent)
        .map(|parent| parent.label);
    assert_eq!(expected, actual);
}

#[test]
fn files_referring_to_each_other_round_trip() {
    let expected: serde_json::Value = serde_json::json!({
        "name": "leaf",
        "parent": {"label": "p", "tree": {"name": "root", "parent": {"label": "q"}}}
    });
    let tree: tree::Tree = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&tree).unwrap();
    assert_eq!(expected, actual);
}
