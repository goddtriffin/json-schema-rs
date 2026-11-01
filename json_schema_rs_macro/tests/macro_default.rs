//! Integration test: `json_schema_to_rust!` with optional properties that have a `default`. The
//! default is an annotation: an absent key stays absent through a round trip, and the generated
//! accessor applies the default.

use json_schema_rs_macro::json_schema_to_rust;

json_schema_to_rust!(
    r#"{"type":"object","properties":{"tabWidth":{"type":"integer","minimum":0,"maximum":255,"default":2},"parser":{"type":"string","default":"babel"}}}"#
);

#[test]
fn default_absent_key_stays_absent_on_round_trip() {
    let expected: serde_json::Value = serde_json::json!({"tabWidth": 4});
    let root: schema_0::Root = serde_json::from_value(expected.clone()).expect("deserialize");
    let actual: serde_json::Value = serde_json::to_value(&root).expect("serialize");
    assert_eq!(expected, actual);
}

#[test]
fn default_accessors_apply_the_default_only_when_absent() {
    let root: schema_0::Root =
        serde_json::from_value(serde_json::json!({"tabWidth": 4})).expect("deserialize");
    let expected: (u8, &str) = (4, "babel");
    let actual: (u8, &str) = (root.tab_width_or_default(), root.parser_or_default());
    assert_eq!(expected, actual);
}
