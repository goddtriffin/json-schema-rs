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
//!   - `jsonschemars`: `CompiledSchema::validate` (collects every error) on the schema
//!     compiled at compile time (registry built, then compiled);
//!   - `jsonschema`: `Validator::iter_errors` collected into a `Vec`;
//!   - `boon`: `Schemas::validate` (builds the full error tree).
//!
//! Every tool runs with its default settings, except that typify codegen mirrors the
//! `cargo-typify` CLI defaults (builder-style interface enabled), so the in-process and
//! CLI tiers generate the same code.
//!
//! A fixture's vendored resources (documents its schema references, see
//! [`crate::fixtures::Resource`]) are handed to each tool the way it accepts extra documents,
//! each under its root `$id`, as JSON values parsed from text inside the measured work (as the
//! schema is): `jsonschemars` through its `$ref` registry (`RegistryBuilder::document` to
//! compile; an in-memory `Retrieve`r to generate code, so only the schema gets root types, as
//! with every other fixture), `jsonschema` through a `referencing::Registry`, `boon` through
//! `Compiler::add_resource`. typify cannot take extra documents (it rejects every external
//! `$ref`), so its codegen on such a fixture is a compatibility failure.
//!
//! Tool panics (typify panics on some schemas) are caught and reported as errors so one
//! failing cell never aborts a run. [`check`] decides whether a (tool, fixture, operation)
//! cell is correct, and only correct cells are timed or profiled.

use crate::fixtures::{Fixture, Resource};
use json_schema_rs::{
    CodeGenInput, CodeGenSettings, CompiledSchema, GenerateRustOutput, JsonSchema, Registry,
    RegistryBuilder, Schema, generate_rust,
};
use serde_json::Value;
use std::any::Any;
use std::collections::BTreeMap;
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
    /// `json-schema-rs` compiled schema (built through its `$ref` registry).
    JsonSchemaRs(CompiledSchema),
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
    /// The tool cannot take the fixture's resource documents.
    ResourcesUnsupported,
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
            AdapterError::ResourcesUnsupported => f.write_str(RESOURCES_UNSUPPORTED),
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

/// Compile schema text with `tool`, ready for [`validate`], with `resources` registered under
/// their URIs.
///
/// # Errors
///
/// Returns [`AdapterError::Unsupported`] for a tool without validation, and
/// [`AdapterError::SchemaParse`], [`AdapterError::Compile`], or [`AdapterError::Panic`] when
/// the tool cannot compile the schema.
pub fn compile(
    tool: Tool,
    schema_text: &str,
    resources: &[Resource],
) -> Result<Compiled, AdapterError> {
    guarded(|| match tool {
        Tool::JsonSchemaRs => {
            let schema: Schema = serde_json::from_str::<Schema>(schema_text)
                .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
            let mut builder: RegistryBuilder<'static> = Registry::builder();
            for resource in resources {
                builder =
                    builder.document(resource.uri.clone(), parse_schema_value(&resource.text)?);
            }
            let registry: Registry<'static> = builder
                .build_owned(schema)
                .map_err(|e| AdapterError::Compile(e.to_string()))?;
            CompiledSchema::new(&registry)
                .map(Compiled::JsonSchemaRs)
                .map_err(|e| AdapterError::Compile(e.to_string()))
        }
        Tool::Jsonschema => {
            let schema: Value = parse_schema_value(schema_text)?;
            let compile_err = |e: jsonschema::ValidationError<'_>| {
                AdapterError::Compile(stable_jsonschema_message(&e.to_string()))
            };
            if resources.is_empty() {
                return jsonschema::validator_for(&schema)
                    .map(Compiled::Jsonschema)
                    .map_err(compile_err);
            }
            let referencing_err =
                |e: jsonschema::ReferencingError| AdapterError::Compile(e.to_string());
            let mut builder: jsonschema::RegistryBuilder<'_> = jsonschema::Registry::new();
            for resource in resources {
                builder = builder
                    .add(resource.uri.as_str(), parse_schema_value(&resource.text)?)
                    .map_err(referencing_err)?;
            }
            let registry: jsonschema::Registry<'_> = builder.prepare().map_err(referencing_err)?;
            jsonschema::options()
                .with_registry(&registry)
                .build(&schema)
                .map(Compiled::Jsonschema)
                .map_err(compile_err)
        }
        Tool::Boon => {
            let schema: Value = parse_schema_value(schema_text)?;
            let mut schemas: boon::Schemas = boon::Schemas::new();
            let mut compiler: boon::Compiler = boon::Compiler::new();
            compiler
                .add_resource(BOON_SCHEMA_LOC, schema)
                .map_err(|e: boon::CompileError| AdapterError::Compile(e.to_string()))?;
            for resource in resources {
                compiler
                    .add_resource(&resource.uri, parse_schema_value(&resource.text)?)
                    .map_err(|e: boon::CompileError| AdapterError::Compile(e.to_string()))?;
            }
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
        Compiled::JsonSchemaRs(compiled) => compiled.validate(instance).is_ok(),
        Compiled::Jsonschema(validator) => {
            let errors: Vec<jsonschema::ValidationError<'_>> =
                validator.iter_errors(instance).collect();
            errors.is_empty()
        }
        Compiled::Boon { schemas, index } => schemas.validate(instance, *index).is_ok(),
    }
}

/// Generate Rust source from schema text with `tool`, with `resources` reachable by their URIs.
///
/// # Errors
///
/// Returns [`AdapterError::Unsupported`] for a tool without codegen,
/// [`AdapterError::ResourcesUnsupported`] when `resources` is not empty and the tool cannot take
/// extra documents, and [`AdapterError::SchemaParse`], [`AdapterError::Codegen`], or
/// [`AdapterError::Panic`] when generation fails.
pub fn codegen(
    tool: Tool,
    schema_text: &str,
    resources: &[Resource],
) -> Result<String, AdapterError> {
    guarded(|| match tool {
        Tool::JsonSchemaRs => codegen_jsonschemars(schema_text, resources),
        Tool::Typify if !resources.is_empty() => Err(AdapterError::ResourcesUnsupported),
        Tool::Typify => codegen_typify(schema_text),
        Tool::Jsonschema | Tool::Boon => Err(AdapterError::Unsupported {
            tool,
            operation: Operation::Codegen,
        }),
    })
}

/// Our codegen for the schema alone, its `$ref`s to other documents served from `resources` by
/// an in-memory retriever: the resources are documents the schema reaches, not inputs of their
/// own, as in every validating tool's registry.
fn codegen_jsonschemars(schema_text: &str, resources: &[Resource]) -> Result<String, AdapterError> {
    let schema: JsonSchema = serde_json::from_str(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
    let settings: CodeGenSettings = CodeGenSettings::builder().build();
    let mut input: CodeGenInput<'_> = CodeGenInput::new().schema(&schema);
    if !resources.is_empty() {
        let mut documents: BTreeMap<String, Value> = BTreeMap::new();
        for resource in resources {
            documents.insert(resource.uri.clone(), parse_schema_value(&resource.text)?);
        }
        input = input.retriever(documents);
    }
    let output: GenerateRustOutput =
        generate_rust(input, &settings).map_err(|e| AdapterError::Codegen(e.to_string()))?;
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

/// Compat detail for a cell left unmeasured because a `$ref` in the fixture names a document the
/// fixture does not vendor; the benchmark runs offline (see [`Fixture::unresolved_refs`]). The
/// CLI tier prints the same text, so both tiers report it identically.
pub const OFFLINE_SKIP: &str =
    "not measured: schema needs external documents the fixture does not vendor (offline run)";

/// Compat detail for a cell whose tool has no way to take the fixture's resource documents (no
/// API or CLI option for extra schema documents). The CLI tier prints the same text.
pub const RESOURCES_UNSUPPORTED: &str = "cannot load the fixture's resource documents";

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
    if !fixture.unresolved_refs().is_empty() {
        return Err(OFFLINE_SKIP.to_owned());
    }
    let resources: &[Resource] = &fixture.resources;
    match operation {
        // Generated source that is not a Rust file is not a successful generation.
        Operation::Codegen => codegen(tool, &fixture.schema_text, resources)
            .and_then(|source: String| {
                syn::parse_file(&source)
                    .map(|_file: syn::File| ())
                    .map_err(|e: syn::Error| {
                        AdapterError::Codegen(format!("generated source is not Rust: {e}"))
                    })
            })
            .map_err(|e: AdapterError| e.to_string()),
        Operation::SchemaCompile => compile(tool, &fixture.schema_text, resources)
            .map(|_compiled: Compiled| ())
            .map_err(|e: AdapterError| e.to_string()),
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let text: &str = operation
                .instance(fixture)
                .ok_or_else(|| AdapterError::MissingInstance(operation).to_string())?;
            let instance: Value = parse_instance(text).map_err(|e: AdapterError| e.to_string())?;
            let compiled: Compiled = compile(tool, &fixture.schema_text, resources)
                .map_err(|e: AdapterError| e.to_string())?;
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
    use crate::fixtures::{Fixture, Resource, default_roots, discover};
    use serde_json::Value;
    use std::path::PathBuf;

    /// A schema whose `name` property is defined in another document, [`NAME_RESOURCE`].
    const SCHEMA_WITH_REF: &str = r#"{
        "$id": "https://example.com/root.json",
        "type": "object",
        "properties": { "name": { "$ref": "name.json" } },
        "required": ["name"]
    }"#;

    /// The document [`SCHEMA_WITH_REF`] references.
    const NAME_RESOURCE: &str = r#"{
        "$id": "https://example.com/name.json",
        "title": "Name",
        "type": "object",
        "properties": { "first": { "type": "string" } },
        "required": ["first"]
    }"#;

    fn name_resource() -> Vec<Resource> {
        vec![Resource {
            uri: "https://example.com/name.json".to_owned(),
            path: PathBuf::new(),
            text: NAME_RESOURCE.to_owned(),
        }]
    }

    /// [`SCHEMA_WITH_REF`] with [`NAME_RESOURCE`] vendored.
    fn fixture_with_resource() -> Fixture {
        Fixture {
            resources: name_resource(),
            ..fixture(
                SCHEMA_WITH_REF,
                Some(r#"{"name":{"first":"a"}}"#),
                Some(r#"{"name":{"first":1}}"#),
            )
        }
    }

    /// Every supported cell's check on `fixture`, in tool then operation order.
    fn check_every_supported_cell(fixture: &Fixture) -> Vec<Result<(), String>> {
        Tool::ALL
            .iter()
            .flat_map(|tool: &Tool| {
                Operation::ALL
                    .iter()
                    .map(move |op: &Operation| (*tool, *op))
            })
            .filter(|(tool, op): &(Tool, Operation)| tool.supports(*op))
            .map(|(tool, op): (Tool, Operation)| check(tool, fixture, op))
            .collect()
    }

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
            resources: Vec::new(),
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
        let compiled: Compiled = compile(tool, SCHEMA, &[]).expect("compile");
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
        let actual: Result<Compiled, AdapterError> =
            compile(Tool::Jsonschema, r#"{"type": 5}"#, &[]);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn boon_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> = compile(Tool::Boon, r#"{"type": 5}"#, &[]);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn compile_rejects_malformed_json() {
        let actual: Result<Compiled, AdapterError> = compile(Tool::Boon, "{", &[]);
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn typify_compile_unsupported() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: Tool::Typify,
            operation: Operation::SchemaCompile,
        };
        let actual: AdapterError = compile(Tool::Typify, SCHEMA, &[]).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen() {
        let expected: String = "\
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"
        .to_owned();
        let actual: String = codegen(Tool::JsonSchemaRs, SCHEMA, &[]).expect("codegen");
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
        let actual: Result<String, AdapterError> = codegen(Tool::Typify, "[]", &[]);
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn jsonschemars_codegen_non_object_root_is_type_alias() {
        let expected: String = "\
//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

"
        .to_owned();
        let actual: String =
            codegen(Tool::JsonSchemaRs, r#"{"type":"string"}"#, &[]).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen_rejects_unresolvable_ref() {
        let actual: Result<String, AdapterError> =
            codegen(Tool::JsonSchemaRs, r##"{"$ref":"#/$defs/missing"}"##, &[]);
        assert!(matches!(actual, Err(AdapterError::Codegen(_))));
    }

    #[test]
    fn jsonschemars_codegen_generates_types_from_a_resource() {
        let expected: String = "\
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = \"Name\")]
#[json_schema(id = \"https://example.com/name.json\")]
pub struct Name {
    pub first: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = \"https://example.com/root.json\")]
pub struct Root {
    pub name: Name,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"
        .to_owned();
        let actual: String =
            codegen(Tool::JsonSchemaRs, SCHEMA_WITH_REF, &name_resource()).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen_without_its_resource_fails() {
        let actual: Result<String, AdapterError> =
            codegen(Tool::JsonSchemaRs, SCHEMA_WITH_REF, &[]);
        assert!(matches!(actual, Err(AdapterError::Codegen(_))));
    }

    #[test]
    fn jsonschemars_compile_without_its_resource_fails() {
        let actual: Result<Compiled, AdapterError> =
            compile(Tool::JsonSchemaRs, SCHEMA_WITH_REF, &[]);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn typify_codegen_rejects_resources() {
        let expected: AdapterError = AdapterError::ResourcesUnsupported;
        let actual: AdapterError =
            codegen(Tool::Typify, SCHEMA_WITH_REF, &name_resource()).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn codegen_unsupported_for_boon() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: Tool::Boon,
            operation: Operation::Codegen,
        };
        let actual: AdapterError = codegen(Tool::Boon, SCHEMA, &[]).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_every_supported_cell() {
        let expected: Vec<Result<(), String>> = vec![Ok(()); 11];
        let actual: Vec<Result<(), String>> = check_every_supported_cell(&good_fixture());
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_skips_a_fixture_with_unresolved_refs() {
        let offline: Fixture = fixture(
            r#"{"properties":{"a":{"$ref":"https://example.com/a.json"}}}"#,
            Some("{}"),
            Some("{}"),
        );
        let expected: Result<(), String> = Err(
            "not measured: schema needs external documents the fixture does not vendor (offline run)"
                .to_owned(),
        );
        let actual: Result<(), String> =
            check(Tool::Jsonschema, &offline, Operation::SchemaCompile);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_every_cell_through_resources_but_typify_codegen() {
        let mut expected: Vec<Result<(), String>> = vec![Ok(()); 11];
        expected[10] = Err("cannot load the fixture's resource documents".to_owned());
        let actual: Vec<Result<(), String>> = check_every_supported_cell(&fixture_with_resource());
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_committed_package_json_but_typify_codegen() {
        let committed: PathBuf = default_roots().into_iter().next().expect("committed root");
        let package_json: Fixture = discover(&[committed])
            .expect("discover")
            .into_iter()
            .find(|fixture: &Fixture| fixture.id == "medium/package-json")
            .expect("package-json fixture");
        let mut expected: Vec<Result<(), String>> = vec![Ok(()); 11];
        expected[10] = Err("cannot load the fixture's resource documents".to_owned());
        let actual: Vec<Result<(), String>> = check_every_supported_cell(&package_json);
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
