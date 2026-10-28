//! Known-failing regression tests for `#[derive(ToJsonSchema)]`: one or more tests per open
//! bug issue, grouped under a `// --- #N: ... ---` comment. Test names describe the behaviour
//! checked, never the issue number; the issue is the wrapper's first argument.
//!
//! Each test body asserts the **correct** behaviour. The wrapper inverts it: the test passes
//! while the body still fails, and fails as soon as the body passes ("unexpected pass"). When
//! an issue is fixed, unwrap its tests into plain `#[test]`s. [`known_failure`] mirrors the
//! helper in `json_schema_rs/tests/known_failures.rs` (a separate crate, so it cannot be
//! shared); a bug that aborts the process needs that file's `known_failure_in_child_process`.

use json_schema_rs::{JsonSchema, ToJsonSchema};
use json_schema_rs_macro::ToJsonSchema;
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

fn schema_from_json(value: serde_json::Value) -> JsonSchema {
    serde_json::from_value::<JsonSchema>(value).expect("valid schema")
}

// --- #46: nested $id resources must carry their own $defs ---
//
// A type with `id` is its own schema resource. Nested in root `$defs`, its `#/$defs/...`
// refs resolve against its `$id` (2020-12 core §9.2.1), so it must carry the `$defs` they name
// and refer to itself as `#`. Each expected schema was checked with jsonschema-cli 0.60.2 (it
// accepts a valid instance and rejects an invalid one). The cases that already work are plain
// tests in `derive_to_json_schema.rs` (`derive_id_resource_*`, `derive_nested_id_resource_*`).

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct ResourceLeaf {
    x: String,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/with-dep")]
#[expect(dead_code)]
struct ResourceWithDep {
    leaf: ResourceLeaf,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceWithDep {
    inner: ResourceWithDep,
}

#[derive(ToJsonSchema)]
#[json_schema(id = "https://example.com/node")]
#[expect(dead_code)]
struct ResourceNode {
    next: Option<Box<ResourceNode>>,
    x: String,
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsResourceNode {
    node: ResourceNode,
}

// A hand-written resource with its own `$defs`, valid on its own.
struct HandWrittenResource;

impl ToJsonSchema for HandWrittenResource {
    fn json_schema() -> JsonSchema {
        schema_from_json(serde_json::json!({
            "$id": "https://example.com/hand",
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/Str" } },
            "required": ["x"],
            "$defs": { "Str": { "type": "string" } }
        }))
    }
}

#[derive(ToJsonSchema)]
#[expect(dead_code)]
struct HoldsHandWrittenResource {
    hand: HandWrittenResource,
}

#[test]
fn nested_id_resource_carries_the_defs_its_refs_need() {
    known_failure(46, || {
        let expected: JsonSchema = schema_from_json(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "inner": { "$ref": "#/$defs/ResourceWithDep" } },
            "additionalProperties": false,
            "required": ["inner"],
            "$defs": {
                "ResourceWithDep": {
                    "$id": "https://example.com/with-dep",
                    "type": "object",
                    "properties": { "leaf": { "$ref": "#/$defs/ResourceLeaf" } },
                    "additionalProperties": false,
                    "required": ["leaf"],
                    "$defs": {
                        "ResourceLeaf": {
                            "type": "object",
                            "properties": { "x": { "type": "string" } },
                            "additionalProperties": false,
                            "required": ["x"]
                        }
                    }
                }
            }
        }));
        let actual: JsonSchema = HoldsResourceWithDep::json_schema();
        assert_eq!(expected, actual);
    });
}

#[test]
fn nested_recursive_id_resource_refers_to_its_own_root() {
    known_failure(46, || {
        let expected: JsonSchema = schema_from_json(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "node": { "$ref": "#/$defs/ResourceNode" } },
            "additionalProperties": false,
            "required": ["node"],
            "$defs": {
                "ResourceNode": {
                    "$id": "https://example.com/node",
                    "type": "object",
                    "properties": {
                        "next": { "$ref": "#" },
                        "x": { "type": "string" }
                    },
                    "additionalProperties": false,
                    "required": ["x"]
                }
            }
        }));
        let actual: JsonSchema = HoldsResourceNode::json_schema();
        assert_eq!(expected, actual);
    });
}

#[test]
fn hand_written_id_resource_keeps_its_own_defs() {
    known_failure(46, || {
        let expected: JsonSchema = schema_from_json(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": { "hand": HandWrittenResource::json_schema() },
            "additionalProperties": false,
            "required": ["hand"]
        }));
        let actual: JsonSchema = HoldsHandWrittenResource::json_schema();
        assert_eq!(expected, actual);
    });
}
