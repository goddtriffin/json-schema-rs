//! Integration test: `json_schema_to_rust!` with typed formats. `ipv4` / `ipv6` are the
//! `std::net` address types; `date-time` / `date` are `chrono` types (the `chrono` feature).
//! Where the dialect asserts formats, the field is a newtype checked by the validator before
//! it is parsed.

mod annotated {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"v4":{"type":"string","format":"ipv4"},"v6":{"type":"string","format":"ipv6"}},"required":["v4"]}"#
    );
}

mod asserted {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","properties":{"v4":{"type":"string","format":"ipv4"}},"required":["v4"]}"#
    );
}

#[cfg(feature = "chrono")]
mod dates {
    use json_schema_rs_macro::json_schema_to_rust;

    json_schema_to_rust!(
        r#"{"type":"object","properties":{"at":{"type":"string","format":"date-time"},"on":{"type":"string","format":"date"}},"required":["at"]}"#
    );
}

#[test]
fn ip_formats_round_trip() {
    let instance: serde_json::Value = serde_json::json!({"v4": "127.0.0.1", "v6": "::1"});
    let root: annotated::schema_0::Root = serde_json::from_value(instance.clone()).unwrap();
    let expected: serde_json::Value = instance;
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn ipv4_field_is_an_ipv4_addr() {
    let root: annotated::schema_0::Root = serde_json::from_str(r#"{"v4":"127.0.0.1"}"#).unwrap();
    let expected: std::net::Ipv4Addr = std::net::Ipv4Addr::LOCALHOST;
    let actual: std::net::Ipv4Addr = root.v4;
    assert_eq!(expected, actual);
}

#[test]
fn ip_format_rejects_a_non_address() {
    let expected: bool = true;
    let actual: bool =
        serde_json::from_str::<annotated::schema_0::Root>(r#"{"v4":"1.2.3"}"#).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn asserted_ip_format_is_checked_then_parsed() {
    let root: asserted::schema_0::Root = serde_json::from_str(r#"{"v4":"10.0.0.1"}"#).unwrap();
    let expected: std::net::Ipv4Addr = std::net::Ipv4Addr::new(10, 0, 0, 1);
    let actual: std::net::Ipv4Addr = *root.v4.as_ref();
    assert_eq!(expected, actual);
}

#[cfg(feature = "chrono")]
#[test]
fn date_time_keeps_its_offset() {
    let instance: serde_json::Value =
        serde_json::json!({"at": "2024-01-05T10:00:00+02:00", "on": "2024-01-05"});
    let root: dates::schema_0::Root = serde_json::from_value(instance.clone()).unwrap();
    let expected: serde_json::Value = instance;
    let actual: serde_json::Value = serde_json::to_value(&root).unwrap();
    assert_eq!(expected, actual);
}

#[cfg(feature = "chrono")]
#[test]
fn date_field_is_a_naive_date() {
    let root: dates::schema_0::Root =
        serde_json::from_str(r#"{"at":"2024-01-05T10:00:00Z","on":"2024-01-05"}"#).unwrap();
    let expected: Option<chrono::NaiveDate> = chrono::NaiveDate::from_ymd_opt(2024, 1, 5);
    let actual: Option<chrono::NaiveDate> = root.on;
    assert_eq!(expected, actual);
}
