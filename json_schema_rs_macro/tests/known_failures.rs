//! Known-failing regression tests for `#[derive(ToJsonSchema)]`: one or more tests per open
//! bug issue, named `issue_<N>_...`.
//!
//! Each test body asserts the **correct** behaviour. The wrapper inverts it: the test passes
//! while the body still fails, and fails as soon as the body passes ("unexpected pass"). When
//! an issue is fixed, unwrap its tests into plain `#[test]`s. The helpers mirror
//! `json_schema_rs/tests/known_failures.rs` (a separate crate, so they cannot be shared).
//!
//! - [`known_failure`] runs the body in-process and treats a panic as "still failing".
//! - [`known_failure_in_child_process`] is for bugs that abort the process (stack overflow): it
//!   re-runs this one test in a child process with a timeout.

use json_schema_rs::json_schema::json_schema::AdditionalProperties;
use json_schema_rs::{JsonSchema, ToJsonSchema};
use json_schema_rs_macro::ToJsonSchema;
use std::collections::BTreeMap;
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Environment variable naming the test a child process runs for real.
const CHILD_ENV: &str = "JSON_SCHEMA_RS_KNOWN_FAILURE_CHILD";
/// Printed by a child process when the test body completed.
const CHILD_PASSED_MARKER: &str = "json-schema-rs known-failure child passed";
/// How long a child process may run before it counts as hung.
const CHILD_TIMEOUT: Duration = Duration::from_secs(10);

/// What happened when a known-failing test body ran.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The body completed: the bug is fixed.
    Passed,
    /// The body panicked, crashed, or timed out: the bug is still present.
    Failed,
    /// The child process exited cleanly without running the body (wrong test name).
    NotRun,
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

/// Like [`known_failure`], but runs `body` in a child process (this test binary filtered to
/// `test_name`), so a stack overflow or a hang fails only the child.
fn known_failure_in_child_process(issue: u32, test_name: &str, body: impl FnOnce()) {
    if std::env::var(CHILD_ENV).is_ok_and(|name| name == test_name) {
        body();
        println!("{CHILD_PASSED_MARKER}");
        return;
    }
    let expected: Outcome = Outcome::Failed;
    let actual: Outcome = run_child(test_name);
    assert_eq!(expected, actual, "{}", unexpected_pass_message(issue));
}

fn run_child(test_name: &str) -> Outcome {
    let exe = std::env::current_exe().expect("path of the running test binary");
    let mut child: Child = Command::new(exe)
        .args([test_name, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, test_name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn child test process");
    let deadline: Instant = Instant::now() + CHILD_TIMEOUT;
    let status: Option<ExitStatus> = loop {
        if let Some(status) = child.try_wait().expect("poll child test process") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            child.kill().expect("kill hung child test process");
            child.wait().expect("reap killed child test process");
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stdout: String = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .expect("read child test process output");
    }
    match status {
        Some(status) if status.success() && stdout.contains(CHILD_PASSED_MARKER) => Outcome::Passed,
        Some(status) if status.success() => Outcome::NotRun,
        Some(_) | None => Outcome::Failed,
    }
}

fn ref_to(name: &str) -> JsonSchema {
    JsonSchema {
        ref_: Some(format!("#/$defs/{name}")),
        ..Default::default()
    }
}

// --- #4: mutually recursive types overflow the stack ---

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct MutualA {
    bs: Vec<MutualB>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct MutualB {
    as_: Vec<MutualA>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleA {
    bs: Vec<CycleB>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleB {
    cs: Vec<CycleC>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct CycleC {
    as_: Vec<CycleA>,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct OverriddenNode {
    #[json_schema(min_items = 1)]
    children: Vec<OverriddenNode>,
}

// These tests check termination only; #4 adds the golden `$defs` output.

#[test]
fn issue_4_derive_terminates_on_mutual_recursion() {
    known_failure_in_child_process(4, "issue_4_derive_terminates_on_mutual_recursion", || {
        let _: JsonSchema = MutualA::json_schema();
    });
}

#[test]
fn issue_4_derive_terminates_on_three_type_cycle() {
    known_failure_in_child_process(4, "issue_4_derive_terminates_on_three_type_cycle", || {
        let _: JsonSchema = CycleA::json_schema();
    });
}

#[test]
fn issue_4_derive_terminates_on_self_reference_with_field_override() {
    known_failure_in_child_process(
        4,
        "issue_4_derive_terminates_on_self_reference_with_field_override",
        || {
            let _: JsonSchema = OverriddenNode::json_schema();
        },
    );
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
