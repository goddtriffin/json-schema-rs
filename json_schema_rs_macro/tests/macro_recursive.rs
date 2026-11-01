//! Integration test: `json_schema_to_rust!` with recursive schemas: a self-reference through `#`
//! and mutual recursion between two `$defs` entries.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["value"]}"##,
    r##"{"$defs":{"Person":{"type":"object","properties":{"name":{"type":"string"},"employer":{"$ref":"#/$defs/Company"}},"required":["name"]},"Company":{"type":"object","properties":{"title":{"type":"string"},"ceo":{"$ref":"#/$defs/Person"}},"required":["title"]}},"type":"object","properties":{"person":{"$ref":"#/$defs/Person"}},"required":["person"]}"##
);

#[test]
fn self_reference_constructs_with_boxed_back_edge() {
    let node = schema_0::Node {
        value: 1,
        next: Some(Box::new(schema_0::Node {
            value: 2,
            next: None,
            children: None,
            additional: BTreeMap::new(),
        })),
        children: None,
        additional: BTreeMap::new(),
    };
    let expected: Option<i64> = Some(2);
    let actual: Option<i64> = node.next.map(|next| next.value);
    assert_eq!(expected, actual);
}

#[test]
fn self_reference_nested_instance_round_trips() {
    let expected: serde_json::Value = serde_json::json!({
        "value": 1,
        "next": {"value": 2, "children": [{"value": 3}]},
        "children": [{"value": 4, "next": {"value": 5}}]
    });
    let node: schema_0::Node = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&node).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn mutual_recursion_nested_instance_round_trips() {
    let expected: serde_json::Value = serde_json::json!({
        "person": {
            "name": "Ada",
            "employer": {"title": "Acme", "ceo": {"name": "Bob", "employer": {"title": "Beta"}}}
        }
    });
    let root: schema_1::Root = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn mutual_recursion_reaches_through_both_types() {
    let json = r#"{"person":{"name":"Ada","employer":{"title":"Acme","ceo":{"name":"Bob"}}}}"#;
    let root: schema_1::Root = serde_json::from_str(json).unwrap();
    let ceo: Option<schema_1::Person> = root
        .person
        .employer
        .and_then(|company: schema_1::Company| company.ceo)
        .map(|ceo| *ceo);
    let expected: Option<String> = Some("Bob".to_string());
    let actual: Option<String> = ceo.map(|person| person.name);
    assert_eq!(expected, actual);
}

#[test]
fn recursive_types_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, Schema, ToJsonSchema};
    let expected: Vec<JsonSchema> = vec![
        schema_0::Node::json_schema().expect("schema"),
        schema_1::Root::json_schema().expect("schema"),
    ];
    let actual: Vec<JsonSchema> = expected
        .iter()
        .map(|schema| {
            let json: String = schema.try_into().expect("serialize");
            Schema::try_from(json.as_str())
                .expect("parse")
                .into_object()
                .expect("schema object")
        })
        .collect();
    assert_eq!(expected, actual);
}
