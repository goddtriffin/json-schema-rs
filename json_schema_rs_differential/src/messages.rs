//! In-process error-message comparison between our library and competitor Rust validators.
//!
//! Bowtie reports only validity flags, so message quality is compared by linking every tool
//! directly: our [`json_schema_rs::validate`], the `jsonschema` crate, and the `boon` crate.
//! Each tool's messages are captured verbatim (one entry per error, or per line of boon's
//! error tree) so they can be read side by side.

use crate::suite::{SuiteFile, SuiteFileCase, SuiteFileTest, is_optional_path};
use json_schema_rs::{JsonSchema, JsonSchemaSettings, ValidationError, validate};
use serde::Serialize;
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Tool name used for our library.
pub const OURS_TOOL: &str = "json-schema-rs";
/// Tool name used for the `jsonschema` crate.
pub const JSONSCHEMA_TOOL: &str = "jsonschema";
/// Tool name used for the `boon` crate.
pub const BOON_TOOL: &str = "boon";

/// Resource location under which schemas are registered with boon, so it never touches the
/// filesystem or network for the root document.
const BOON_SCHEMA_LOC: &str = "mem://differential/schema.json";

/// What a tool concluded about an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The instance validated.
    Valid,
    /// The instance failed validation.
    Invalid,
    /// The tool could not validate (schema rejected, unsupported feature, or panic).
    Error,
}

/// One tool's verdict and messages on one instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolOutput {
    /// The tool name.
    pub tool: String,
    /// The tool's verdict.
    pub outcome: ToolOutcome,
    /// Every error message, one per entry (empty when valid).
    pub messages: Vec<String>,
}

/// One schema/instance pair with every tool's messages.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MessageCase {
    /// Where the case comes from, e.g. `suite draft2020-12/minimum.json` or `fixture small/config`.
    pub source: String,
    /// A human-readable title (case and test description, or instance path).
    pub title: String,
    /// Schema path relative to the benchmark fixtures directory (fixture cases only).
    pub schema_path: Option<String>,
    /// The schema, embedded for suite cases only (fixture schemas are referenced by path).
    pub schema: Option<Value>,
    /// Instance path relative to the benchmark fixtures directory (fixture cases only).
    pub instance_path: Option<String>,
    /// The instance, embedded for suite cases and for fixture instances of at most
    /// [`MAX_EMBEDDED_INSTANCE_BYTES`] bytes of compact JSON.
    pub instance: Option<Value>,
    /// Every tool's output, ours first.
    pub outputs: Vec<ToolOutput>,
}

/// Largest fixture instance (compact JSON bytes) embedded in a message case.
pub const MAX_EMBEDDED_INSTANCE_BYTES: usize = 2048;

/// The strict parse settings used for our library (unknown keywords are parse errors), the
/// same settings the suite integration test uses.
#[must_use]
pub fn strict_settings() -> JsonSchemaSettings {
    JsonSchemaSettings::builder()
        .disallow_unknown_fields(true)
        .build()
}

fn guarded<F>(tool: &str, run: F) -> ToolOutput
where
    F: FnOnce() -> ToolOutput,
{
    catch_unwind(AssertUnwindSafe(run)).unwrap_or_else(|_| ToolOutput {
        tool: tool.to_string(),
        outcome: ToolOutcome::Error,
        messages: vec!["panicked".to_string()],
    })
}

fn output(tool: &str, outcome: ToolOutcome, messages: Vec<String>) -> ToolOutput {
    ToolOutput {
        tool: tool.to_string(),
        outcome,
        messages,
    }
}

/// Validate with our library (strict parse), capturing the `Display` of every error.
#[must_use]
pub fn run_ours(schema: &Value, instance: &Value) -> ToolOutput {
    guarded(OURS_TOOL, || {
        let parsed: JsonSchema = match JsonSchema::new_from_serde_value(schema, &strict_settings())
        {
            Ok(parsed) => parsed,
            Err(e) => {
                return output(
                    OURS_TOOL,
                    ToolOutcome::Error,
                    vec![format!("schema rejected: {e}")],
                );
            }
        };
        match validate(&parsed, instance) {
            Ok(()) => output(OURS_TOOL, ToolOutcome::Valid, Vec::new()),
            Err(errors) => output(
                OURS_TOOL,
                ToolOutcome::Invalid,
                errors.iter().map(ValidationError::to_string).collect(),
            ),
        }
    })
}

/// Validate with the `jsonschema` crate (draft auto-detected from `$schema`, 2020-12 by
/// default), capturing every error as `<instance path>: <message>` (`/` for the root).
#[must_use]
pub fn run_jsonschema(schema: &Value, instance: &Value) -> ToolOutput {
    guarded(JSONSCHEMA_TOOL, || {
        let validator: jsonschema::Validator = match jsonschema::validator_for(schema) {
            Ok(validator) => validator,
            Err(e) => {
                return output(
                    JSONSCHEMA_TOOL,
                    ToolOutcome::Error,
                    vec![jsonschema_build_error(&e.to_string())],
                );
            }
        };
        let messages: Vec<String> = validator
            .iter_errors(instance)
            .map(|e: jsonschema::ValidationError<'_>| {
                let path: String = e.instance_path().to_string();
                let path: &str = if path.is_empty() { "/" } else { path.as_str() };
                format!("{path}: {e}")
            })
            .collect();
        if messages.is_empty() {
            output(JSONSCHEMA_TOOL, ToolOutcome::Valid, messages)
        } else {
            output(JSONSCHEMA_TOOL, ToolOutcome::Invalid, messages)
        }
    })
}

/// Marker of the `jsonschema` error for an external resource that cannot be retrieved.
const UNRETRIEVABLE_MARKER: &str = "is not present in a registry and retrieving it failed";

/// Render a `jsonschema` schema-build error. Remote retrieval is disabled, and which of several
/// unretrievable external resources the crate reports first varies between runs, so that error
/// is replaced by a fixed message to keep the output byte-stable.
fn jsonschema_build_error(message: &str) -> String {
    if message.contains(UNRETRIEVABLE_MARKER) {
        "schema rejected: references an external resource that is not available offline (remote retrieval disabled)".to_string()
    } else {
        format!("schema rejected: {message}")
    }
}

/// Validate with the `boon` crate (draft auto-detected from `$schema`, 2020-12 by default),
/// capturing its error tree (`Display`) one line per entry.
#[must_use]
pub fn run_boon(schema: &Value, instance: &Value) -> ToolOutput {
    guarded(BOON_TOOL, || {
        let mut schemas: boon::Schemas = boon::Schemas::new();
        let mut compiler: boon::Compiler = boon::Compiler::new();
        let compile_result: Result<boon::SchemaIndex, String> = compiler
            .add_resource(BOON_SCHEMA_LOC, schema.clone())
            .map_err(|e: boon::CompileError| e.to_string())
            .and_then(|()| {
                compiler
                    .compile(BOON_SCHEMA_LOC, &mut schemas)
                    .map_err(|e: boon::CompileError| e.to_string())
            });
        let index: boon::SchemaIndex = match compile_result {
            Ok(index) => index,
            Err(e) => {
                return output(
                    BOON_TOOL,
                    ToolOutcome::Error,
                    vec![format!("schema rejected: {e}")],
                );
            }
        };
        match schemas.validate(instance, index) {
            Ok(()) => output(BOON_TOOL, ToolOutcome::Valid, Vec::new()),
            Err(e) => output(
                BOON_TOOL,
                ToolOutcome::Invalid,
                e.to_string().lines().map(str::to_string).collect(),
            ),
        }
    })
}

/// Run every tool, ours first.
#[must_use]
pub fn run_all(schema: &Value, instance: &Value) -> Vec<ToolOutput> {
    vec![
        run_ours(schema, instance),
        run_jsonschema(schema, instance),
        run_boon(schema, instance),
    ]
}

/// The curated suite case for one suite file: the first `valid: false` test whose schema our
/// library strict-parses and whose instance we correctly reject. Optional-suite files and
/// files with no such test yield `None`.
#[must_use]
pub fn select_suite_case(draft: &str, file: &SuiteFile) -> Option<MessageCase> {
    if is_optional_path(&file.path) {
        return None;
    }
    let settings: JsonSchemaSettings = strict_settings();
    for case in &file.cases {
        let Ok(parsed) = JsonSchema::new_from_serde_value(&case.schema, &settings) else {
            continue;
        };
        let selected: Option<&SuiteFileTest> = case.tests.iter().find(|t: &&SuiteFileTest| {
            !t.valid
                && catch_unwind(AssertUnwindSafe(|| validate(&parsed, &t.data).is_err()))
                    .unwrap_or(false)
        });
        if let Some(test) = selected {
            return Some(suite_message_case(draft, file, case, test));
        }
    }
    None
}

fn suite_message_case(
    draft: &str,
    file: &SuiteFile,
    case: &SuiteFileCase,
    test: &SuiteFileTest,
) -> MessageCase {
    MessageCase {
        source: format!("suite {draft}/{}", file.path),
        title: format!("{} / {}", case.description, test.description),
        schema_path: None,
        schema: Some(case.schema.clone()),
        instance_path: None,
        instance: Some(test.data.clone()),
        outputs: run_all(&case.schema, &test.data),
    }
}

/// One message case per invalid instance of a benchmark fixture. The schema is referenced by
/// `schema_path` only; the instance is embedded when it is small.
#[must_use]
pub fn fixture_cases(
    fixture_id: &str,
    schema_path: &str,
    schema: &Value,
    invalid_instances: &[(String, Value)],
) -> Vec<MessageCase> {
    invalid_instances
        .iter()
        .map(|(path, instance): &(String, Value)| MessageCase {
            source: format!("fixture {fixture_id}"),
            title: path.clone(),
            schema_path: Some(schema_path.to_string()),
            schema: None,
            instance_path: Some(path.clone()),
            instance: (instance.to_string().len() <= MAX_EMBEDDED_INSTANCE_BYTES)
                .then(|| instance.clone()),
            outputs: run_all(schema, instance),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        MessageCase, ToolOutcome, ToolOutput, fixture_cases, run_boon, run_jsonschema, run_ours,
        select_suite_case,
    };
    use crate::suite::{SuiteFile, SuiteFileCase, SuiteFileTest};
    use serde_json::{Value, json};

    fn out(tool: &str, outcome: ToolOutcome, messages: &[&str]) -> ToolOutput {
        ToolOutput {
            tool: tool.to_string(),
            outcome,
            messages: messages.iter().map(|m: &&str| (*m).to_string()).collect(),
        }
    }

    #[test]
    fn ours_valid() {
        let expected: ToolOutput = out("json-schema-rs", ToolOutcome::Valid, &[]);
        let actual: ToolOutput = run_ours(&json!({"type": "string"}), &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn ours_invalid_captures_display() {
        let instance: Value = json!(1);
        let expected: ToolOutput = ToolOutput {
            tool: "json-schema-rs".to_string(),
            outcome: ToolOutcome::Invalid,
            messages: json_schema_rs::validate(
                &json_schema_rs::JsonSchema::new_from_serde_value(
                    &json!({"type": "string"}),
                    &super::strict_settings(),
                )
                .expect("parses"),
                &instance,
            )
            .expect_err("invalid")
            .iter()
            .map(ToString::to_string)
            .collect(),
        };
        let actual: ToolOutput = run_ours(&json!({"type": "string"}), &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn ours_rejects_unknown_keyword() {
        let expected: ToolOutcome = ToolOutcome::Error;
        let actual: ToolOutcome = run_ours(&json!({"notAKeyword": 1}), &json!(1)).outcome;
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_valid() {
        let expected: ToolOutput = out("jsonschema", ToolOutcome::Valid, &[]);
        let actual: ToolOutput = run_jsonschema(&json!({"type": "string"}), &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_invalid_prefixes_instance_path() {
        let expected: ToolOutput = out(
            "jsonschema",
            ToolOutcome::Invalid,
            &["/a: 1 is not of type \"string\""],
        );
        let actual: ToolOutput = run_jsonschema(
            &json!({"properties": {"a": {"type": "string"}}}),
            &json!({"a": 1}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_root_path_is_slash() {
        let expected: ToolOutput = out(
            "jsonschema",
            ToolOutcome::Invalid,
            &["/: 1 is not of type \"string\""],
        );
        let actual: ToolOutput = run_jsonschema(&json!({"type": "string"}), &json!(1));
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_schema_error() {
        let actual: ToolOutcome = run_jsonschema(&json!({"type": 12}), &json!(1)).outcome;
        let expected: ToolOutcome = ToolOutcome::Error;
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_unretrievable_resource_is_normalized() {
        let expected: ToolOutput = out(
            "jsonschema",
            ToolOutcome::Error,
            &[
                "schema rejected: references an external resource that is not available offline (remote retrieval disabled)",
            ],
        );
        let actual: ToolOutput = run_jsonschema(
            &json!({"$ref": "https://example.com/remote.json"}),
            &json!(1),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_other_build_errors_are_kept() {
        let expected: String = "schema rejected: boom".to_string();
        let actual: String = super::jsonschema_build_error("boom");
        assert_eq!(expected, actual);
    }

    #[test]
    fn boon_valid() {
        let expected: ToolOutput = out("boon", ToolOutcome::Valid, &[]);
        let actual: ToolOutput = run_boon(&json!({"type": "string"}), &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn boon_invalid_splits_tree_lines() {
        let expected: ToolOutput = out(
            "boon",
            ToolOutcome::Invalid,
            &[
                "jsonschema validation failed with mem://differential/schema.json#",
                "- at '': want string, but got number",
            ],
        );
        let actual: ToolOutput = run_boon(&json!({"type": "string"}), &json!(1));
        assert_eq!(expected, actual);
    }

    #[test]
    fn boon_schema_error() {
        let actual: ToolOutcome = run_boon(&json!({"type": 12}), &json!(1)).outcome;
        let expected: ToolOutcome = ToolOutcome::Error;
        assert_eq!(expected, actual);
    }

    fn suite_file(path: &str) -> SuiteFile {
        SuiteFile {
            path: path.to_string(),
            cases: vec![
                SuiteFileCase {
                    description: "unsupported".to_string(),
                    schema: json!({"notAKeyword": true}),
                    tests: vec![SuiteFileTest {
                        description: "never chosen".to_string(),
                        data: json!(1),
                        valid: false,
                    }],
                },
                SuiteFileCase {
                    description: "string".to_string(),
                    schema: json!({"type": "string"}),
                    tests: vec![
                        SuiteFileTest {
                            description: "valid one".to_string(),
                            data: json!("x"),
                            valid: true,
                        },
                        SuiteFileTest {
                            description: "number".to_string(),
                            data: json!(1),
                            valid: false,
                        },
                    ],
                },
            ],
        }
    }

    #[test]
    fn selects_first_rejected_invalid_test() {
        let expected: Option<(String, String)> = Some((
            "suite draft2020-12/type.json".to_string(),
            "string / number".to_string(),
        ));
        let actual: Option<(String, String)> =
            select_suite_case("draft2020-12", &suite_file("type.json"))
                .map(|c: MessageCase| (c.source, c.title));
        assert_eq!(expected, actual);
    }

    #[test]
    fn skips_optional_files() {
        let expected: Option<MessageCase> = None;
        let actual: Option<MessageCase> =
            select_suite_case("draft2020-12", &suite_file("optional/type.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_selectable_test_is_none() {
        let mut file: SuiteFile = suite_file("type.json");
        file.cases.truncate(1);
        let expected: Option<MessageCase> = None;
        let actual: Option<MessageCase> = select_suite_case("draft2020-12", &file);
        assert_eq!(expected, actual);
    }

    #[test]
    fn fixture_cases_reference_schema_and_embed_small_instances() {
        let schema: Value = json!({"type": "string"});
        let big: Value = json!("x".repeat(3000));
        let expected: Vec<MessageCase> = vec![
            MessageCase {
                source: "fixture small/x".to_string(),
                title: "small/x/instances/invalid/a.json".to_string(),
                schema_path: Some("small/x/schema.json".to_string()),
                schema: None,
                instance_path: Some("small/x/instances/invalid/a.json".to_string()),
                instance: Some(json!(1)),
                outputs: super::run_all(&schema, &json!(1)),
            },
            MessageCase {
                source: "fixture small/x".to_string(),
                title: "small/x/instances/invalid/b.json".to_string(),
                schema_path: Some("small/x/schema.json".to_string()),
                schema: None,
                instance_path: Some("small/x/instances/invalid/b.json".to_string()),
                instance: None,
                outputs: super::run_all(&schema, &big),
            },
        ];
        let actual: Vec<MessageCase> = fixture_cases(
            "small/x",
            "small/x/schema.json",
            &schema,
            &[
                ("small/x/instances/invalid/a.json".to_string(), json!(1)),
                ("small/x/instances/invalid/b.json".to_string(), big.clone()),
            ],
        );
        assert_eq!(expected, actual);
    }
}
