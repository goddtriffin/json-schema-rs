//! Integration test: `#[derive(json_schema_rs_macro::Deserialize)]` on hand-written structs, for
//! every serde attribute it accepts and its absence: present, absent, repeated and unknown keys,
//! `deny_unknown_fields`, `rename_all`, `rename`, `default`, `deserialize_with` (with and without `default`),
//! `flatten` (with and without `deserialize_with`), deprecated fields, input that is not an
//! object, a struct wide enough for the derive's other expansion, and `Self` in a field type. Errors are serde's derive's, word for word.

use std::collections::{BTreeMap, HashMap};

/// Reads an `i64` and doubles it, so a test can tell that the value went through this function.
fn doubled<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    <i64 as serde::Deserialize>::deserialize(deserializer).map(|value: i64| value * 2)
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
struct Plain {
    name: String,
    #[serde(rename = "age-years", skip_serializing_if = "Option::is_none")]
    age_years: Option<u8>,
    r#type: Option<bool>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Camel {
    log_level: String,
    #[serde(rename = "max-retries")]
    max_retries: u8,
    r#type: Option<bool>,
    #[serde(flatten)]
    additional_keys: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
#[serde(deny_unknown_fields)]
struct Closed {
    a: u8,
    b: Option<u8>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
struct Helpers {
    #[serde(deserialize_with = "doubled")]
    with: i64,
    #[serde(default, deserialize_with = "doubled")]
    with_default: i64,
    #[serde(default)]
    plain_default: Vec<u8>,
    #[serde(deserialize_with = "Option::deserialize")]
    required_nullable: Option<u8>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
struct Open {
    name: Option<String>,
    #[serde(flatten)]
    additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
struct OpenWith {
    #[serde(flatten, deserialize_with = "doubled")]
    rest: HashMap<String, i64>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
#[deprecated = "use something else"]
struct Old {
    #[deprecated]
    value: u8,
}

/// More fields than the derive builds by moving its slots (64), so it takes their values out
/// one by one: the same semantics, another expansion.
#[derive(Debug, Default, PartialEq, json_schema_rs_macro::Deserialize)]
struct Wide {
    name: String,
    #[serde(deserialize_with = "doubled")]
    with: i64,
    #[serde(default)]
    plain_default: Vec<u8>,
    #[serde(rename = "f-03")]
    f03: Option<u8>,
    f04: Option<u8>,
    f05: Option<u8>,
    f06: Option<u8>,
    f07: Option<u8>,
    f08: Option<u8>,
    f09: Option<u8>,
    f10: Option<u8>,
    f11: Option<u8>,
    f12: Option<u8>,
    f13: Option<u8>,
    f14: Option<u8>,
    f15: Option<u8>,
    f16: Option<u8>,
    f17: Option<u8>,
    f18: Option<u8>,
    f19: Option<u8>,
    f20: Option<u8>,
    f21: Option<u8>,
    f22: Option<u8>,
    f23: Option<u8>,
    f24: Option<u8>,
    f25: Option<u8>,
    f26: Option<u8>,
    f27: Option<u8>,
    f28: Option<u8>,
    f29: Option<u8>,
    f30: Option<u8>,
    f31: Option<u8>,
    f32: Option<u8>,
    f33: Option<u8>,
    f34: Option<u8>,
    f35: Option<u8>,
    f36: Option<u8>,
    f37: Option<u8>,
    f38: Option<u8>,
    f39: Option<u8>,
    f40: Option<u8>,
    f41: Option<u8>,
    f42: Option<u8>,
    f43: Option<u8>,
    f44: Option<u8>,
    f45: Option<u8>,
    f46: Option<u8>,
    f47: Option<u8>,
    f48: Option<u8>,
    f49: Option<u8>,
    f50: Option<u8>,
    f51: Option<u8>,
    f52: Option<u8>,
    f53: Option<u8>,
    f54: Option<u8>,
    f55: Option<u8>,
    f56: Option<u8>,
    f57: Option<u8>,
    f58: Option<u8>,
    f59: Option<u8>,
    f60: Option<u8>,
    f61: Option<u8>,
    f62: Option<u8>,
    f63: Option<u8>,
    f64: Option<u8>,
    f65: Option<u8>,
}

#[derive(Debug, PartialEq, json_schema_rs_macro::Deserialize)]
struct Node {
    value: u8,
    next: Option<Box<Self>>,
}

/// The result of deserializing `json` into `T`, the error as text.
fn read<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str::<T>(json).map_err(|error: serde_json::Error| error.to_string())
}

#[test]
fn present_keys_are_read_into_their_fields() {
    let expected: Result<Plain, String> = Ok(Plain {
        name: "Rex".to_string(),
        age_years: Some(3),
        r#type: Some(true),
    });
    let actual: Result<Plain, String> = read(r#"{"name": "Rex", "age-years": 3, "type": true}"#);
    assert_eq!(expected, actual);
}

#[test]
fn absent_option_key_is_none() {
    let expected: Result<Plain, String> = Ok(Plain {
        name: "Rex".to_string(),
        age_years: None,
        r#type: None,
    });
    let actual: Result<Plain, String> = read(r#"{"name": "Rex"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn absent_required_key_is_missing_field() {
    let expected: Result<Plain, String> =
        Err("missing field `name` at line 1 column 16".to_string());
    let actual: Result<Plain, String> = read(r#"{"age-years": 3}"#);
    assert_eq!(expected, actual);
}

#[test]
fn renamed_field_does_not_read_its_rust_name() {
    let expected: Result<Plain, String> = Ok(Plain {
        name: "Rex".to_string(),
        age_years: None,
        r#type: None,
    });
    let actual: Result<Plain, String> = read(r#"{"name": "Rex", "age_years": 3}"#);
    assert_eq!(expected, actual);
}

#[test]
fn repeated_key_is_duplicate_field() {
    let expected: Result<Plain, String> =
        Err("duplicate field `name` at line 1 column 20".to_string());
    let actual: Result<Plain, String> = read(r#"{"name": "a", "name": "b"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn unknown_key_is_ignored() {
    let expected: Result<Plain, String> = Ok(Plain {
        name: "Rex".to_string(),
        age_years: None,
        r#type: None,
    });
    let actual: Result<Plain, String> =
        read(r#"{"extra": {"deep": [1, 2.5, null]}, "name": "Rex"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn deny_unknown_fields_rejects_unknown_key() {
    let expected: Result<Closed, String> =
        Err("unknown field `c`, expected `a` or `b` at line 1 column 12".to_string());
    let actual: Result<Closed, String> = read(r#"{"a": 1, "c": 2}"#);
    assert_eq!(expected, actual);
}

#[test]
fn deny_unknown_fields_accepts_known_keys() {
    let expected: Result<Closed, String> = Ok(Closed { a: 1, b: Some(2) });
    let actual: Result<Closed, String> = read(r#"{"a": 1, "b": 2}"#);
    assert_eq!(expected, actual);
}

#[test]
fn deserialize_with_reads_through_its_function() {
    let expected: Result<Helpers, String> = Ok(Helpers {
        with: 2,
        with_default: 4,
        plain_default: vec![5],
        required_nullable: None,
    });
    let actual: Result<Helpers, String> =
        read(r#"{"with": 1, "with_default": 2, "plain_default": [5], "required_nullable": null}"#);
    assert_eq!(expected, actual);
}

#[test]
fn absent_default_keys_are_their_defaults() {
    let expected: Result<Helpers, String> = Ok(Helpers {
        with: 2,
        with_default: 0,
        plain_default: Vec::new(),
        required_nullable: Some(7),
    });
    let actual: Result<Helpers, String> = read(r#"{"with": 1, "required_nullable": 7}"#);
    assert_eq!(expected, actual);
}

#[test]
fn absent_deserialize_with_key_without_default_is_missing_field() {
    let expected: Result<Helpers, String> =
        Err("missing field `with` at line 1 column 27".to_string());
    let actual: Result<Helpers, String> = read(r#"{"required_nullable": null}"#);
    assert_eq!(expected, actual);
}

#[test]
fn absent_nullable_option_through_deserialize_with_is_missing_field() {
    let expected: Result<Helpers, String> =
        Err("missing field `required_nullable` at line 1 column 11".to_string());
    let actual: Result<Helpers, String> = read(r#"{"with": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn flatten_collects_unknown_keys_last_value_wins() {
    let expected: Result<Open, String> = Ok(Open {
        name: Some("a".to_string()),
        additional: BTreeMap::from([
            ("x".to_string(), serde_json::json!(2)),
            ("y".to_string(), serde_json::json!([true])),
        ]),
    });
    let actual: Result<Open, String> = read(r#"{"x": 1, "name": "a", "y": [true], "x": 2}"#);
    assert_eq!(expected, actual);
}

#[test]
fn flatten_without_unknown_keys_is_empty() {
    let expected: Result<Open, String> = Ok(Open {
        name: None,
        additional: BTreeMap::new(),
    });
    let actual: Result<Open, String> = read("{}");
    assert_eq!(expected, actual);
}

#[test]
fn flatten_repeated_known_key_is_duplicate_field() {
    let expected: Result<Open, String> =
        Err("duplicate field `name` at line 1 column 20".to_string());
    let actual: Result<Open, String> = read(r#"{"name": "a", "name": "b"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn flatten_with_deserialize_with_reads_each_value_through_it() {
    let expected: Result<OpenWith, String> = Ok(OpenWith {
        rest: HashMap::from([("a".to_string(), 2), ("b".to_string(), 6)]),
    });
    let actual: Result<OpenWith, String> = read(r#"{"a": 1, "b": 3}"#);
    assert_eq!(expected, actual);
}

#[test]
fn flatten_value_of_wrong_type_is_its_error() {
    let expected: Result<OpenWith, String> =
        Err("invalid type: string \"x\", expected i64 at line 1 column 9".to_string());
    let actual: Result<OpenWith, String> = read(r#"{"a": "x"}"#);
    assert_eq!(expected, actual);
}

#[test]
#[expect(deprecated)]
fn deprecated_struct_and_field_are_read() {
    let expected: Result<Old, String> = Ok(Old { value: 1 });
    let actual: Result<Old, String> = read(r#"{"value": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn array_is_not_a_struct() {
    let expected: Result<Plain, String> =
        Err("invalid type: sequence, expected struct Plain at line 1 column 0".to_string());
    let actual: Result<Plain, String> = read(r#"["Rex", 3, true]"#);
    assert_eq!(expected, actual);
}

#[test]
fn string_is_not_a_struct() {
    let expected: Result<Open, String> =
        Err("invalid type: string \"a\", expected struct Open at line 1 column 3".to_string());
    let actual: Result<Open, String> = read(r#""a""#);
    assert_eq!(expected, actual);
}

#[test]
fn reads_from_a_value() {
    let expected: Result<Plain, String> = Ok(Plain {
        name: "Rex".to_string(),
        age_years: Some(3),
        r#type: None,
    });
    let actual: Result<Plain, String> =
        serde_json::from_value::<Plain>(serde_json::json!({"name": "Rex", "age-years": 3}))
            .map_err(|error: serde_json::Error| error.to_string());
    assert_eq!(expected, actual);
}

#[test]
fn wide_struct_reads_present_keys_into_their_fields() {
    let expected: Result<Wide, String> = Ok(Wide {
        name: "Rex".to_string(),
        with: 8,
        f03: Some(3),
        f65: Some(65),
        ..Wide::default()
    });
    let actual: Result<Wide, String> =
        read(r#"{"f65": 65, "name": "Rex", "with": 4, "f-03": 3, "unknown": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn wide_struct_absent_required_key_is_missing_field() {
    let expected: Result<Wide, String> =
        Err("missing field `name` at line 1 column 11".to_string());
    let actual: Result<Wide, String> = read(r#"{"with": 4}"#);
    assert_eq!(expected, actual);
}

#[test]
fn wide_struct_absent_deserialize_with_key_is_missing_field() {
    let expected: Result<Wide, String> =
        Err("missing field `with` at line 1 column 15".to_string());
    let actual: Result<Wide, String> = read(r#"{"name": "Rex"}"#);
    assert_eq!(expected, actual);
}

#[test]
fn wide_struct_repeated_key_is_duplicate_field() {
    let expected: Result<Wide, String> =
        Err("duplicate field `f65` at line 1 column 16".to_string());
    let actual: Result<Wide, String> = read(r#"{"f65": 1, "f65": 2}"#);
    assert_eq!(expected, actual);
}

#[test]
fn self_in_a_field_type_is_the_struct() {
    let expected: Result<Node, String> = Ok(Node {
        value: 1,
        next: Some(Box::new(Node {
            value: 2,
            next: None,
        })),
    });
    let actual: Result<Node, String> = read(r#"{"value": 1, "next": {"value": 2}}"#);
    assert_eq!(expected, actual);
}

#[test]
fn rename_all_names_fields_without_their_own_rename() {
    let expected: Result<Camel, String> = Ok(Camel {
        log_level: "info".to_string(),
        max_retries: 2,
        r#type: Some(true),
        additional_keys: BTreeMap::from([("log_level".to_string(), serde_json::json!(1))]),
    });
    let actual: Result<Camel, String> =
        read(r#"{"logLevel": "info", "max-retries": 2, "type": true, "log_level": 1}"#);
    assert_eq!(expected, actual);
}

#[test]
fn rename_all_reads_the_same_keys_as_serde() {
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SerdeCamel {
        log_level: String,
        #[serde(rename = "max-retries")]
        max_retries: u8,
        r#type: Option<bool>,
        #[serde(flatten)]
        additional_keys: BTreeMap<String, serde_json::Value>,
    }
    let json: &str = r#"{"logLevel": "info", "max-retries": 2, "type": true, "maxRetries": 9}"#;
    let expected: Result<SerdeCamel, String> =
        serde_json::from_str::<SerdeCamel>(json).map_err(|error| error.to_string());
    let actual: Result<SerdeCamel, String> = read::<Camel>(json).map(|camel: Camel| SerdeCamel {
        log_level: camel.log_level,
        max_retries: camel.max_retries,
        r#type: camel.r#type,
        additional_keys: camel.additional_keys,
    });
    assert_eq!(expected, actual);
}
