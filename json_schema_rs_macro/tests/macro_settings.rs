//! Integration test: `json_schema_to_rust!` settings arguments (`name = value` after the schema
//! literals) change the generated code. Each setting is paired with the same schema and instance
//! under the default, so every test shows a difference the setting makes.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Deserializes `instance` as `T` and serializes it back; `None` when `T` rejects it.
fn round_trip<T: DeserializeOwned + Serialize>(
    instance: serde_json::Value,
) -> Option<serde_json::Value> {
    let value: T = serde_json::from_value(instance).ok()?;
    Some(serde_json::to_value(&value).expect("serialize"))
}

/// Whether `instance` deserializes as `T`.
fn accepts<T: DeserializeOwned>(instance: serde_json::Value) -> bool {
    serde_json::from_value::<T>(instance).is_ok()
}

// --- model_name_source ---

mod model_name_property_key_first {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"address":{"title":"Location","type":"object","properties":{"street":{"type":"string"}},"required":["street"]}},"required":["address"]}"#,
        model_name_source = PropertyKeyFirst
    );
}

mod model_name_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"address":{"title":"Location","type":"object","properties":{"street":{"type":"string"}},"required":["street"]}},"required":["address"]}"#
    );
}

#[test]
fn model_name_property_key_first_names_type_after_property_key() {
    let root: model_name_property_key_first::schema_0::Root =
        serde_json::from_value(serde_json::json!({"address": {"street": "Main"}}))
            .expect("deserialize");
    let expected: &str = "macro_settings::model_name_property_key_first::schema_0::Address";
    let actual: &str = std::any::type_name_of_val(&root.address);
    assert_eq!(expected, actual);
}

#[test]
fn model_name_default_names_type_after_title() {
    let root: model_name_default::schema_0::Root =
        serde_json::from_value(serde_json::json!({"address": {"street": "Main"}}))
            .expect("deserialize");
    let expected: &str = "macro_settings::model_name_default::schema_0::Location";
    let actual: &str = std::any::type_name_of_val(&root.address);
    assert_eq!(expected, actual);
}

// --- dedupe_mode ---

mod dedupe_disabled {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Item","type":"object","properties":{"id":{"type":"integer"}}}"#,
        r#"{"title":"Item","type":"object","properties":{"id":{"type":"integer"}}}"#,
        dedupe_mode = Disabled
    );
}

mod dedupe_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Item","type":"object","properties":{"id":{"type":"integer"}}}"#,
        r#"{"title":"Item","type":"object","properties":{"id":{"type":"integer"}}}"#
    );
}

#[test]
fn dedupe_disabled_keeps_identical_schemas_apart() {
    let expected: [&str; 2] = [
        "macro_settings::dedupe_disabled::schema_0::Item",
        "macro_settings::dedupe_disabled::schema_1::Item",
    ];
    let actual: [&str; 2] = [
        std::any::type_name::<dedupe_disabled::schema_0::Item>(),
        std::any::type_name::<dedupe_disabled::schema_1::Item>(),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn dedupe_default_shares_identical_schemas() {
    let expected: [&str; 2] = [
        "macro_settings::dedupe_default::shared::Item",
        "macro_settings::dedupe_default::shared::Item",
    ];
    let actual: [&str; 2] = [
        std::any::type_name::<dedupe_default::schema_0::Item>(),
        std::any::type_name::<dedupe_default::schema_1::Item>(),
    ];
    assert_eq!(expected, actual);
}

// Two objects that differ only in `description`, at different locations: self-validating types
// that validate the same merge wherever they sit, so the dedupe mode decides.

mod dedupe_functional {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"home":{"title":"Address","type":"object","description":"Home address","properties":{"street":{"type":"string"}},"required":["street"]},"work":{"title":"Address","type":"object","description":"Work address","properties":{"street":{"type":"string"}},"required":["street"]}},"required":["home","work"]}"#,
        dedupe_mode = Functional
    );
}

mod dedupe_full {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"home":{"title":"Address","type":"object","description":"Home address","properties":{"street":{"type":"string"}},"required":["street"]},"work":{"title":"Address","type":"object","description":"Work address","properties":{"street":{"type":"string"}},"required":["street"]}},"required":["home","work"]}"#
    );
}

#[test]
fn dedupe_functional_merges_types_differing_only_in_description() {
    let root: dedupe_functional::schema_0::Root = serde_json::from_value(
        serde_json::json!({"home": {"street": "A"}, "work": {"street": "B"}}),
    )
    .expect("deserialize");
    let expected: [&str; 2] = [
        "macro_settings::dedupe_functional::schema_0::Address",
        "macro_settings::dedupe_functional::schema_0::Address",
    ];
    let actual: [&str; 2] = [
        std::any::type_name_of_val(&root.home),
        std::any::type_name_of_val(&root.work),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn dedupe_default_keeps_types_differing_in_description_apart() {
    let root: dedupe_full::schema_0::Root = serde_json::from_value(
        serde_json::json!({"home": {"street": "A"}, "work": {"street": "B"}}),
    )
    .expect("deserialize");
    let expected: [&str; 2] = [
        "macro_settings::dedupe_full::schema_0::Address0",
        "macro_settings::dedupe_full::schema_0::Address1",
    ];
    let actual: [&str; 2] = [
        std::any::type_name_of_val(&root.home),
        std::any::type_name_of_val(&root.work),
    ];
    assert_eq!(expected, actual);
}

// Two different documents that define the same `Address`: one shared type, which validates.
mod dedupe_across_documents {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Order","type":"object","properties":{"id":{"type":"string"},"address":{"title":"Address","type":"object","properties":{"street":{"type":"string","minLength":1}}}}}"#,
        r#"{"title":"Customer","type":"object","properties":{"name":{"type":"string"},"address":{"title":"Address","type":"object","properties":{"street":{"type":"string","minLength":1}}}}}"#
    );
}

#[test]
fn dedupe_default_shares_identical_types_across_documents() {
    let expected: [&str; 2] = [
        "macro_settings::dedupe_across_documents::shared::Address",
        "macro_settings::dedupe_across_documents::shared::Address",
    ];
    let actual: [&str; 2] = [
        std::any::type_name::<dedupe_across_documents::schema_0::Address>(),
        std::any::type_name::<dedupe_across_documents::schema_1::Address>(),
    ];
    assert_eq!(expected, actual);
}

#[test]
fn dedupe_shared_type_across_documents_validates() {
    let expected: bool = false;
    let actual: bool =
        dedupe_across_documents::shared::Address::try_from(serde_json::json!({"street": ""}))
            .is_ok();
    assert_eq!(expected, actual);
}

// --- unknown_properties ---

mod unknown_properties_capture {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}"#,
        unknown_properties = Capture
    );
}

mod unknown_properties_closed_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}"#
    );
}

#[test]
fn unknown_properties_capture_keeps_keys_additional_properties_false_forbids() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"id": "x", "extra": 1}));
    let actual: Option<serde_json::Value> = round_trip::<unknown_properties_capture::schema_0::Root>(
        serde_json::json!({"id": "x", "extra": 1}),
    );
    assert_eq!(expected, actual);
}

#[test]
fn unknown_properties_default_rejects_keys_additional_properties_false_forbids() {
    let expected: Option<serde_json::Value> = None;
    let actual: Option<serde_json::Value> = round_trip::<
        unknown_properties_closed_default::schema_0::Root,
    >(serde_json::json!({"id": "x", "extra": 1}));
    assert_eq!(expected, actual);
}

mod unknown_properties_drop {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        unknown_properties = Drop
    );
}

mod unknown_properties_open_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#
    );
}

#[test]
fn unknown_properties_drop_loses_unknown_keys_on_round_trip() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"id": "x"}));
    let actual: Option<serde_json::Value> = round_trip::<unknown_properties_drop::schema_0::Root>(
        serde_json::json!({"id": "x", "extra": 1}),
    );
    assert_eq!(expected, actual);
}

#[test]
fn unknown_properties_default_keeps_unknown_keys_on_round_trip() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"id": "x", "extra": 1}));
    let actual: Option<serde_json::Value> = round_trip::<
        unknown_properties_open_default::schema_0::Root,
    >(serde_json::json!({"id": "x", "extra": 1}));
    assert_eq!(expected, actual);
}

// --- unique_items ---

mod unique_items_hash_set {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#,
        unique_items = HashSet
    );
}

mod unique_items_btree_set {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#,
        unique_items = BTreeSet
    );
}

#[cfg(feature = "indexmap")]
mod unique_items_index_set {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#,
        unique_items = IndexSet
    );
}

mod unique_items_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#
    );
}

#[test]
fn unique_items_hash_set_rejects_duplicate_item() {
    let expected: bool = false;
    let actual: bool =
        accepts::<unique_items_hash_set::schema_0::Root>(serde_json::json!({"tags": ["a", "a"]}));
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_btree_set_rejects_duplicate_item() {
    let expected: bool = false;
    let actual: bool =
        accepts::<unique_items_btree_set::schema_0::Root>(serde_json::json!({"tags": ["a", "a"]}));
    assert_eq!(expected, actual);
}

#[cfg(feature = "indexmap")]
#[test]
fn unique_items_index_set_rejects_duplicate_item() {
    let expected: bool = false;
    let actual: bool =
        accepts::<unique_items_index_set::schema_0::Root>(serde_json::json!({"tags": ["a", "a"]}));
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_default_deserializes_duplicate_item() {
    let expected: bool = true;
    let actual: bool =
        accepts::<unique_items_default::schema_0::Root>(serde_json::json!({"tags": ["a", "a"]}));
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_btree_set_serializes_items_sorted() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"tags": ["a", "b", "c"]}));
    let actual: Option<serde_json::Value> = round_trip::<unique_items_btree_set::schema_0::Root>(
        serde_json::json!({"tags": ["c", "a", "b"]}),
    );
    assert_eq!(expected, actual);
}

#[cfg(feature = "indexmap")]
#[test]
fn unique_items_index_set_keeps_item_order() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"tags": ["c", "a", "b"]}));
    let actual: Option<serde_json::Value> = round_trip::<unique_items_index_set::schema_0::Root>(
        serde_json::json!({"tags": ["c", "a", "b"]}),
    );
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_default_keeps_item_order() {
    let expected: Option<serde_json::Value> = Some(serde_json::json!({"tags": ["c", "a", "b"]}));
    let actual: Option<serde_json::Value> = round_trip::<unique_items_default::schema_0::Root>(
        serde_json::json!({"tags": ["c", "a", "b"]}),
    );
    assert_eq!(expected, actual);
}

#[test]
fn unique_items_hash_set_holds_items_in_hash_set() {
    let root: unique_items_hash_set::schema_0::Root =
        serde_json::from_value(serde_json::json!({"tags": ["a", "b"]})).expect("deserialize");
    let expected: std::collections::HashSet<String> =
        std::collections::HashSet::from(["a".to_string(), "b".to_string()]);
    let actual: std::collections::HashSet<String> = root.tags;
    assert_eq!(expected, actual);
}

// --- builders ---

mod builders {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Veg","type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1},"weight":{"type":"integer","minimum":0,"maximum":255}}}"#,
        builders = true
    );
}

#[test]
fn builders_build_struct_with_setters() {
    let built: Result<builders::schema_0::Veg, json_schema_rs::BuilderError> =
        builders::schema_0::Veg::builder()
            .name("carrot")
            .weight(3_u8)
            .try_into();
    let expected: Option<serde_json::Value> =
        Some(serde_json::json!({"name": "carrot", "weight": 3}));
    let actual: Option<serde_json::Value> = built
        .ok()
        .map(|veg: builders::schema_0::Veg| serde_json::to_value(veg).expect("serialize"));
    assert_eq!(expected, actual);
}

#[test]
fn builders_reject_value_the_schema_rejects() {
    let built: Result<builders::schema_0::Veg, json_schema_rs::BuilderError> =
        builders::schema_0::Veg::builder().name("").try_into();
    let expected: Option<String> = Some(
        "the value supplied for `name` did not convert: string has 0 code points, minLength is 1"
            .to_string(),
    );
    let actual: Option<String> = built
        .err()
        .map(|error: json_schema_rs::BuilderError| error.to_string());
    assert_eq!(expected, actual);
}

// --- map_type ---

mod map_type_hash_map {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"labels":{"type":"object","additionalProperties":{"type":"integer"}}},"additionalProperties":{"type":"string"}}"#,
        map_type = HashMap
    );
}

#[cfg(feature = "indexmap")]
mod map_type_index_map {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"labels":{"type":"object","additionalProperties":{"type":"integer"}}},"additionalProperties":{"type":"string"}}"#,
        map_type = IndexMap
    );
}

mod map_type_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"labels":{"type":"object","additionalProperties":{"type":"integer"}}},"additionalProperties":{"type":"string"}}"#
    );
}

#[test]
fn map_type_hash_map_holds_entries_in_hash_map() {
    let root: map_type_hash_map::schema_0::Root =
        serde_json::from_value(serde_json::json!({"labels": {"b": 2, "a": 1.0}}))
            .expect("deserialize");
    let expected: Option<std::collections::HashMap<String, i64>> =
        Some(std::collections::HashMap::from([
            ("a".to_string(), 1),
            ("b".to_string(), 2),
        ]));
    let actual: Option<std::collections::HashMap<String, i64>> = root.labels;
    assert_eq!(expected, actual);
}

#[test]
fn map_type_hash_map_holds_unknown_keys_in_hash_map() {
    let root: map_type_hash_map::schema_0::Root =
        serde_json::from_value(serde_json::json!({"x": "1", "y": "2"})).expect("deserialize");
    let expected: std::collections::HashMap<String, String> = std::collections::HashMap::from([
        ("x".to_string(), "1".to_string()),
        ("y".to_string(), "2".to_string()),
    ]);
    let actual: std::collections::HashMap<String, String> = root.additional;
    assert_eq!(expected, actual);
}

#[cfg(feature = "indexmap")]
#[test]
fn map_type_index_map_keeps_key_order() {
    let root: map_type_index_map::schema_0::Root =
        serde_json::from_str(r#"{"labels":{"b":2,"a":1},"z":"1","y":"2"}"#).expect("deserialize");
    let expected: &str = r#"{"labels":{"b":2,"a":1},"z":"1","y":"2"}"#;
    let actual: String = serde_json::to_string(&root).expect("serialize");
    assert_eq!(expected, actual);
}

#[test]
fn map_type_default_sorts_keys() {
    let root: map_type_default::schema_0::Root =
        serde_json::from_str(r#"{"labels":{"b":2,"a":1},"z":"1","y":"2"}"#).expect("deserialize");
    let expected: &str = r#"{"labels":{"a":1,"b":2},"y":"2","z":"1"}"#;
    let actual: String = serde_json::to_string(&root).expect("serialize");
    assert_eq!(expected, actual);
}

// --- flatten_single_entry_combinators ---

mod flatten_single_entry_enabled {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"size":{"oneOf":[{"type":"integer"}]}},"required":["size"]}"#,
        flatten_single_entry_combinators = true
    );
}

mod flatten_single_entry_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"size":{"oneOf":[{"type":"integer"}]}},"required":["size"]}"#
    );
}

#[test]
fn flatten_single_entry_combinators_makes_one_entry_one_of_its_entry_type() {
    let root: flatten_single_entry_enabled::schema_0::Root =
        serde_json::from_value(serde_json::json!({"size": 3})).expect("deserialize");
    let expected: &str = "i64";
    let actual: &str = std::any::type_name_of_val(&root.size);
    assert_eq!(expected, actual);
}

#[test]
fn flatten_single_entry_default_keeps_one_entry_one_of_a_union() {
    let root: flatten_single_entry_default::schema_0::Root =
        serde_json::from_value(serde_json::json!({"size": 3})).expect("deserialize");
    let expected: &str = "macro_settings::flatten_single_entry_default::schema_0::SizeOneOf";
    let actual: &str = std::any::type_name_of_val(&root.size);
    assert_eq!(expected, actual);
}

// --- self_validating ---

mod self_validating_disabled {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"size":{"oneOf":[{"type":"integer","maximum":10},{"type":"integer","minimum":5}]}},"required":["size"]}"#,
        self_validating = false
    );
}

mod self_validating_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"size":{"oneOf":[{"type":"integer","maximum":10},{"type":"integer","minimum":5}]}},"required":["size"]}"#
    );
}

#[test]
fn self_validating_disabled_one_of_takes_first_fitting_variant_when_two_match() {
    let expected: bool = true;
    let actual: bool =
        accepts::<self_validating_disabled::schema_0::Root>(serde_json::json!({"size": 7}));
    assert_eq!(expected, actual);
}

#[test]
fn self_validating_default_one_of_rejects_value_two_branches_match() {
    let expected: bool = false;
    let actual: bool =
        accepts::<self_validating_default::schema_0::Root>(serde_json::json!({"size": 7}));
    assert_eq!(expected, actual);
}

// --- constrained_newtypes ---

mod constrained_newtypes_disabled {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"name":{"type":"string","minLength":2}},"required":["name"]}"#,
        constrained_newtypes = false
    );
}

mod constrained_newtypes_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"name":{"type":"string","minLength":2}},"required":["name"]}"#
    );
}

#[test]
fn constrained_newtypes_disabled_deserializes_value_violating_field_schema() {
    let expected: bool = true;
    let actual: bool =
        accepts::<constrained_newtypes_disabled::schema_0::Root>(serde_json::json!({"name": "A"}));
    assert_eq!(expected, actual);
}

#[test]
fn constrained_newtypes_default_rejects_value_violating_field_schema() {
    let expected: bool = false;
    let actual: bool =
        accepts::<constrained_newtypes_default::schema_0::Root>(serde_json::json!({"name": "A"}));
    assert_eq!(expected, actual);
}

// --- forced_spec_version ---

mod forced_spec_version_draft07 {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#,
        forced_spec_version = Draft07
    );
}

mod forced_spec_version_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#
    );
}

#[test]
fn forced_spec_version_draft07_overrides_schema_and_asserts_format() {
    let expected: bool = false;
    let actual: bool =
        accepts::<forced_spec_version_draft07::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

#[test]
fn forced_spec_version_default_follows_schema_and_annotates_format() {
    let expected: bool = true;
    let actual: bool =
        accepts::<forced_spec_version_default::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

// --- default_spec_version ---

mod default_spec_version_draft07 {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#,
        default_spec_version = Draft07
    );
}

mod default_spec_version_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#
    );
}

#[test]
fn default_spec_version_draft07_asserts_format_without_schema_keyword() {
    let expected: bool = false;
    let actual: bool =
        accepts::<default_spec_version_draft07::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

#[test]
fn default_spec_version_default_annotates_format_without_schema_keyword() {
    let expected: bool = true;
    let actual: bool =
        accepts::<default_spec_version_default::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

// --- format_mode ---

mod format_mode_assertion {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#,
        format_mode = Assertion
    );
}

mod format_mode_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"email":{"type":"string","format":"email"}},"required":["email"]}"#
    );
}

#[test]
fn format_mode_assertion_rejects_value_violating_format() {
    let expected: bool = false;
    let actual: bool =
        accepts::<format_mode_assertion::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

#[test]
fn format_mode_default_accepts_value_violating_format() {
    let expected: bool = true;
    let actual: bool =
        accepts::<format_mode_default::schema_0::Root>(serde_json::json!({"email": "x"}));
    assert_eq!(expected, actual);
}

// --- integer_semantics ---

mod integer_semantics_lexical {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"ratio":{"type":"number","not":{"type":"integer"}}},"required":["ratio"]}"#,
        integer_semantics = Lexical
    );
}

mod integer_semantics_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"ratio":{"type":"number","not":{"type":"integer"}}},"required":["ratio"]}"#
    );
}

#[test]
fn integer_semantics_lexical_reads_one_point_zero_as_not_an_integer() {
    let expected: bool = true;
    let actual: bool =
        accepts::<integer_semantics_lexical::schema_0::Root>(serde_json::json!({"ratio": 1.0}));
    assert_eq!(expected, actual);
}

#[test]
fn integer_semantics_default_reads_one_point_zero_as_an_integer() {
    let expected: bool = false;
    let actual: bool =
        accepts::<integer_semantics_default::schema_0::Root>(serde_json::json!({"ratio": 1.0}));
    assert_eq!(expected, actual);
}

// --- exclusive_bounds ---

// Under the default (Draft 2020-12, numeric bounds) a boolean `exclusiveMinimum` does not parse,
// and a numeric one means the same under both shapes (`BooleanModifier` keeps a number as the
// bound it states), so no schema compiles under both and behaves differently. The pair is the
// boolean modifier's two values, both under `BooleanModifier`, with the same instance.

mod exclusive_bounds_boolean_modifier_true {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"n":{"type":"integer","minimum":5,"exclusiveMinimum":true}},"required":["n"]}"#,
        exclusive_bounds = BooleanModifier
    );
}

mod exclusive_bounds_boolean_modifier_false {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"type":"object","properties":{"n":{"type":"integer","minimum":5,"exclusiveMinimum":false}},"required":["n"]}"#,
        exclusive_bounds = BooleanModifier
    );
}

#[test]
fn exclusive_bounds_boolean_modifier_true_makes_minimum_exclusive() {
    let expected: bool = false;
    let actual: bool = accepts::<exclusive_bounds_boolean_modifier_true::schema_0::Root>(
        serde_json::json!({"n": 5}),
    );
    assert_eq!(expected, actual);
}

#[test]
fn exclusive_bounds_boolean_modifier_false_keeps_minimum_inclusive() {
    let expected: bool = true;
    let actual: bool = accepts::<exclusive_bounds_boolean_modifier_false::schema_0::Root>(
        serde_json::json!({"n": 5}),
    );
    assert_eq!(expected, actual);
}

// --- ref_siblings ---

mod ref_siblings_ignored {
    json_schema_rs_macro::json_schema_to_rust!(
        r##"{"$defs":{"Text":{"type":"string"}},"type":"object","properties":{"name":{"$ref":"#/$defs/Text","maxLength":2}},"required":["name"]}"##,
        ref_siblings = Ignored
    );
}

mod ref_siblings_default {
    json_schema_rs_macro::json_schema_to_rust!(
        r##"{"$defs":{"Text":{"type":"string"}},"type":"object","properties":{"name":{"$ref":"#/$defs/Text","maxLength":2}},"required":["name"]}"##
    );
}

#[test]
fn ref_siblings_ignored_skips_keyword_next_to_ref() {
    let expected: bool = true;
    let actual: bool =
        accepts::<ref_siblings_ignored::schema_0::Root>(serde_json::json!({"name": "abc"}));
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_default_evaluates_keyword_next_to_ref() {
    let expected: bool = false;
    let actual: bool =
        accepts::<ref_siblings_default::schema_0::Root>(serde_json::json!({"name": "abc"}));
    assert_eq!(expected, actual);
}

// --- string_enum_impls ---

mod string_enum_impls {
    json_schema_rs_macro::json_schema_to_rust!(r#"{"title":"Veg","enum":["pea","sweet potato"]}"#);
}

#[test]
fn string_enum_impls_parse_json_value() {
    let expected: Result<string_enum_impls::schema_0::Veg, String> =
        Ok(string_enum_impls::schema_0::Veg::SweetPotato);
    let actual: Result<string_enum_impls::schema_0::Veg, String> = "sweet potato"
        .parse::<string_enum_impls::schema_0::Veg>()
        .map_err(|error: json_schema_rs::ParseEnumError| error.to_string());
    assert_eq!(expected, actual);
}

#[test]
fn string_enum_impls_display_json_value() {
    let expected: String = "sweet potato".to_string();
    let actual: String = string_enum_impls::schema_0::Veg::SweetPotato.to_string();
    assert_eq!(expected, actual);
}

#[test]
fn string_enum_impls_reject_unknown_value() {
    let expected: Result<string_enum_impls::schema_0::Veg, String> =
        Err("unknown variant `kale`, expected `pea` or `sweet potato`".to_string());
    let actual: Result<string_enum_impls::schema_0::Veg, String> = "kale"
        .parse::<string_enum_impls::schema_0::Veg>()
        .map_err(|error: json_schema_rs::ParseEnumError| error.to_string());
    assert_eq!(expected, actual);
}

// --- schema_defaults ---

mod schema_defaults {
    json_schema_rs_macro::json_schema_to_rust!(
        r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string","default":"carrot"},"weight":{"type":"integer","minimum":0,"maximum":255,"default":3}}}"#,
        schema_defaults = true
    );
}

#[test]
fn schema_defaults_struct_default_holds_property_defaults() {
    let expected: serde_json::Value = serde_json::json!({"name": "carrot", "weight": 3});
    let actual: serde_json::Value =
        serde_json::to_value(schema_defaults::schema_0::Veg::default()).expect("serialize");
    assert_eq!(expected, actual);
}
