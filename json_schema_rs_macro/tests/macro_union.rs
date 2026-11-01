//! Integration test: `json_schema_to_rust!` with unions: an `anyOf` of `$ref` branches and a
//! string, a `const`-discriminated `oneOf`, a nullable `anyOf`, and a type array. Instances
//! deserialize from the bare JSON value.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r##"{"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}},"required":["meow"],"additionalProperties":false},"Dog":{"type":"object","properties":{"bark":{"type":"integer"}},"required":["bark"],"additionalProperties":false}},"type":"object","properties":{"pets":{"type":"array","items":{"anyOf":[{"$ref":"#/$defs/Cat"},{"$ref":"#/$defs/Dog"},{"type":"string"}]}}},"required":["pets"]}"##,
    r#"{"type":"object","properties":{"shapes":{"type":"array","items":{"oneOf":[{"title":"Circle","type":"object","properties":{"kind":{"const":"circle"},"radius":{"type":"number"}},"required":["kind","radius"]},{"title":"Square","type":"object","properties":{"kind":{"const":"square"},"side":{"type":"number"}},"required":["kind","side"]}]}}},"required":["shapes"]}"#,
    r#"{"type":"object","properties":{"label":{"anyOf":[{"type":"string"},{"type":"null"}]},"amount":{"type":["integer","number"]}},"required":["label","amount"]}"#
);

#[test]
fn anyof_ref_and_string_branches_pick_matching_variant() {
    let json = r#"{"pets":[{"meow":true},{"bark":3},"goldfish"]}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: Vec<&str> = vec!["cat", "dog", "string"];
    let actual: Vec<&str> = root
        .pets
        .iter()
        .map(|pet| match pet {
            schema_0::PetsAnyOf::Cat(_) => "cat",
            schema_0::PetsAnyOf::Dog(_) => "dog",
            schema_0::PetsAnyOf::String(_) => "string",
        })
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn anyof_ref_and_string_branches_round_trip() {
    let expected: serde_json::Value =
        serde_json::json!({"pets": [{"meow": true}, {"bark": 3}, "goldfish"]});
    let root: schema_0::Root = serde_json::from_value(expected.clone()).unwrap();
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn oneof_const_discriminator_picks_matching_variant() {
    let json = r#"{"shapes":[{"kind":"square","side":2.5},{"kind":"circle","radius":1.5}]}"#;
    let root: schema_1::Root = serde_json::from_str(json).unwrap();
    let expected: Vec<&str> = vec!["square", "circle"];
    let actual: Vec<&str> = root
        .shapes
        .iter()
        .map(|shape| match shape {
            schema_1::ShapesOneOf::Circle(_) => "circle",
            schema_1::ShapesOneOf::Square(_) => "square",
        })
        .collect();
    assert_eq!(expected, actual);
}

#[test]
fn oneof_rejects_instance_matching_no_branch() {
    let json = r#"{"shapes":[{"kind":"triangle","side":1.0}]}"#;
    let expected: bool = true;
    let actual: bool = serde_json::from_str::<schema_1::Root>(json).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn nullable_anyof_is_option_and_type_array_is_union() {
    let null_label: schema_2::Root = serde_json::from_str(r#"{"label":null,"amount":7}"#).unwrap();
    let text_label: schema_2::Root = serde_json::from_str(r#"{"label":"x","amount":7.5}"#).unwrap();
    let kind = |amount: &schema_2::Amount| -> &str {
        match amount {
            schema_2::Amount::Integer(_) => "integer",
            schema_2::Amount::Number(_) => "number",
        }
    };
    let expected: (Option<String>, &str, Option<String>, &str) =
        (None, "integer", Some("x".to_string()), "number");
    let actual: (Option<String>, &str, Option<String>, &str) = (
        null_label.label.clone(),
        kind(&null_label.amount),
        text_label.label.clone(),
        kind(&text_label.amount),
    );
    assert_eq!(expected, actual);
}

#[test]
fn union_types_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, Schema, ToJsonSchema};
    let expected: Vec<JsonSchema> = vec![
        schema_0::PetsAnyOf::json_schema().expect("schema"),
        schema_1::ShapesOneOf::json_schema().expect("schema"),
        schema_2::Amount::json_schema().expect("schema"),
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
