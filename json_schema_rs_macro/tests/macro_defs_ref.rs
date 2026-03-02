//! Integration test: `json_schema_to_rust!` with `$defs` + `$ref`.

use json_schema_rs_macro::json_schema_to_rust;

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
        },
    };
    let expected = "NYC";
    let actual = root.address.city.as_str();
    assert_eq!(expected, actual);
}

#[test]
fn defs_ref_deserializes() {
    let json = r#"{"address":{"city":"Boston"}}"#;
    let actual: schema_0::Root = serde_json::from_str(json).unwrap();
    // Generated structs do not implement PartialEq; match full shape and value.
    assert!(matches!(
        actual,
        schema_0::Root {
            address: schema_0::Address { city: ref c }
        } if c == "Boston"
    ));
}

#[test]
fn defs_ref_each_type_json_schema_round_trip() {
    use json_schema_rs::{JsonSchema, ToJsonSchema};
    let root_schema = schema_0::Root::json_schema();
    let json: String = (&root_schema).try_into().expect("serialize");
    let reparsed: json_schema_rs::JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
    assert_eq!(root_schema, reparsed, "Root::json_schema() round-trip");
}
