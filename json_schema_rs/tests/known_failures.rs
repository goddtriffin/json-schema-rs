//! Known-failing regression tests: one or more tests per open bug issue, named `issue_<N>_...`.
//!
//! Each test body asserts the **correct** behaviour. The wrapper inverts it: the test passes
//! while the body still fails, and fails as soon as the body passes ("unexpected pass"), so a
//! fix cannot land without the maintainer noticing. When an issue is fixed, unwrap its tests
//! into plain `#[test]`s (and tighten them to full golden values where the issue asks for it).
//!
//! - [`known_failure`] runs the body in-process and treats a panic as "still failing".
//! - [`known_failure_in_child_process`] is for bugs that abort the process (stack overflow) or
//!   never terminate (hang): it re-runs this one test in a child process with a timeout.
//!
//! Official-suite failures are tracked separately in `suite_xfail.json`.

use json_schema_rs::{
    CodeGenSettings, GenerateRustOutput, JsonSchema, JsonSchemaParseError, JsonSchemaSettings,
    generate_rust, validate,
};
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

// --- The harness itself ---

/// A child whose body completes is reported as a pass, so a fixed bug is noticed.
#[test]
fn known_failure_harness_reports_child_pass() {
    const NAME: &str = "known_failure_harness_reports_child_pass";
    if std::env::var(CHILD_ENV).is_ok_and(|name| name == NAME) {
        println!("{CHILD_PASSED_MARKER}");
        return;
    }
    let expected: Outcome = Outcome::Passed;
    let actual: Outcome = run_child(NAME);
    assert_eq!(expected, actual);
}

/// A child that matches no test is reported as not run, never as "still failing".
#[test]
fn known_failure_harness_reports_child_not_run() {
    let expected: Outcome = Outcome::NotRun;
    let actual: Outcome = run_child("known_failure_harness_no_such_test");
    assert_eq!(expected, actual);
}

fn parse(schema_json: &str) -> JsonSchema {
    JsonSchema::new_from_str(schema_json, &JsonSchemaSettings::default()).expect("schema parses")
}

/// Validator verdict for `instance_json` against `schema_json` (default settings, 2020-12).
fn is_valid(schema_json: &str, instance_json: &str) -> bool {
    let instance: serde_json::Value = serde_json::from_str(instance_json).expect("instance parses");
    validate(&parse(schema_json), &instance).is_ok()
}

/// Codegen outcome for `schema_json`: `Ok(())` when Rust is generated.
fn generates(schema_json: &str) -> Result<(), String> {
    generate_rust(&[parse(schema_json)], &CodeGenSettings::builder().build())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Sorted names of every `pub struct` in the generated output.
fn generated_struct_names(output: &GenerateRustOutput) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for buffer in output.shared.iter().chain(output.per_schema.iter()) {
        let source: &str = std::str::from_utf8(buffer).expect("generated Rust is UTF-8");
        for line in source.lines() {
            if let Some(rest) = line.strip_prefix("pub struct ") {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                names.push(name);
            }
        }
    }
    names.sort();
    names
}

// --- #18: codegen rejects allOf containing a $ref (fixed in #9) ---

#[test]
fn issue_18_codegen_accepts_all_of_ref() {
    known_failure(18, || {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = generates(
            r##"{
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "allOf": [{"$ref": "#/definitions/base"}],
                "definitions": {"base": {"type": "object", "properties": {"a": {"type": "string"}}}}
            }"##,
        );
        assert_eq!(expected, actual);
    });
}

// --- #9: codegen struct-name collision drops the second type (fixed in #9) ---

#[test]
fn issue_9_codegen_struct_name_collision_keeps_both_types() {
    known_failure(9, || {
        let output: GenerateRustOutput = generate_rust(
            &[parse(
                r#"{
                    "type": "object",
                    "properties": {
                        "a": {"title": "Item", "type": "object", "properties": {"x": {"type": "string"}}},
                        "b": {"title": "Item", "type": "object", "properties": {"y": {"type": "integer"}}}
                    }
                }"#,
            )],
            &CodeGenSettings::builder().build(),
        )
        .expect("codegen succeeds");
        let expected: Vec<String> = vec!["Item0".into(), "Item1".into(), "Root".into()];
        let actual: Vec<String> = generated_struct_names(&output);
        assert_eq!(expected, actual);
    });
}

// --- #28: $ref siblings ignored in 2019-09 / 2020-12 (fixed in #8) ---

#[test]
fn issue_28_ref_siblings_apply_in_2020_12() {
    known_failure(28, || {
        let expected: bool = false;
        let actual: bool = is_valid(
            r##"{"type":"array","$defs":{"a":{"type":"array"}},"$ref":"#/$defs/a","maxItems":1}"##,
            "[1, 2]",
        );
        assert_eq!(expected, actual);
    });
}

// --- #29: type "integer" rejects 1.0 and values above i64::MAX (fixed in #5) ---

#[test]
fn issue_29_integer_accepts_zero_fraction_float() {
    known_failure(29, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"integer"}"#, "1.0");
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_29_integer_accepts_u64_above_i64_max() {
    known_failure(29, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"integer"}"#, "18446744073709551615");
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_29_integer_accepts_bignum() {
    known_failure(29, || {
        let expected: bool = true;
        let actual: bool = is_valid(
            r#"{"type":"integer"}"#,
            "12345678910111213141516171819202122232425",
        );
        assert_eq!(expected, actual);
    });
}

// --- #30: strict mode never reports UnknownField (fixed in #7) ---

#[test]
fn issue_30_strict_mode_reports_unknown_field_with_path() {
    known_failure(30, || {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::new_from_str(
            r#"{"type":"object","properties":{"a":{"type":"string","bogus":1}}}"#,
            &settings,
        );
        // Expected: Err(UnknownField { key: "bogus", path: "/properties/a" }).
        assert!(
            matches!(
                &actual,
                Err(JsonSchemaParseError::UnknownField { key, path })
                    if key == "bogus" && path == "/properties/a"
            ),
            "{actual:?}"
        );
    });
}

// --- #31: "type" arrays truncated to their first entry (fixed in #6) ---

#[test]
fn issue_31_type_array_accepts_second_entry_string() {
    known_failure(31, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":["integer","string"]}"#, r#""x""#);
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_31_type_array_accepts_second_entry_object() {
    known_failure(31, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":["string","object"]}"#, "{}");
        assert_eq!(expected, actual);
    });
}

// --- #32: type "null" accepts every instance (fixed in #6) ---

#[test]
fn issue_32_type_null_rejects_string() {
    known_failure(32, || {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"null"}"#, r#""x""#);
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_32_type_null_rejects_integer() {
    known_failure(32, || {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"null"}"#, "1");
        assert_eq!(expected, actual);
    });
}

// --- #33: enum / const / uniqueItems treat 1 and 1.0 as different (fixed in #5) ---

#[test]
fn issue_33_enum_matches_mathematically_equal_number() {
    known_failure(33, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"enum":[0]}"#, "0.0");
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_33_const_matches_mathematically_equal_number() {
    known_failure(33, || {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"const":-2.0}"#, "-2");
        assert_eq!(expected, actual);
    });
}

#[test]
fn issue_33_unique_items_rejects_mathematically_equal_numbers() {
    known_failure(33, || {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"array","uniqueItems":true}"#, "[1, 1.0]");
        assert_eq!(expected, actual);
    });
}

// --- #34: integer-valued decimals ("minLength": 2.0) fail to parse (fixed in #7) ---

#[test]
fn issue_34_count_keyword_accepts_integer_valued_decimal() {
    known_failure(34, || {
        let instance: serde_json::Value = serde_json::json!("a");
        let expected: Result<bool, String> = Ok(false);
        let actual: Result<bool, String> = JsonSchema::new_from_str(
            r#"{"type":"string","minLength":2.0}"#,
            &JsonSchemaSettings::default(),
        )
        .map(|schema| validate(&schema, &instance).is_ok())
        .map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    });
}

// --- #35: codegen hangs on self-referential schemas (fixed in #9) ---
//
// The issue's first repro also has `"children": {"type": "array", "items": {"$ref": "#"}}`;
// that variant allocates several GB per second while it hangs, so these tests use shapes whose
// hang holds memory flat. #9 adds golden tests for the full repros.

#[test]
fn issue_35_codegen_terminates_on_self_reference_via_root() {
    known_failure_in_child_process(
        35,
        "issue_35_codegen_terminates_on_self_reference_via_root",
        || {
            let expected: Result<(), String> = Ok(());
            let actual: Result<(), String> = generates(
                r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"}},"required":["value"]}"##,
            );
            assert_eq!(expected, actual);
        },
    );
}

#[test]
fn issue_35_codegen_terminates_on_self_reference_via_defs() {
    known_failure_in_child_process(
        35,
        "issue_35_codegen_terminates_on_self_reference_via_defs",
        || {
            let expected: Result<(), String> = Ok(());
            let actual: Result<(), String> = generates(
                r##"{"type":"object","properties":{"head":{"$ref":"#/$defs/Node"}},"$defs":{"Node":{"type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#/$defs/Node"}}}}}"##,
            );
            assert_eq!(expected, actual);
        },
    );
}

#[test]
fn issue_35_codegen_terminates_on_mutual_recursion_via_defs() {
    known_failure_in_child_process(
        35,
        "issue_35_codegen_terminates_on_mutual_recursion_via_defs",
        || {
            let expected: Result<(), String> = Ok(());
            let actual: Result<(), String> = generates(
                r##"{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}},"$defs":{"A":{"type":"object","properties":{"b":{"$ref":"#/$defs/B"}}},"B":{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}}}}}"##,
            );
            assert_eq!(expected, actual);
        },
    );
}

// --- #36: validator stack overflow on an in-place $ref loop (fixed in #8) ---

const IN_PLACE_LOOP_SCHEMA: &str = r##"{"anyOf":[{"type":"string"},{"$ref":"#"}]}"##;

#[test]
fn issue_36_validator_terminates_on_in_place_ref_loop() {
    known_failure_in_child_process(
        36,
        "issue_36_validator_terminates_on_in_place_ref_loop",
        || {
            let expected: bool = false;
            let actual: bool = is_valid(IN_PLACE_LOOP_SCHEMA, r#"{"x":1}"#);
            assert_eq!(expected, actual);
        },
    );
}

/// Passes today and must keep passing: the first `anyOf` branch matches before the loop.
#[test]
fn issue_36_in_place_ref_loop_schema_accepts_matching_branch() {
    let expected: bool = true;
    let actual: bool = is_valid(IN_PLACE_LOOP_SCHEMA, r#""hi""#);
    assert_eq!(expected, actual);
}
