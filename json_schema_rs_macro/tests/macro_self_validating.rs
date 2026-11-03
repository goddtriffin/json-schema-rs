//! Integration test: `json_schema_to_rust!` generates self-validating types by default:
//! `validate`, `TryFrom<serde_json::Value>`, and `oneOf` / `anyOf` variants chosen by validation.

use json_schema_rs_macro::json_schema_to_rust;
use schema_0::{Root, SizeOneOf};

json_schema_to_rust!(
    r#"{"type":"object","properties":{"name":{"type":"string","minLength":2},"size":{"oneOf":[{"type":"integer","maximum":10},{"type":"integer","minimum":5}]}},"required":["name"],"additionalProperties":false}"#
);

#[test]
fn validate_reports_a_constraint_the_rust_type_does_not_hold() {
    let root: Root = Root {
        name: "A".to_string(),
        size: None,
    };
    let expected: bool = true;
    let actual: bool = root.validate().is_err();
    assert_eq!(expected, actual);
}

#[test]
fn validate_accepts_a_conforming_value() {
    let root: Root = Root {
        name: "Ada".to_string(),
        size: None,
    };
    let expected: bool = true;
    let actual: bool = root.validate().is_ok();
    assert_eq!(expected, actual);
}

#[test]
fn try_from_rejects_a_value_violating_the_schema() {
    let expected: bool = true;
    let actual: bool = Root::try_from(serde_json::json!({"name": "A"})).is_err();
    assert_eq!(expected, actual);
}

#[test]
fn one_of_picks_the_only_matching_branch_and_rejects_two_matches() {
    let parse = |size: i64| -> Option<&'static str> {
        let root: Root =
            serde_json::from_value(serde_json::json!({"name": "Ada", "size": size})).ok()?;
        root.size.map(|size| match size {
            SizeOneOf::Integer0(_) => "first",
            SizeOneOf::Integer1(_) => "second",
        })
    };
    let expected: [Option<&str>; 3] = [Some("first"), Some("second"), None];
    let actual: [Option<&str>; 3] = [parse(3), parse(12), parse(7)];
    assert_eq!(expected, actual);
}
