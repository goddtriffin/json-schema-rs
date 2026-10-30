//! Integration test: `json_schema_to_rust!` with `$defs` + `$ref`.

use json_schema_rs_macro::json_schema_to_rust;
use std::collections::BTreeMap;

json_schema_to_rust!(
    r##"{
  "$defs": {
    "Address": {
      "type": "object",
      "properties": { "city": { "type": "string" } },
      "required": ["city"]
    }
  },
  "type": "object",
  "properties": { "address": { "$ref": "#/$defs/Address" } },
  "required": ["address"]
}"##
);

#[test]
fn expanded_code_compiles_and_ref_type_exists() {
    let root = schema_0::Root {
        address: schema_0::Address {
            city: String::from("NYC"),
            additional: BTreeMap::new(),
        },
        additional: BTreeMap::new(),
    };
    let expected = "NYC";
    let actual = root.address.city.as_str();
    assert_eq!(expected, actual);
}

#[test]
fn defs_ref_deserializes() {
    let json = r#"{"address":{"city":"Boston"}}"#;
    let root: schema_0::Root = serde_json::from_str(json).unwrap();
    let expected: serde_json::Value = serde_json::json!({"address": {"city": "Boston"}});
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn defs_ref_each_type_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, ToJsonSchema};
    let root_schema: JsonSchema = schema_0::Root::json_schema().expect("schema");
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
