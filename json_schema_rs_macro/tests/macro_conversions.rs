//! Integration test: `json_schema_to_rust!`'s `convert` and `replace` settings put the user's
//! types in the generated code, as typify's `convert` and `replace` do. The self-validating
//! types validate the JSON before converting it.

use json_schema_rs::ToJsonSchema as _;
use std::str::FromStr as _;

/// A tag type the user owns, in place of the generated `Tag`.
pub mod my {
    #[derive(
        Debug,
        Clone,
        PartialEq,
        serde::Serialize,
        serde::Deserialize,
        json_schema_rs_macro::ToJsonSchema,
    )]
    pub struct Tag {
        pub label: String,
    }
}

mod invoice {
    json_schema_rs_macro::json_schema_to_rust!(
        r##"{"title":"Invoice","type":"object","properties":{"total":{"type":"string","format":"decimal","pattern":"^[0-9]+\\.[0-9]{2}$"},"tag":{"$ref":"#/$defs/Tag"}},"required":["total"],"$defs":{"Tag":{"type":"object","properties":{"label":{"type":"string"}},"required":["label"]}}}"##,
        convert = { {"type":"string","format":"decimal","pattern":"^[0-9]+\\.[0-9]{2}$"} => rust_decimal::Decimal },
        replace = { Tag = crate::my::Tag }
    );
}

#[test]
fn convert_deserializes_into_the_users_type() {
    let invoice: invoice::schema_0::Invoice =
        serde_json::from_value(serde_json::json!({"total": "12.50", "tag": {"label": "food"}}))
            .expect("deserialize");
    let expected: (rust_decimal::Decimal, Option<my::Tag>) = (
        rust_decimal::Decimal::from_str("12.50").expect("decimal"),
        Some(my::Tag {
            label: "food".to_string(),
        }),
    );
    let actual: (rust_decimal::Decimal, Option<my::Tag>) = (invoice.total, invoice.tag);
    assert_eq!(expected, actual);
}

/// `12.5` is a decimal but fails the schema's `pattern`: `TryFrom` validates the JSON before
/// it is converted.
#[test]
fn convert_validates_the_json_before_converting() {
    let expected: bool = true;
    let actual: bool =
        invoice::schema_0::Invoice::try_from(serde_json::json!({"total": "12.5"})).is_err();
    assert_eq!(expected, actual);
}

/// Without the validation, `12.5` converts.
#[test]
fn convert_without_validation_takes_what_the_users_type_takes() {
    let expected: bool = true;
    let actual: bool =
        serde_json::from_value::<invoice::schema_0::Invoice>(serde_json::json!({"total": "12.5"}))
            .is_ok();
    assert_eq!(expected, actual);
}

#[test]
fn convert_keeps_the_subschema_in_the_reverse_schema() {
    let schema: serde_json::Value =
        serde_json::to_value(invoice::schema_0::Invoice::json_schema().expect("json schema"))
            .expect("to value");
    let expected: serde_json::Value = serde_json::json!({
        "format": "decimal",
        "pattern": "^[0-9]+\\.[0-9]{2}$",
        "type": "string"
    });
    let actual: serde_json::Value = schema["properties"]["total"].clone();
    assert_eq!(expected, actual);
}
