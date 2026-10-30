//! Known-failing regression tests: one or more tests per open bug issue, grouped under a
//! `// --- #N: ... ---` comment. Test names describe the behaviour checked, never the issue
//! number; the issue is the wrapper's first argument.
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
    CodeGenSettings, GenerateRustOutput, JsonPointer, JsonSchema, JsonSchemaSettings,
    ValidationError, ValidationResult, generate_rust, validate,
};
use serde_json::json;
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
#[expect(
    dead_code,
    reason = "no open validator, parser or codegen bug has a known-failing test; remove this when adding one"
)]
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
#[expect(
    dead_code,
    reason = "no open bug aborts or hangs the process; remove this when adding one"
)]
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

/// The generated Rust for `schema_json` alone (default settings).
fn generated(schema_json: &str) -> String {
    let output: GenerateRustOutput =
        generate_rust(&[parse(schema_json)], &CodeGenSettings::builder().build())
            .expect("codegen succeeds");
    String::from_utf8(output.per_schema[0].clone()).expect("generated Rust is UTF-8")
}

#[test]
fn type_array_accepts_second_entry_string() {
    let expected: bool = true;
    let actual: bool = is_valid(r#"{"type":["integer","string"]}"#, r#""x""#);
    assert_eq!(expected, actual);
}

#[test]
fn type_array_accepts_second_entry_object() {
    let expected: bool = true;
    let actual: bool = is_valid(r#"{"type":["string","object"]}"#, "{}");
    assert_eq!(expected, actual);
}

#[test]
fn type_null_rejects_string() {
    let expected: bool = false;
    let actual: bool = is_valid(r#"{"type":"null"}"#, r#""x""#);
    assert_eq!(expected, actual);
}

#[test]
fn type_null_rejects_integer() {
    let expected: bool = false;
    let actual: bool = is_valid(r#"{"type":"null"}"#, "1");
    assert_eq!(expected, actual);
}

#[test]
fn unknown_type_name_fails_to_parse() {
    let expected: String = "invalid JSON Schema: unknown type \"strng\", expected one of \
                            \"array\", \"boolean\", \"integer\", \"null\", \"number\", \
                            \"object\", \"string\" at line 1 column 15"
        .to_string();
    let actual: String =
        JsonSchema::new_from_str(r#"{"type":"strng"}"#, &JsonSchemaSettings::default())
            .expect_err("unknown type name is a parse error")
            .to_string();
    assert_eq!(expected, actual);
}

/// `allOf` of a `$ref` merges the referenced object; the root is that object, so it takes the
/// definition's name.
#[test]
fn codegen_accepts_all_of_ref() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Base {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r##"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "allOf": [{"$ref": "#/definitions/base"}],
            "definitions": {"base": {"type": "object", "properties": {"a": {"type": "string"}}}}
        }"##,
    );
    assert_eq!(expected, actual);
}

/// Two different types that want the same name both get a numeric suffix.
#[test]
fn codegen_struct_name_collision_keeps_both_types() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Item0>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Item1>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r#"{
            "type": "object",
            "properties": {
                "a": {"title": "Item", "type": "object", "properties": {"x": {"type": "string"}}},
                "b": {"title": "Item", "type": "object", "properties": {"y": {"type": "integer"}}}
            }
        }"#,
    );
    assert_eq!(expected, actual);
}

#[test]
fn ref_siblings_apply_in_2020_12() {
    let expected: bool = false;
    let actual: bool = is_valid(
        r##"{"type":"array","$defs":{"a":{"type":"array"}},"$ref":"#/$defs/a","maxItems":1}"##,
        "[1, 2]",
    );
    assert_eq!(expected, actual);
}

/// A struct that refers to itself through the root boxes the by-value back-edge.
#[test]
fn codegen_terminates_on_self_reference_via_root() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"}},"required":["value"]}"##,
    );
    assert_eq!(expected, actual);
}

/// The issue's first repro: two back-edges to the root; the one inside `Vec` needs no `Box`.
#[test]
fn codegen_terminates_on_self_reference_via_root_with_children_array() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["value"]}"##,
    );
    assert_eq!(expected, actual);
}

#[test]
fn codegen_terminates_on_self_reference_via_defs() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Node>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r##"{"type":"object","properties":{"head":{"$ref":"#/$defs/Node"}},"$defs":{"Node":{"type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#/$defs/Node"}}}}}"##,
    );
    assert_eq!(expected, actual);
}

/// Mutual recursion boxes one edge of the cycle (the walk reaches `A` first). The root has the
/// same shape as `B`, so the two are one type.
#[test]
fn codegen_terminates_on_mutual_recursion_via_defs() {
    let expected: String = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct A {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Box<B>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct B {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<A>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
    .to_string();
    let actual: String = generated(
        r##"{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}},"$defs":{"A":{"type":"object","properties":{"b":{"$ref":"#/$defs/B"}}},"B":{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}}}}}"##,
    );
    assert_eq!(expected, actual);
}

const IN_PLACE_LOOP_SCHEMA: &str = r##"{"anyOf":[{"type":"string"},{"$ref":"#"}]}"##;

/// The loop fails its branch with a `RefCycle` cause, so `anyOf` fails as a whole.
#[test]
fn validator_terminates_on_in_place_ref_loop() {
    let expected: ValidationResult = Err(vec![ValidationError::NoSubschemaMatched {
        instance_path: JsonPointer::root(),
        subschema_count: 2,
    }]);
    let actual: ValidationResult = validate(&parse(IN_PLACE_LOOP_SCHEMA), &json!({"x": 1}));
    assert_eq!(expected, actual);
}

/// The first `anyOf` branch matches before the loop is reached.
#[test]
fn in_place_ref_loop_schema_accepts_matching_branch() {
    let expected: bool = true;
    let actual: bool = is_valid(IN_PLACE_LOOP_SCHEMA, r#""hi""#);
    assert_eq!(expected, actual);
}
