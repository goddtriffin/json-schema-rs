//! Bowtie harness for json-schema-rs.
//!
//! Speaks Bowtie's line-delimited JSON protocol (version 1) over stdin/stdout: one command per
//! input line, one response per output line. Each `run` case is parsed with **strict** schema
//! ingestion (unknown keywords are rejected), its registry is built and the schema compiled once
//! ([`json_schema_rs::CompiledSchema`]), and every test instance is validated with
//! [`json_schema_rs::CompiledSchema::validate`].
//!
//! Classification of each case and test (the differential report builder depends on these exact
//! skip messages):
//!
//! - Strict parsing rejects a key the draft does not define → whole-case skip
//!   `unsupported keyword "<key>" at <path>`.
//! - Any other parse failure, a registry error other than one `$ref`, or a compile error (e.g. a
//!   pattern our regex engine cannot compile) → whole-case skip `unsupported schema: <error>`.
//! - The registry reports an unresolvable `$ref` → whole-case skip `unsupported $ref: <detail>`.
//! - Validation reports another capability gap → test skip `unsupported: <detail>`.
//! - Anything else is a real verdict `{"valid": bool}`.
//! - A panic anywhere in a case is caught and reported as `errored`, so one bad case never takes
//!   the harness down.
//!
//! The harness never writes to stderr during normal operation; Bowtie treats stderr output as a
//! sign of trouble.

use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::panic::{self, AssertUnwindSafe, PanicHookInfo};

use json_schema_rs::{
    CompileError, CompiledSchema, JsonSchemaParseError, JsonSchemaSettings, Registry,
    RegistryError, Schema, SpecVersion, ValidationError, ValidationErrorKind, ValidationResult,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Bowtie IO protocol version this harness implements.
const PROTOCOL_VERSION: u64 = 1;

/// Dialects this harness declares to Bowtie, as Bowtie's dialect URIs, paired with our
/// [`SpecVersion`].
const DIALECTS: [(&str, SpecVersion); 5] = [
    (
        "https://json-schema.org/draft/2020-12/schema",
        SpecVersion::Draft202012,
    ),
    (
        "https://json-schema.org/draft/2019-09/schema",
        SpecVersion::Draft201909,
    ),
    (
        "http://json-schema.org/draft-07/schema#",
        SpecVersion::Draft07,
    ),
    (
        "http://json-schema.org/draft-06/schema#",
        SpecVersion::Draft06,
    ),
    (
        "http://json-schema.org/draft-04/schema#",
        SpecVersion::Draft04,
    ),
];

const REPOSITORY_URL: &str = "https://github.com/goddtriffin/json-schema-rs";
const ISSUES_URL: &str = "https://github.com/goddtriffin/json-schema-rs/issues";
const DOCUMENTATION_URL: &str = "https://docs.rs/json-schema-rs";

/// Rust toolchain version the harness was built with, injected by the container build.
const RUSTC_VERSION: Option<&str> = option_env!("BOWTIE_HARNESS_RUSTC_VERSION");

const ANNOTATIONS_UNSUPPORTED: &str = "annotations are not supported";

thread_local! {
    /// Details of the most recent panic on this thread, captured by the panic hook so a caught
    /// panic can be reported to Bowtie instead of being printed to stderr.
    static LAST_PANIC: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// One command sent by Bowtie.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
enum Command {
    /// Bowtie also sends the protocol version it speaks; only version 1 exists, so it is not read.
    Start,
    Dialect {
        dialect: String,
    },
    Run {
        seq: Value,
        case: Case,
        #[serde(default)]
        output: OutputFormat,
    },
    Stop,
}

/// The output format Bowtie requests for a `run` command.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OutputFormat {
    #[default]
    Flag,
    Annotations,
}

/// A test case: one schema, the remote documents it may reference (by URI), and the instances
/// to validate against it.
#[derive(Debug, Deserialize)]
struct Case {
    schema: Value,
    #[serde(default)]
    registry: Map<String, Value>,
    tests: Vec<Test>,
}

/// One instance to validate.
#[derive(Debug, Deserialize)]
struct Test {
    instance: Value,
}

/// How the harness handled one test instance.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestOutcome {
    /// A real validation verdict.
    Verdict(bool),
    /// The test needs something the library does not support.
    Skipped(String),
    /// The library panicked while validating the instance.
    Errored(String),
}

/// How the harness handled one whole case.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CaseOutcome {
    /// The schema was accepted; one outcome per test, in order.
    Results(Vec<TestOutcome>),
    /// The schema uses something the library does not support.
    Skipped(String),
    /// The library panicked while handling the case.
    Errored(String),
}

/// Protocol state carried between commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Harness {
    /// Spec version for schemas without a recognized `$schema` of their own.
    dialect: SpecVersion,
}

impl Default for Harness {
    fn default() -> Self {
        Self {
            dialect: SpecVersion::Draft202012,
        }
    }
}

impl Harness {
    /// Handles one command. Returns the response to write, or `None` when the harness should exit.
    fn handle(&mut self, command: Command) -> Option<Value> {
        match command {
            Command::Start => Some(start_response()),
            Command::Dialect { dialect } => {
                let spec_version: Option<SpecVersion> = dialect_spec_version(&dialect);
                if let Some(v) = spec_version {
                    self.dialect = v;
                }
                Some(json!({ "ok": spec_version.is_some() }))
            }
            Command::Run { seq, case, output } => {
                let outcome: CaseOutcome = run_case(&case, self.dialect, output);
                Some(run_response(&seq, outcome))
            }
            Command::Stop => None,
        }
    }
}

/// Maps a Bowtie dialect URI to our [`SpecVersion`], for the dialects this harness declares.
fn dialect_spec_version(uri: &str) -> Option<SpecVersion> {
    DIALECTS
        .iter()
        .find(|(dialect_uri, _)| *dialect_uri == uri)
        .map(|(_, spec_version)| *spec_version)
}

/// Strict ingestion settings for a case: Bowtie's current dialect is the default draft (a schema
/// with its own `$schema` follows that, a custom metaschema through the registry), and keys the
/// draft does not define are rejected, so they surface as skips rather than ignored keys.
fn strict_settings(dialect: SpecVersion) -> JsonSchemaSettings {
    JsonSchemaSettings::builder()
        .default_spec_version(dialect)
        .disallow_unknown_fields(true)
        .build()
}

/// Skip message for a schema that strict parsing rejected.
fn parse_skip_message(error: &JsonSchemaParseError) -> String {
    match error {
        JsonSchemaParseError::UnknownField { key, path } => format!(
            "unsupported keyword \"{key}\" at {}",
            path.display_root_or_path()
        ),
        JsonSchemaParseError::Serde(_)
        | JsonSchemaParseError::InvalidKeywordValue { .. }
        | JsonSchemaParseError::Io(_)
        | JsonSchemaParseError::BooleanRoot { .. }
        | JsonSchemaParseError::UnknownSchemaUri { .. } => {
            format!("unsupported schema: {error}")
        }
    }
}

/// Skip message when a validation error reflects a library capability gap rather than an
/// invalid instance.
fn capability_gap_message(error: &ValidationError) -> Option<String> {
    match &error.kind {
        ValidationErrorKind::InvalidRef { ref_str, reason } => {
            Some(format!("unsupported $ref: \"{ref_str}\": {reason}"))
        }
        ValidationErrorKind::InvalidPatternInSchema { .. }
        | ValidationErrorKind::InvalidSchema { .. } => Some(format!("unsupported: {error}")),
        _ => None,
    }
}

/// Classifies one validation result as a verdict or a capability-gap skip.
fn classify(result: &ValidationResult) -> TestOutcome {
    match result {
        Ok(()) => TestOutcome::Verdict(true),
        Err(errors) => errors
            .iter()
            .find_map(capability_gap_message)
            .map_or(TestOutcome::Verdict(false), TestOutcome::Skipped),
    }
}

/// Runs one case: parses its schema strictly and validates every test instance.
fn run_case(case: &Case, dialect: SpecVersion, output: OutputFormat) -> CaseOutcome {
    if output == OutputFormat::Annotations {
        return CaseOutcome::Results(vec![
            TestOutcome::Skipped(
                ANNOTATIONS_UNSUPPORTED.to_string()
            );
            case.tests.len()
        ]);
    }
    let settings: JsonSchemaSettings = strict_settings(dialect);
    let schema: Schema = match catch_panic(|| Schema::new_from_serde_value(&case.schema, &settings))
    {
        Ok(Ok(schema)) => schema,
        Ok(Err(errors)) => return CaseOutcome::Skipped(parse_skip_message(errors.first())),
        Err(panic_message) => return CaseOutcome::Errored(panic_message),
    };
    let registry: Registry<'_> = match catch_panic(|| {
        let mut builder = Registry::builder().settings(settings.clone());
        for (uri, document) in &case.registry {
            builder = builder.document(uri.clone(), document.clone());
        }
        builder.build(&schema)
    }) {
        Ok(Ok(registry)) => registry,
        Ok(Err(errors)) => {
            // One skip reason per case, as for parsing, so results stay comparable.
            let error: &RegistryError = errors.first();
            return CaseOutcome::Skipped(match error.ref_str() {
                Some(ref_str) => format!("unsupported $ref: \"{ref_str}\": {error}"),
                None => format!("unsupported schema: {error}"),
            });
        }
        Err(panic_message) => return CaseOutcome::Errored(panic_message),
    };
    let compiled: CompiledSchema = match catch_panic(|| CompiledSchema::new(&registry)) {
        Ok(Ok(compiled)) => compiled,
        Ok(Err(errors)) => {
            let error: &CompileError = errors.first();
            return CaseOutcome::Skipped(format!("unsupported schema: {error}"));
        }
        Err(panic_message) => return CaseOutcome::Errored(panic_message),
    };
    let outcomes: Vec<TestOutcome> = case
        .tests
        .iter()
        .map(
            |test: &Test| match catch_panic(|| compiled.validate(&test.instance)) {
                Ok(result) => classify(&result),
                Err(panic_message) => TestOutcome::Errored(panic_message),
            },
        )
        .collect();
    CaseOutcome::Results(outcomes)
}

/// Runs `f`, converting a panic into an error carrying the panic's details.
fn catch_panic<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(f)).map_err(|payload: Box<dyn std::any::Any + Send>| {
        LAST_PANIC
            .with(|last: &RefCell<Option<String>>| last.borrow_mut().take())
            .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic with a non-string payload".to_string())
    })
}

/// Panic hook that records the panic for [`catch_panic`] instead of printing to stderr.
fn record_panic(info: &PanicHookInfo<'_>) {
    let details: String = info.to_string();
    LAST_PANIC.with(|last: &RefCell<Option<String>>| *last.borrow_mut() = Some(details));
}

/// JSON for one test outcome.
fn test_outcome_json(outcome: TestOutcome) -> Value {
    match outcome {
        TestOutcome::Verdict(valid) => json!({ "valid": valid }),
        TestOutcome::Skipped(message) => json!({ "skipped": true, "message": message }),
        TestOutcome::Errored(message) => {
            json!({ "errored": true, "context": { "message": message } })
        }
    }
}

/// JSON response to a `run` command.
fn run_response(seq: &Value, outcome: CaseOutcome) -> Value {
    match outcome {
        CaseOutcome::Results(outcomes) => {
            let results: Vec<Value> = outcomes.into_iter().map(test_outcome_json).collect();
            json!({ "seq": seq, "results": results })
        }
        CaseOutcome::Skipped(message) => json!({ "seq": seq, "skipped": true, "message": message }),
        CaseOutcome::Errored(message) => {
            json!({ "seq": seq, "errored": true, "context": { "message": message } })
        }
    }
}

/// JSON response to a `start` command, describing this implementation.
fn start_response() -> Value {
    let dialects: Vec<&str> = DIALECTS.iter().map(|(uri, _)| *uri).collect();
    let mut implementation: Map<String, Value> = Map::new();
    implementation.insert("name".to_string(), json!("json-schema-rs"));
    implementation.insert("language".to_string(), json!("rust"));
    implementation.insert("version".to_string(), json!(env!("CARGO_PKG_VERSION")));
    implementation.insert("dialects".to_string(), json!(dialects));
    implementation.insert("homepage".to_string(), json!(REPOSITORY_URL));
    implementation.insert("issues".to_string(), json!(ISSUES_URL));
    implementation.insert("source".to_string(), json!(REPOSITORY_URL));
    implementation.insert("documentation".to_string(), json!(DOCUMENTATION_URL));
    if let Some(rustc_version) = RUSTC_VERSION {
        implementation.insert("language_version".to_string(), json!(rustc_version));
    }
    let os_release: String = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    if let Some(os) = os_release_field(&os_release, "ID") {
        implementation.insert("os".to_string(), json!(os));
    }
    if let Some(os_version) = os_release_field(&os_release, "VERSION_ID") {
        implementation.insert("os_version".to_string(), json!(os_version));
    }
    json!({ "version": PROTOCOL_VERSION, "implementation": implementation })
}

/// Reads one `KEY=value` field from `/etc/os-release` contents, unquoting the value.
fn os_release_field(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line: &str| {
        line.strip_prefix(key)
            .and_then(|rest: &str| rest.strip_prefix('='))
            .map(|value: &str| value.trim().trim_matches('"').to_string())
    })
}

/// Response for a line that is not a command this harness understands. A `seq` is echoed when
/// present so Bowtie can attribute the error to its case.
fn malformed_command_response(line: &Value, error: &serde_json::Error) -> Value {
    let context: Value = json!({ "message": format!("malformed command: {error}") });
    match line.get("seq") {
        Some(seq) => json!({ "seq": seq, "errored": true, "context": context }),
        None => json!({ "errored": true, "context": context }),
    }
}

fn main() -> io::Result<()> {
    panic::set_hook(Box::new(record_panic));
    let stdin: io::Stdin = io::stdin();
    let mut stdout: io::StdoutLock<'_> = io::stdout().lock();
    let mut harness: Harness = Harness::default();
    for line in stdin.lock().lines() {
        let line: String = line?;
        if line.trim().is_empty() {
            continue;
        }
        let raw: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
        let response: Value = match Command::deserialize(&raw) {
            Ok(command) => match harness.handle(command) {
                Some(response) => response,
                None => return Ok(()),
            },
            Err(error) => malformed_command_response(&raw, &error),
        };
        serde_json::to_writer(&mut stdout, &response).map_err(io::Error::other)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Case, CaseOutcome, Command, Harness, OutputFormat, SpecVersion, TestOutcome,
        ValidationError, ValidationErrorKind, ValidationResult, catch_panic, classify,
        dialect_spec_version, run_case, run_response,
    };
    use json_schema_rs::JsonPointer;
    use serde::Deserialize;
    use serde_json::{Value, json};

    fn case(value: &Value) -> Case {
        Case::deserialize(value).expect("test case should deserialize")
    }

    fn run_flag(case_json: &Value) -> CaseOutcome {
        run_case(
            &case(case_json),
            SpecVersion::Draft202012,
            OutputFormat::Flag,
        )
    }

    #[test]
    fn dialect_maps_2020_12() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft202012);
        let actual: Option<SpecVersion> =
            dialect_spec_version("https://json-schema.org/draft/2020-12/schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_maps_2019_09() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft201909);
        let actual: Option<SpecVersion> =
            dialect_spec_version("https://json-schema.org/draft/2019-09/schema");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_maps_draft_07() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft07);
        let actual: Option<SpecVersion> =
            dialect_spec_version("http://json-schema.org/draft-07/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_maps_draft_06() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft06);
        let actual: Option<SpecVersion> =
            dialect_spec_version("http://json-schema.org/draft-06/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_maps_draft_04() {
        let expected: Option<SpecVersion> = Some(SpecVersion::Draft04);
        let actual: Option<SpecVersion> =
            dialect_spec_version("http://json-schema.org/draft-04/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_rejects_undeclared_draft_03() {
        let expected: Option<SpecVersion> = None;
        let actual: Option<SpecVersion> =
            dialect_spec_version("http://json-schema.org/draft-03/schema#");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_command_sets_harness_dialect() {
        let mut harness: Harness = Harness::default();
        let command: Command = Command::deserialize(
            &json!({"cmd": "dialect", "dialect": "http://json-schema.org/draft-07/schema#"}),
        )
        .expect("dialect command should deserialize");
        let _: Option<Value> = harness.handle(command);
        let expected: Harness = Harness {
            dialect: SpecVersion::Draft07,
        };
        let actual: Harness = harness;
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_command_responds_ok() {
        let command: Command = Command::deserialize(
            &json!({"cmd": "dialect", "dialect": "http://json-schema.org/draft-04/schema#"}),
        )
        .expect("dialect command should deserialize");
        let expected: Option<Value> = Some(json!({"ok": true}));
        let actual: Option<Value> = Harness::default().handle(command);
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_command_responds_not_ok_for_undeclared_dialect() {
        let command: Command = Command::deserialize(
            &json!({"cmd": "dialect", "dialect": "http://json-schema.org/draft-03/schema#"}),
        )
        .expect("dialect command should deserialize");
        let expected: Option<Value> = Some(json!({"ok": false}));
        let actual: Option<Value> = Harness::default().handle(command);
        assert_eq!(expected, actual);
    }

    #[test]
    fn stop_command_ends_session() {
        let command: Command =
            Command::deserialize(&json!({"cmd": "stop"})).expect("stop command should deserialize");
        let expected: Option<Value> = None;
        let actual: Option<Value> = Harness::default().handle(command);
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_keyword_overrides_session_dialect() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![TestOutcome::Verdict(false)]);
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {"$schema": "http://json-schema.org/draft-04/schema#", "type": "integer"},
            "tests": [{"instance": 1.0}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn session_dialect_applies_without_schema_keyword() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![TestOutcome::Verdict(false)]);
        let actual: CaseOutcome = run_case(
            &case(&json!({"schema": {"type": "integer"}, "tests": [{"instance": 1.0}]})),
            SpecVersion::Draft04,
            OutputFormat::Flag,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn valid_and_invalid_instances_yield_verdicts() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![
            TestOutcome::Verdict(true),
            TestOutcome::Verdict(false),
        ]);
        let actual: CaseOutcome = run_flag(&json!({
            "description": "minimum",
            "schema": {"type": "integer", "minimum": 2},
            "tests": [
                {"description": "above", "instance": 3, "valid": true},
                {"description": "below", "instance": 1, "valid": false}
            ]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_root_keyword_skips_case_with_exact_message() {
        let expected: CaseOutcome =
            CaseOutcome::Skipped("unsupported keyword \"bogus\" at root".to_string());
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {"bogus": {"type": "string"}},
            "tests": [{"instance": 1}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_nested_keyword_skips_case_with_exact_message() {
        let expected: CaseOutcome = CaseOutcome::Skipped(
            "unsupported keyword \"bogus\" at /properties/foo/allOf/1".to_string(),
        );
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {
                "properties": {
                    "bogus": {"type": "string"},
                    "foo": {"allOf": [{"type": "integer"}, {"bogus": {"type": "string"}}]}
                }
            },
            "tests": [{"instance": {}}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn keyword_the_draft_defines_is_parsed_not_skipped() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![TestOutcome::Verdict(true)]);
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {"not": {"type": "string"}},
            "tests": [{"instance": 1}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn true_schema_accepts_every_instance() {
        let expected: CaseOutcome =
            CaseOutcome::Results(vec![TestOutcome::Verdict(true), TestOutcome::Verdict(true)]);
        let actual: CaseOutcome = run_flag(&json!({
            "schema": true,
            "tests": [{"instance": 1}, {"instance": {"a": [null]}}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn false_schema_rejects_every_instance() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![
            TestOutcome::Verdict(false),
            TestOutcome::Verdict(false),
        ]);
        let actual: CaseOutcome = run_flag(&json!({
            "schema": false,
            "tests": [{"instance": 1}, {"instance": {"a": [null]}}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolvable_ref_skips_case() {
        let expected: CaseOutcome = CaseOutcome::Skipped(
            "unsupported $ref: \"#/$defs/missing\": $ref \"#/$defs/missing\" at root \
             (\"json-schema:///#/$defs/missing\") does not resolve: no such location"
                .to_string(),
        );
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {"$ref": "#/$defs/missing"},
            "tests": [{"instance": 1}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn registry_document_resolves_remote_ref() {
        let expected: CaseOutcome = CaseOutcome::Results(vec![
            TestOutcome::Verdict(true),
            TestOutcome::Verdict(false),
        ]);
        let actual: CaseOutcome = run_flag(&json!({
            "schema": {"$ref": "http://localhost:1234/integer.json"},
            "registry": {
                "http://localhost:1234/integer.json": {"type": "integer"}
            },
            "tests": [{"instance": 1}, {"instance": "a"}]
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_pattern_in_schema_is_a_capability_gap() {
        let result: ValidationResult = Err(vec![ValidationError::new(
            ValidationErrorKind::InvalidPatternInSchema {
                pattern: "(".to_string(),
            },
            JsonPointer::root(),
            JsonPointer::try_from("/pattern").expect("pointer"),
        )]);
        let expected: TestOutcome =
            TestOutcome::Skipped("unsupported: root: schema has invalid pattern \"(\"".to_string());
        let actual: TestOutcome = classify(&result);
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_output_skips_every_test() {
        let skipped: TestOutcome =
            TestOutcome::Skipped("annotations are not supported".to_string());
        let expected: CaseOutcome = CaseOutcome::Results(vec![skipped.clone(), skipped]);
        let actual: CaseOutcome = run_case(
            &case(&json!({"schema": {}, "tests": [{"instance": 1}, {"instance": 2}]})),
            SpecVersion::Draft202012,
            OutputFormat::Annotations,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn panic_is_caught_as_error() {
        let expected: Result<(), String> = Err("boom".to_string());
        let actual: Result<(), String> = catch_panic(|| panic!("boom"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn results_response_json() {
        let outcome: CaseOutcome = CaseOutcome::Results(vec![
            TestOutcome::Verdict(true),
            TestOutcome::Skipped("unsupported $ref: x".to_string()),
            TestOutcome::Errored("boom".to_string()),
        ]);
        let expected: Value = json!({
            "seq": 7,
            "results": [
                {"valid": true},
                {"skipped": true, "message": "unsupported $ref: x"},
                {"errored": true, "context": {"message": "boom"}}
            ]
        });
        let actual: Value = run_response(&json!(7), outcome);
        assert_eq!(expected, actual);
    }

    #[test]
    fn skipped_case_response_json() {
        let outcome: CaseOutcome =
            CaseOutcome::Skipped("unsupported keyword \"not\" at root".to_string());
        let expected: Value = json!({
            "seq": "abc",
            "skipped": true,
            "message": "unsupported keyword \"not\" at root"
        });
        let actual: Value = run_response(&json!("abc"), outcome);
        assert_eq!(expected, actual);
    }

    #[test]
    fn errored_case_response_json() {
        let outcome: CaseOutcome = CaseOutcome::Errored("boom".to_string());
        let expected: Value = json!({"seq": 1, "errored": true, "context": {"message": "boom"}});
        let actual: Value = run_response(&json!(1), outcome);
        assert_eq!(expected, actual);
    }
}
