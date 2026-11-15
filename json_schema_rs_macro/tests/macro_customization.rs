//! Integration test: `json_schema_to_rust!`'s `extra_derives`, `extra_attrs` and `patch`
//! settings add derives and attributes to the generated types and rename them, as typify's
//! `derives` and `patch` do.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

mod basket {
    json_schema_rs_macro::json_schema_to_rust!(
        r##"{"title":"Basket","type":"object","properties":{"veg":{"$ref":"#/$defs/Veg"},"code":{"type":"string","minLength":2}},"required":["veg"],"$defs":{"Veg":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}}}"##,
        extra_derives = [schemars::JsonSchema],
        extra_attrs = [must_use],
        patch = { Veg = { rename = "Vegetable", derives = [Hash], attrs = [non_exhaustive] } }
    );
}

/// The hash of `value`.
fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut hasher: DefaultHasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn patch_renames_the_type() {
    let root: basket::schema_0::Basket =
        serde_json::from_value(serde_json::json!({"veg": {"name": "leek"}})).expect("deserialize");
    let expected: &str = "macro_customization::basket::schema_0::Vegetable";
    let actual: &str = std::any::type_name_of_val(&root.veg);
    assert_eq!(expected, actual);
}

#[test]
fn patch_derives_reach_the_patched_type() {
    let a: basket::schema_0::Vegetable =
        serde_json::from_value(serde_json::json!({"name": "leek"})).expect("deserialize");
    let expected: u64 = hash_of(&a.clone());
    let actual: u64 = hash_of(&a);
    assert_eq!(expected, actual);
}

/// schemars names a `serde(transparent)` newtype after the type it wraps.
#[test]
fn extra_derives_reach_every_type() {
    let expected: Vec<String> = vec![
        "Basket".to_string(),
        "String".to_string(),
        "Vegetable".to_string(),
    ];
    let actual: Vec<String> = vec![
        <basket::schema_0::Basket as schemars::JsonSchema>::schema_name(),
        <basket::schema_0::Code as schemars::JsonSchema>::schema_name(),
        <basket::schema_0::Vegetable as schemars::JsonSchema>::schema_name(),
    ];
    assert_eq!(expected, actual);
}
