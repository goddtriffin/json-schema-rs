//! Known-failing regression tests for `#[derive(ToJsonSchema)]`: one or more tests per open
//! bug issue, named `issue_<N>_...`.
//!
//! Each test body asserts the **correct** behaviour. The wrapper inverts it: the test passes
//! while the body still fails, and fails as soon as the body passes ("unexpected pass"). When
//! an issue is fixed, unwrap its tests into plain `#[test]`s. [`known_failure`] mirrors the
//! helper in `json_schema_rs/tests/known_failures.rs` (a separate crate, so it cannot be
//! shared); a bug that aborts the process needs that file's `known_failure_in_child_process`.

use json_schema_rs::json_schema::json_schema::AdditionalProperties;
use json_schema_rs::{JsonSchema, ToJsonSchema};
use json_schema_rs_macro::ToJsonSchema;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// What happened when a known-failing test body ran.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The body completed: the bug is fixed.
    Passed,
    /// The body panicked: the bug is still present.
    Failed,
}

fn unexpected_pass_message(issue: u32) -> String {
    format!(
        "#{issue} now behaves correctly: remove the known-failure wrapper so this runs as a \
         normal test (see design.md, Known failures)"
    )
}

/// Runs `body`, which asserts the correct behaviour for `issue`; passes while `body` panics.
fn known_failure(issue: u32, body: impl FnOnce()) {
    let expected: Outcome = Outcome::Failed;
    let actual: Outcome = match catch_unwind(AssertUnwindSafe(body)) {
        Ok(()) => Outcome::Passed,
        Err(_) => Outcome::Failed,
    };
    assert_eq!(expected, actual, "{}", unexpected_pass_message(issue));
}

fn ref_to(name: &str) -> JsonSchema {
    JsonSchema {
        ref_: Some(format!("#/$defs/{name}")),
        ..Default::default()
    }
}

// --- #24: $defs keyed by last path segment, so same-name types collide ---

mod first {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Config {
        name: String,
    }
}

mod second {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Config {
        port: i64,
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct TwoConfigs {
    a: first::Config,
    b: second::Config,
}

#[test]
fn issue_24_same_name_types_get_suffixed_defs_entries() {
    known_failure(24, || {
        let expected: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            type_: Some("object".to_string()),
            defs: Some(BTreeMap::from([
                ("Config0".to_string(), first::Config::json_schema()),
                ("Config1".to_string(), second::Config::json_schema()),
            ])),
            properties: BTreeMap::from([
                ("a".to_string(), ref_to("Config0")),
                ("b".to_string(), ref_to("Config1")),
            ]),
            additional_properties: Some(AdditionalProperties::Forbid),
            required: Some(vec!["a".to_string(), "b".to_string()]),
            ..Default::default()
        };
        let actual: JsonSchema = TwoConfigs::json_schema();
        assert_eq!(expected, actual);
    });
}

#[cfg(feature = "uuid")]
mod user {
    use json_schema_rs_macro::ToJsonSchema;

    #[derive(ToJsonSchema)]
    #[expect(dead_code)]
    pub(super) struct Uuid {
        value: String,
    }
}

#[cfg(feature = "uuid")]
#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct WithUserUuid {
    id: user::Uuid,
}

#[cfg(feature = "uuid")]
#[test]
fn issue_24_user_type_named_uuid_gets_defs_entry() {
    known_failure(24, || {
        let expected: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            type_: Some("object".to_string()),
            defs: Some(BTreeMap::from([(
                "Uuid".to_string(),
                user::Uuid::json_schema(),
            )])),
            properties: BTreeMap::from([("id".to_string(), ref_to("Uuid"))]),
            additional_properties: Some(AdditionalProperties::Forbid),
            required: Some(vec!["id".to_string()]),
            ..Default::default()
        };
        let actual: JsonSchema = WithUserUuid::json_schema();
        assert_eq!(expected, actual);
    });
}
