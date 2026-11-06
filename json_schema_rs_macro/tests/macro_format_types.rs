//! Integration test: `json_schema_to_rust!` `format_types` setting. By default `format: "uuid"`
//! is a `Uuid` even where the dialect only annotates formats (2019-09 and 2020-12, a deliberate
//! divergence from those drafts); `format_types = FollowDialect` keeps such a field a `String`
//! unless the dialect asserts formats. Only compiled with the `uuid` feature.
#![cfg(feature = "uuid")]

mod typed {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#
    );
}

mod follow_dialect {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#,
        format_types = FollowDialect,
    );
}

mod follow_dialect_asserted {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#,
        format_types = FollowDialect,
        format_mode = Assertion,
    );
}

#[test]
fn typed_default_holds_uuid() {
    let root: typed::schema_0::Root =
        serde_json::from_str(r#"{"id":"550e8400-e29b-41d4-a716-446655440000"}"#).unwrap();
    let expected: Option<uuid::Uuid> =
        Some(uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap());
    let actual: Option<uuid::Uuid> = root.id;
    assert_eq!(expected, actual);
}

#[test]
fn typed_default_rejects_non_uuid_string() {
    let expected: bool = true;
    let actual: bool =
        serde_json::from_str::<typed::schema_0::Root>(r#"{"id":"not-a-uuid"}"#).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn follow_dialect_annotation_keeps_any_string() {
    let root: follow_dialect::schema_0::Root =
        serde_json::from_str(r#"{"id":"not-a-uuid"}"#).unwrap();
    let expected: Option<String> = Some("not-a-uuid".to_string());
    let actual: Option<String> = root.id;
    assert_eq!(expected, actual);
}

#[test]
fn follow_dialect_assertion_holds_uuid() {
    let root: follow_dialect_asserted::schema_0::Root =
        serde_json::from_str(r#"{"id":"550e8400-e29b-41d4-a716-446655440000"}"#).unwrap();
    let expected: Option<uuid::Uuid> =
        Some(uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap());
    let actual: Option<uuid::Uuid> = root.id;
    assert_eq!(expected, actual);
}
