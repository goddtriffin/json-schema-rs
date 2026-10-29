//! Uniform in-process adapters over every benchmarked tool, linked as a library.
//!
//! The criterion bench (`benches/cross_tool.rs`) and the heap profiler
//! (`src/bin/heap_profile.rs`) share these functions so both measure exactly the same work:
//!
//! * [`codegen`] — schema JSON text -> Rust source text (`jsonschemars`, `typify`).
//! * [`compile`] — schema JSON text -> compiled, ready-to-validate schema (`jsonschemars`,
//!   `jsonschema`, `boon`).
//! * [`validate`] — compiled schema + parsed instance -> verdict, through each tool's full
//!   error-report entry point:
//!   - `jsonschemars`: `json_schema_rs::validate_with_registry` (collects every error) on the
//!     registry built at compile time;
//!   - `jsonschema`: `Validator::iter_errors` collected into a `Vec`;
//!   - `boon`: `Schemas::validate` (builds the full error tree).
//!
//! Every tool runs with its default settings, except that typify codegen mirrors the
//! `cargo-typify` CLI defaults (builder-style interface enabled), so the in-process and
//! CLI tiers generate the same code.
//!
//! Tool panics (typify panics on some schemas) are caught and reported as errors so one
//! failing cell never aborts a run. [`check`] decides whether a (tool, fixture, operation)
//! cell is correct, and only correct cells are timed or profiled.

use crate::fixtures::Fixture;
use json_schema_rs::{
    CodeGenSettings, GenerateRustOutput, JsonSchema, Registry, Schema, generate_rust,
};
use serde_json::Value;
use std::any::Any;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// A synthetic URL under which a schema document is registered with boon, so the compiler
/// never touches the filesystem or network for the root document.
const BOON_SCHEMA_LOC: &str = "mem://benchmark/schema.json";

/// A benchmarked tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tool {
    /// This repository's `json-schema-rs` library.
    JsonSchemaRs,
    /// The `jsonschema` crate.
    Jsonschema,
    /// The `boon` crate.
    Boon,
    /// The `typify` crate.
    Typify,
}

impl Tool {
    /// Every tool, in report order.
    pub const ALL: [Tool; 4] = [
        Tool::JsonSchemaRs,
        Tool::Jsonschema,
        Tool::Boon,
        Tool::Typify,
    ];

    /// Stable id token used in benchmark ids and raw capture files.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Tool::JsonSchemaRs => "jsonschemars",
            Tool::Jsonschema => "jsonschema",
            Tool::Boon => "boon",
            Tool::Typify => "typify",
        }
    }

    /// Whether this tool implements `operation`.
    #[must_use]
    pub const fn supports(self, operation: Operation) -> bool {
        match operation {
            Operation::Codegen => matches!(self, Tool::JsonSchemaRs | Tool::Typify),
            Operation::SchemaCompile | Operation::ValidateValid | Operation::ValidateInvalid => {
                matches!(self, Tool::JsonSchemaRs | Tool::Jsonschema | Tool::Boon)
            }
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A benchmarked operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    /// Schema JSON text -> Rust source text.
    Codegen,
    /// Schema JSON text -> compiled, ready-to-validate schema.
    SchemaCompile,
    /// Compiled schema + parsed instance -> verdict, on the fixture's valid instance.
    ValidateValid,
    /// Compiled schema + parsed instance -> verdict, on the fixture's invalid instance.
    ValidateInvalid,
}

impl Operation {
    /// Every operation, in report order.
    pub const ALL: [Operation; 4] = [
        Operation::Codegen,
        Operation::SchemaCompile,
        Operation::ValidateValid,
        Operation::ValidateInvalid,
    ];

    /// Stable id token used as the criterion group name and in raw capture files.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Operation::Codegen => "codegen",
            Operation::SchemaCompile => "schema_compile",
            Operation::ValidateValid => "validate_valid",
            Operation::ValidateInvalid => "validate_invalid",
        }
    }

    /// The instance text a validation operation runs on (`None` for non-validation
    /// operations, or when the fixture lacks that instance).
    #[must_use]
    pub fn instance(self, fixture: &Fixture) -> Option<&str> {
        match self {
            Operation::Codegen | Operation::SchemaCompile => None,
            Operation::ValidateValid => fixture.valid_instance.as_deref(),
            Operation::ValidateInvalid => fixture.invalid_instance.as_deref(),
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A schema compiled by one of the validating tools.
///
/// Variants hold each tool's native compiled value unboxed so compile timings include no
/// adapter-added allocation.
pub enum Compiled {
    /// `json-schema-rs` parsed schema document (an object or a boolean schema) in its `$ref`
    /// registry.
    JsonSchemaRs(Registry<'static>),
    /// `jsonschema` validator.
    Jsonschema(jsonschema::Validator),
    /// `boon` schema collection and the root schema's index.
    Boon {
        /// Compiled schemas.
        schemas: boon::Schemas,
        /// Index of the root schema within `schemas`.
        index: boon::SchemaIndex,
    },
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tool: Tool = match self {
            Compiled::JsonSchemaRs(_) => Tool::JsonSchemaRs,
            Compiled::Jsonschema(_) => Tool::Jsonschema,
            Compiled::Boon { .. } => Tool::Boon,
        };
        write!(f, "Compiled({tool})")
    }
}

/// Failure of an adapter call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// The tool does not implement the operation.
    Unsupported {
        /// Tool asked to run the operation.
        tool: Tool,
        /// Requested operation.
        operation: Operation,
    },
    /// The fixture lacks the instance a validation operation needs.
    MissingInstance(Operation),
    /// Schema text could not be parsed into the tool's input model.
    SchemaParse(String),
    /// Instance text is not valid JSON.
    InstanceParse(String),
    /// The tool rejected the schema while compiling it.
    Compile(String),
    /// The tool failed to generate code.
    Codegen(String),
    /// The tool panicked.
    Panic(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Unsupported { tool, operation } => {
                write!(f, "{tool} does not support {operation}")
            }
            AdapterError::MissingInstance(operation) => {
                write!(f, "fixture has no instance for {operation}")
            }
            AdapterError::SchemaParse(message) => write!(f, "schema parse error: {message}"),
            AdapterError::InstanceParse(message) => write!(f, "instance parse error: {message}"),
            AdapterError::Compile(message) => write!(f, "compile error: {message}"),
            AdapterError::Codegen(message) => write!(f, "codegen error: {message}"),
            AdapterError::Panic(message) => write!(f, "panic: {message}"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// Render a caught panic payload as text.
fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

/// Run `f`, converting a panic into [`AdapterError::Panic`].
fn guarded<T>(f: impl FnOnce() -> Result<T, AdapterError>) -> Result<T, AdapterError> {
    catch_unwind(AssertUnwindSafe(f))
        .unwrap_or_else(|payload| Err(AdapterError::Panic(panic_message(payload.as_ref()))))
}

/// Parse instance text into a JSON value.
///
/// # Errors
///
/// Returns [`AdapterError::InstanceParse`] when `text` is not valid JSON.
pub fn parse_instance(text: &str) -> Result<Value, AdapterError> {
    serde_json::from_str(text)
        .map_err(|e: serde_json::Error| AdapterError::InstanceParse(e.to_string()))
}

/// Compile schema text with `tool`, ready for [`validate`].
///
/// # Errors
///
/// Returns [`AdapterError::Unsupported`] for a tool without validation, and
/// [`AdapterError::SchemaParse`], [`AdapterError::Compile`], or [`AdapterError::Panic`] when
/// the tool cannot compile the schema.
pub fn compile(tool: Tool, schema_text: &str) -> Result<Compiled, AdapterError> {
    guarded(|| match tool {
        Tool::JsonSchemaRs => {
            let schema: Schema = serde_json::from_str::<Schema>(schema_text)
                .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
            Registry::builder()
                .build_owned(schema)
                .map(Compiled::JsonSchemaRs)
                .map_err(|e| AdapterError::Compile(e.to_string()))
        }
        Tool::Jsonschema => {
            let schema: Value = parse_schema_value(schema_text)?;
            jsonschema::validator_for(&schema)
                .map(Compiled::Jsonschema)
                .map_err(|e| AdapterError::Compile(stable_jsonschema_message(&e.to_string())))
        }
        Tool::Boon => {
            let schema: Value = parse_schema_value(schema_text)?;
            let mut schemas: boon::Schemas = boon::Schemas::new();
            let mut compiler: boon::Compiler = boon::Compiler::new();
            compiler
                .add_resource(BOON_SCHEMA_LOC, schema)
                .map_err(|e: boon::CompileError| AdapterError::Compile(e.to_string()))?;
            let index: boon::SchemaIndex = compiler
                .compile(BOON_SCHEMA_LOC, &mut schemas)
                .map_err(|e: boon::CompileError| AdapterError::Compile(e.to_string()))?;
            Ok(Compiled::Boon { schemas, index })
        }
        Tool::Typify => Err(AdapterError::Unsupported {
            tool,
            operation: Operation::SchemaCompile,
        }),
    })
}

/// Drop the resource URL from jsonschema's "resource not present in a registry" error.
///
/// jsonschema reports whichever unresolvable external resource it meets first, and that
/// order varies between runs, so the URL would make the compatibility detail
/// nondeterministic.
fn stable_jsonschema_message(message: &str) -> String {
    const MARKER: &str = "' is not present in a registry";
    match (message.strip_prefix("Resource '"), message.find(MARKER)) {
        (Some(_), Some(at)) => format!("external resource{}", &message[at + 1..]),
        _ => message.to_owned(),
    }
}

fn parse_schema_value(schema_text: &str) -> Result<Value, AdapterError> {
    serde_json::from_str(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))
}

/// Validate `instance` against `compiled` through the tool's full error-report API.
///
/// Returns `true` when the instance is valid.
#[must_use]
pub fn validate(compiled: &Compiled, instance: &Value) -> bool {
    match compiled {
        Compiled::JsonSchemaRs(registry) => {
            json_schema_rs::validate_with_registry(registry, instance).is_ok()
        }
        Compiled::Jsonschema(validator) => {
            let errors: Vec<jsonschema::ValidationError<'_>> =
                validator.iter_errors(instance).collect();
            errors.is_empty()
        }
        Compiled::Boon { schemas, index } => schemas.validate(instance, *index).is_ok(),
    }
}

/// Generate Rust source from schema text with `tool`.
///
/// # Errors
///
/// Returns [`AdapterError::Unsupported`] for a tool without codegen, and
/// [`AdapterError::SchemaParse`], [`AdapterError::Codegen`], or [`AdapterError::Panic`] when
/// generation fails.
pub fn codegen(tool: Tool, schema_text: &str) -> Result<String, AdapterError> {
    guarded(|| match tool {
        Tool::JsonSchemaRs => codegen_jsonschemars(schema_text),
        Tool::Typify => codegen_typify(schema_text),
        Tool::Jsonschema | Tool::Boon => Err(AdapterError::Unsupported {
            tool,
            operation: Operation::Codegen,
        }),
    })
}

fn codegen_jsonschemars(schema_text: &str) -> Result<String, AdapterError> {
    let schema: JsonSchema = serde_json::from_str(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
    let settings: CodeGenSettings = CodeGenSettings::builder().build();
    let output: GenerateRustOutput = generate_rust(std::slice::from_ref(&schema), &settings)
        .map_err(|e| AdapterError::Codegen(e.to_string()))?;
    let mut source: Vec<u8> = output.shared.unwrap_or_default();
    for buffer in output.per_schema {
        source.extend_from_slice(&buffer);
    }
    String::from_utf8(source)
        .map_err(|e: std::string::FromUtf8Error| AdapterError::Codegen(e.to_string()))
}

/// typify codegen as the `cargo-typify` CLI performs it with default flags.
fn codegen_typify(schema_text: &str) -> Result<String, AdapterError> {
    let schema: schemars::schema::RootSchema = serde_json::from_str(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
    let mut settings: typify::TypeSpaceSettings = typify::TypeSpaceSettings::default();
    settings.with_struct_builder(true);
    let mut type_space: typify::TypeSpace = typify::TypeSpace::new(&settings);
    type_space
        .add_root_schema(schema)
        .map_err(|e: typify::Error| AdapterError::Codegen(e.to_string()))?;
    let file: syn::File = syn::parse2::<syn::File>(type_space.to_stream())
        .map_err(|e: syn::Error| AdapterError::Codegen(e.to_string()))?;
    Ok(prettyplease::unparse(&file))
}

/// Compat detail for a cell left unmeasured because the fixture's schema has external
/// `$ref`s; the benchmark runs offline (see [`Fixture::external_refs`]). The CLI tier prints the
/// same text, so both tiers report it identically.
pub const OFFLINE_SKIP: &str = "not measured: schema needs external documents (offline run)";

/// Compat detail for a `validate_invalid` cell left unmeasured because the same tool rejects the
/// fixture's valid instance. The CLI tier prints the same text.
pub const VALID_REJECTED_SKIP: &str = "not measured: valid instance was rejected";

/// Verify `tool` is correct on `fixture` for `operation`: codegen succeeds, the schema
/// compiles, the valid instance is accepted, or the invalid instance is rejected.
///
/// # Errors
///
/// Returns a short one-line reason when the cell is unsupported, the fixture lacks the
/// needed instance, the tool fails or panics, or the verdict is wrong.
pub fn check(tool: Tool, fixture: &Fixture, operation: Operation) -> Result<(), String> {
    check_inner(tool, fixture, operation).map_err(|reason: String| one_line(&reason))
}

fn check_inner(tool: Tool, fixture: &Fixture, operation: Operation) -> Result<(), String> {
    if !tool.supports(operation) {
        return Err(AdapterError::Unsupported { tool, operation }.to_string());
    }
    let external: Vec<String> = fixture.external_refs();
    if !external.is_empty() {
        return Err(OFFLINE_SKIP.to_owned());
    }
    match operation {
        Operation::Codegen => codegen(tool, &fixture.schema_text)
            .map(|_source: String| ())
            .map_err(|e: AdapterError| e.to_string()),
        Operation::SchemaCompile => compile(tool, &fixture.schema_text)
            .map(|_compiled: Compiled| ())
            .map_err(|e: AdapterError| e.to_string()),
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let text: &str = operation
                .instance(fixture)
                .ok_or_else(|| AdapterError::MissingInstance(operation).to_string())?;
            let instance: Value = parse_instance(text).map_err(|e: AdapterError| e.to_string())?;
            let compiled: Compiled =
                compile(tool, &fixture.schema_text).map_err(|e: AdapterError| e.to_string())?;
            let expected_valid: bool = operation == Operation::ValidateValid;
            // Rejecting the invalid instance proves nothing when the tool also rejects the valid
            // one (e.g. it rejects everything), so that cell is not measured. The CLI tier
            // applies the same rule.
            if !expected_valid && let Some(valid_text) = fixture.valid_instance.as_deref() {
                let valid_instance: Value =
                    parse_instance(valid_text).map_err(|e: AdapterError| e.to_string())?;
                let accepted: bool = guarded(|| Ok(validate(&compiled, &valid_instance)))
                    .map_err(|e: AdapterError| e.to_string())?;
                if !accepted {
                    return Err(VALID_REJECTED_SKIP.to_owned());
                }
            }
            let valid: bool = guarded(|| Ok(validate(&compiled, &instance)))
                .map_err(|e: AdapterError| e.to_string())?;
            if valid == expected_valid {
                Ok(())
            } else if expected_valid {
                Err("wrong verdict: valid instance rejected".to_owned())
            } else {
                Err("wrong verdict: invalid instance accepted".to_owned())
            }
        }
    }
}

/// Collapse whitespace runs (tabs, newlines) to single spaces so a reason fits a TSV cell.
#[must_use]
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        AdapterError, Compiled, Operation, Tool, check, codegen, compile, one_line, validate,
    };
    use crate::fixtures::Fixture;
    use serde_json::Value;
    use std::path::PathBuf;

    const SCHEMA: &str = r#"{
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    }"#;

    fn fixture(schema: &str, valid: Option<&str>, invalid: Option<&str>) -> Fixture {
        Fixture {
            id: "test/inline".to_owned(),
            dir: PathBuf::new(),
            schema_text: schema.to_owned(),
            valid_instance: valid.map(str::to_owned),
            invalid_instance: invalid.map(str::to_owned),
        }
    }

    fn good_fixture() -> Fixture {
        fixture(SCHEMA, Some(r#"{"name":"a"}"#), Some(r#"{"name":1}"#))
    }

    /// A fixture whose "valid" instance is actually invalid and vice versa.
    fn swapped_fixture() -> Fixture {
        fixture(SCHEMA, Some(r#"{"name":1}"#), Some(r#"{"name":"a"}"#))
    }

    fn verdict(tool: Tool, instance: &str) -> bool {
        let compiled: Compiled = compile(tool, SCHEMA).expect("compile");
        let value: Value = serde_json::from_str(instance).expect("instance");
        validate(&compiled, &value)
    }

    #[test]
    fn tool_ids() {
        let expected: Vec<&str> = vec!["jsonschemars", "jsonschema", "boon", "typify"];
        let actual: Vec<&str> = Tool::ALL.iter().map(|tool: &Tool| tool.id()).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn operation_ids() {
        let expected: Vec<&str> = vec![
            "codegen",
            "schema_compile",
            "validate_valid",
            "validate_invalid",
        ];
        let actual: Vec<&str> = Operation::ALL
            .iter()
            .map(|op: &Operation| op.id())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn supported_cells() {
        let expected: Vec<(&str, &str)> = vec![
            ("jsonschemars", "codegen"),
            ("jsonschemars", "schema_compile"),
            ("jsonschemars", "validate_valid"),
            ("jsonschemars", "validate_invalid"),
            ("jsonschema", "schema_compile"),
            ("jsonschema", "validate_valid"),
            ("jsonschema", "validate_invalid"),
            ("boon", "schema_compile"),
            ("boon", "validate_valid"),
            ("boon", "validate_invalid"),
            ("typify", "codegen"),
        ];
        let actual: Vec<(&str, &str)> = Tool::ALL
            .iter()
            .flat_map(|tool: &Tool| {
                Operation::ALL
                    .iter()
                    .filter(|op: &&Operation| tool.supports(**op))
                    .map(|op: &Operation| (tool.id(), op.id()))
            })
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_accepts_valid() {
        let expected: bool = true;
        let actual: bool = verdict(Tool::JsonSchemaRs, r#"{"name":"a"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_rejects_invalid() {
        let expected: bool = false;
        let actual: bool = verdict(Tool::JsonSchemaRs, r#"{"name":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_accepts_valid() {
        let expected: bool = true;
        let actual: bool = verdict(Tool::Jsonschema, r#"{"name":"a"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_rejects_invalid() {
        let expected: bool = false;
        let actual: bool = verdict(Tool::Jsonschema, r#"{"name":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn boon_accepts_valid() {
        let expected: bool = true;
        let actual: bool = verdict(Tool::Boon, r#"{"name":"a"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn boon_rejects_invalid() {
        let expected: bool = false;
        let actual: bool = verdict(Tool::Boon, r#"{"name":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> = compile(Tool::Jsonschema, r#"{"type": 5}"#);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn boon_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> = compile(Tool::Boon, r#"{"type": 5}"#);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn compile_rejects_malformed_json() {
        let actual: Result<Compiled, AdapterError> = compile(Tool::Boon, "{");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn typify_compile_unsupported() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: Tool::Typify,
            operation: Operation::SchemaCompile,
        };
        let actual: AdapterError = compile(Tool::Typify, SCHEMA).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen() {
        let expected: String = "\
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
}

"
        .to_owned();
        let actual: String = codegen(Tool::JsonSchemaRs, SCHEMA).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn typify_codegen_succeeds() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(Tool::Typify, &good_fixture(), Operation::Codegen);
        assert_eq!(expected, actual);
    }

    #[test]
    fn typify_codegen_rejects_malformed_schema() {
        let actual: Result<String, AdapterError> = codegen(Tool::Typify, "[]");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn jsonschemars_codegen_rejects_non_object_root() {
        let actual: Result<String, AdapterError> =
            codegen(Tool::JsonSchemaRs, r#"{"type":"string"}"#);
        assert!(matches!(actual, Err(AdapterError::Codegen(_))));
    }

    #[test]
    fn codegen_unsupported_for_boon() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: Tool::Boon,
            operation: Operation::Codegen,
        };
        let actual: AdapterError = codegen(Tool::Boon, SCHEMA).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_every_supported_cell() {
        let fixture: Fixture = good_fixture();
        let expected: Vec<Result<(), String>> = vec![Ok(()); 11];
        let actual: Vec<Result<(), String>> = Tool::ALL
            .iter()
            .flat_map(|tool: &Tool| {
                Operation::ALL
                    .iter()
                    .map(move |op: &Operation| (*tool, *op))
            })
            .filter(|(tool, op): &(Tool, Operation)| tool.supports(*op))
            .map(|(tool, op): (Tool, Operation)| check(tool, &fixture, op))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_skips_a_fixture_with_external_refs() {
        let offline: Fixture = fixture(
            r#"{"properties":{"a":{"$ref":"https://example.com/a.json"}}}"#,
            Some("{}"),
            Some("{}"),
        );
        let expected: Result<(), String> =
            Err("not measured: schema needs external documents (offline run)".to_owned());
        let actual: Result<(), String> =
            check(Tool::Jsonschema, &offline, Operation::SchemaCompile);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_rejected_valid_instance() {
        let expected: Result<(), String> = Err("wrong verdict: valid instance rejected".to_owned());
        let actual: Result<(), String> =
            check(Tool::Boon, &swapped_fixture(), Operation::ValidateValid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_accepted_invalid_instance() {
        let accepts_both: Fixture =
            fixture(SCHEMA, Some(r#"{"name":"a"}"#), Some(r#"{"name":"b"}"#));
        let expected: Result<(), String> =
            Err("wrong verdict: invalid instance accepted".to_owned());
        let actual: Result<(), String> =
            check(Tool::Jsonschema, &accepts_both, Operation::ValidateInvalid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_skips_validate_invalid_when_valid_instance_is_rejected() {
        let rejects_both: Fixture = fixture(SCHEMA, Some(r#"{"name":1}"#), Some(r#"{"name":2}"#));
        let expected: Result<(), String> =
            Err("not measured: valid instance was rejected".to_owned());
        let actual: Result<(), String> =
            check(Tool::Boon, &rejects_both, Operation::ValidateInvalid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_validate_invalid_without_a_valid_instance() {
        let invalid_only: Fixture = fixture(SCHEMA, None, Some(r#"{"name":1}"#));
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            Tool::JsonSchemaRs,
            &invalid_only,
            Operation::ValidateInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_missing_instance() {
        let expected: Result<(), String> =
            Err("fixture has no instance for validate_invalid".to_owned());
        let actual: Result<(), String> = check(
            Tool::JsonSchemaRs,
            &fixture(SCHEMA, None, None),
            Operation::ValidateInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_unsupported() {
        let expected: Result<(), String> = Err("typify does not support validate_valid".to_owned());
        let actual: Result<(), String> =
            check(Tool::Typify, &good_fixture(), Operation::ValidateValid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn guarded_turns_panic_into_error() {
        let expected: Result<(), AdapterError> = Err(AdapterError::Panic("boom".to_owned()));
        let actual: Result<(), AdapterError> = super::guarded(|| panic!("boom"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn stable_jsonschema_message_drops_resource_url() {
        let expected: String =
            "external resource is not present in a registry and retrieving it failed".to_owned();
        let actual: String = super::stable_jsonschema_message(
            "Resource 'https://x/a.json' is not present in a registry and retrieving it failed",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn stable_jsonschema_message_keeps_other_messages() {
        let expected: String = "5 is not valid".to_owned();
        let actual: String = super::stable_jsonschema_message("5 is not valid");
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_line_collapses_whitespace() {
        let expected: String = "a b c".to_owned();
        let actual: String = one_line("a\tb\n\n c");
        assert_eq!(expected, actual);
    }
}
