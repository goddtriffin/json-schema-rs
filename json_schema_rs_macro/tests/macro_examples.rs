//! Integration test: `json_schema_to_rust!` with a schema that has the `examples` keyword (meta-data; annotation only).

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":["foo"]}"#
);

#[test]
fn examples_annotation_direct_construction() {
    let root: schema_0::Root = schema_0::Root {
        x: String::from("foo"),
    };
    let expected: &str = "foo";
    let actual: &str = root.x.as_str();
    assert_eq!(expected, actual);
}

#[test]
fn examples_annotation_deserializes() {
    let parsed: schema_0::Root = serde_json::from_str(r#"{"x":"bar"}"#).unwrap();
    let expected: &str = "bar";
    let actual: &str = parsed.x.as_str();
    assert_eq!(expected, actual);
}
