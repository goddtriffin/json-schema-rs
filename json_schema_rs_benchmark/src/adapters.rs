//! Uniform in-process adapters over every tool the main benchmark crate links as a library.
//!
//! The criterion bench (`benches/cross_tool.rs`) and the heap profiler
//! (`src/bin/heap_profile.rs`) run these through [`crate::harness`], so both measure exactly the
//! same work:
//!
//! * codegen — schema JSON text -> Rust source text (`jsonschemars`, `typify`, `schemafy`,
//!   `jsonschema_code_generator`).
//! * compile — schema JSON text -> compiled, ready-to-validate schema (`jsonschemars`,
//!   `jsonschema`, `boon`, `valico`, `jsonschema-valid`).
//! * validate — compiled schema + parsed instance -> verdict, through each tool's full
//!   error-report entry point:
//!   - `jsonschemars`: `CompiledSchema::validate` (collects every error) on the schema
//!     compiled at compile time (registry built, then compiled);
//!   - `jsonschema`: `Validator::iter_errors` collected into a `Vec`;
//!   - `boon`: `Schemas::validate` (builds the full error tree);
//!   - `valico`: `ScopedSchema::validate` (collects every error), valid only when strictly valid
//!     (no error and no `$ref` it could not resolve);
//!   - `jsonschema-valid`: `Config::validate` with its error iterator collected into a `Vec`.
//!
//! Every tool runs with its default settings, except that typify codegen mirrors the
//! `cargo-typify` CLI defaults (builder-style interface enabled), so the in-process and
//! CLI tiers generate the same code. Generators that produce a token stream (typify, schemafy,
//! `jsonschema_code_generator`) are pretty-printed with `prettyplease`, so every codegen cell ends
//! at Rust source text. `jsonschema_code_generator` only reads schemas from files, so its timed
//! codegen includes reading the fixture's `schema.json` (from the page cache).
//!
//! A fixture's vendored resources (documents its schema references, see
//! [`crate::fixtures::Resource`]) are handed to each tool the way it accepts extra documents,
//! each under its root `$id`, as JSON values parsed from text inside the measured work (as the
//! schema is): `jsonschemars` through its `$ref` registry (`RegistryBuilder::document` to
//! compile; an in-memory `Retrieve`r to generate code, so only the schema gets root types, as
//! with every other fixture), `jsonschema` through a `referencing::Registry`, `boon` through
//! `Compiler::add_resource`, `valico` by compiling each into the same `Scope`. typify, schemafy,
//! `jsonschema_code_generator` and jsonschema-valid cannot take extra documents, so their cells on
//! such a fixture are compatibility failures.

use crate::fixtures::{Fixture, Resource, SCHEMA_FILE_NAME};
use crate::harness::{Adapter, AdapterError, Operation, parse_schema_value, unsupported};
use json_schema_rs::{
    CodeGenInput, CodeGenSettings, CompiledSchema, GenerateRustOutput, Registry, RegistryBuilder,
    Schema, generate_rust,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// A synthetic URL under which a schema document is registered with boon, so the compiler
/// never touches the filesystem or network for the root document.
const BOON_SCHEMA_LOC: &str = "mem://benchmark/schema.json";

/// The name of the root type every generator that takes one is given, as our codegen names it.
const ROOT_TYPE_NAME: &str = "Root";

/// A benchmarked tool linked into the main benchmark crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tool {
    /// This repository's `json-schema-rs` library.
    JsonSchemaRs,
    /// The `jsonschema` crate (runtime validator).
    Jsonschema,
    /// The `boon` crate.
    Boon,
    /// The `valico` crate.
    Valico,
    /// The `jsonschema-valid` crate.
    JsonschemaValid,
    /// The `typify` crate.
    Typify,
    /// The `schemafy_lib` crate (the generator behind the `schemafy!` macro).
    Schemafy,
    /// The `jsonschema_code_generator` crate.
    JsonschemaCodeGenerator,
}

impl Tool {
    /// Every tool, in report order.
    pub const ALL: [Tool; 8] = [
        Tool::JsonSchemaRs,
        Tool::Jsonschema,
        Tool::Boon,
        Tool::Valico,
        Tool::JsonschemaValid,
        Tool::Typify,
        Tool::Schemafy,
        Tool::JsonschemaCodeGenerator,
    ];
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

self_cell::self_cell!(
    /// A `jsonschema-valid` configuration together with the schema it borrows.
    pub struct JsonschemaValidCompiled {
        owner: Value,

        #[covariant]
        dependent: JsonschemaValidConfig,
    }
);

/// `jsonschema_valid::Config` with its lifetime as the only parameter, for [`self_cell`].
type JsonschemaValidConfig<'a> = jsonschema_valid::Config<'a>;

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
    /// `valico` scope holding every compiled document, and the root schema's URL in it.
    Valico {
        /// Compiled documents.
        scope: valico::json_schema::Scope,
        /// URL of the root schema within `scope`.
        url: url::Url,
    },
    /// `jsonschema-valid` configuration (its id map) over the owned schema.
    JsonschemaValid(JsonschemaValidCompiled),
}

impl fmt::Debug for Compiled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tool: Tool = match self {
            Compiled::JsonSchemaRs(_) => Tool::JsonSchemaRs,
            Compiled::Jsonschema(_) => Tool::Jsonschema,
            Compiled::Boon { .. } => Tool::Boon,
            Compiled::Valico { .. } => Tool::Valico,
            Compiled::JsonschemaValid(_) => Tool::JsonschemaValid,
        };
        write!(f, "Compiled({tool})")
    }
}

impl Adapter for Tool {
    type Compiled = Compiled;

    const ALL: &'static [Tool] = &Tool::ALL;

    fn id(self) -> &'static str {
        match self {
            Tool::JsonSchemaRs => "jsonschemars",
            Tool::Jsonschema => "jsonschema",
            Tool::Boon => "boon",
            Tool::Valico => "valico",
            Tool::JsonschemaValid => "jsonschema-valid",
            Tool::Typify => "typify",
            Tool::Schemafy => "schemafy",
            Tool::JsonschemaCodeGenerator => "jsonschema_code_generator",
        }
    }

    fn supports(self, operation: Operation) -> bool {
        match operation {
            Operation::Codegen => matches!(
                self,
                Tool::JsonSchemaRs | Tool::Typify | Tool::Schemafy | Tool::JsonschemaCodeGenerator
            ),
            Operation::SchemaCompile | Operation::ValidateValid | Operation::ValidateInvalid => {
                matches!(
                    self,
                    Tool::JsonSchemaRs
                        | Tool::Jsonschema
                        | Tool::Boon
                        | Tool::Valico
                        | Tool::JsonschemaValid
                )
            }
        }
    }

    fn codegen(self, fixture: &Fixture) -> Result<String, AdapterError> {
        let schema_text: &str = &fixture.schema_text;
        let resources: &[Resource] = &fixture.resources;
        match self {
            Tool::JsonSchemaRs => codegen_jsonschemars(schema_text, resources),
            Tool::Typify | Tool::Schemafy | Tool::JsonschemaCodeGenerator
                if !resources.is_empty() =>
            {
                Err(AdapterError::ResourcesUnsupported)
            }
            Tool::Typify => codegen_typify(schema_text),
            Tool::Schemafy => codegen_schemafy(schema_text),
            Tool::JsonschemaCodeGenerator => {
                let path: PathBuf = fixture.dir.join(SCHEMA_FILE_NAME);
                pretty_print(jsonschema_code_generator::generate_token_stream(&path))
            }
            Tool::Jsonschema | Tool::Boon | Tool::Valico | Tool::JsonschemaValid => {
                Err(unsupported(self, Operation::Codegen))
            }
        }
    }

    fn compile(self, fixture: &Fixture) -> Result<Compiled, AdapterError> {
        let schema_text: &str = &fixture.schema_text;
        let resources: &[Resource] = &fixture.resources;
        match self {
            Tool::JsonSchemaRs => compile_jsonschemars(schema_text, resources),
            Tool::Jsonschema => compile_jsonschema(schema_text, resources),
            Tool::Boon => compile_boon(schema_text, resources),
            Tool::Valico => compile_valico(schema_text, resources),
            Tool::JsonschemaValid if !resources.is_empty() => {
                Err(AdapterError::ResourcesUnsupported)
            }
            Tool::JsonschemaValid => compile_jsonschema_valid(schema_text),
            Tool::Typify | Tool::Schemafy | Tool::JsonschemaCodeGenerator => {
                Err(unsupported(self, Operation::SchemaCompile))
            }
        }
    }

    fn validate(compiled: &Compiled, instance: &Value) -> bool {
        match compiled {
            Compiled::JsonSchemaRs(compiled) => compiled.validate(instance).is_ok(),
            Compiled::Jsonschema(validator) => {
                let errors: Vec<jsonschema::ValidationError<'_>> =
                    validator.iter_errors(instance).collect();
                errors.is_empty()
            }
            Compiled::Boon { schemas, index } => schemas.validate(instance, *index).is_ok(),
            Compiled::Valico { scope, url } => scope.resolve(url).is_some_and(
                |schema: valico::json_schema::schema::ScopedSchema<'_>| {
                    schema.validate(instance).is_strictly_valid()
                },
            ),
            Compiled::JsonschemaValid(compiled) => {
                compiled.with_dependent(|_schema: &Value, config: &jsonschema_valid::Config<'_>| {
                    match config.validate(instance) {
                        Ok(()) => true,
                        Err(errors) => {
                            let errors: Vec<jsonschema_valid::ValidationError> = errors.collect();
                            errors.is_empty()
                        }
                    }
                })
            }
        }
    }
}

fn compile_jsonschemars(
    schema_text: &str,
    resources: &[Resource],
) -> Result<Compiled, AdapterError> {
    let schema: Schema = serde_json::from_str::<Schema>(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
    let mut builder: RegistryBuilder<'static> = Registry::builder();
    for resource in resources {
        builder = builder.document(resource.uri.clone(), parse_schema_value(&resource.text)?);
    }
    let registry: Registry<'static> = builder
        .build_owned(schema)
        .map_err(|e| AdapterError::Compile(e.to_string()))?;
    CompiledSchema::new(&registry)
        .map(Compiled::JsonSchemaRs)
        .map_err(|e| AdapterError::Compile(e.to_string()))
}

fn compile_jsonschema(schema_text: &str, resources: &[Resource]) -> Result<Compiled, AdapterError> {
    let schema: Value = parse_schema_value(schema_text)?;
    let compile_err = |e: jsonschema::ValidationError<'_>| {
        AdapterError::Compile(stable_jsonschema_message(&e.to_string()))
    };
    if resources.is_empty() {
        return jsonschema::validator_for(&schema)
            .map(Compiled::Jsonschema)
            .map_err(compile_err);
    }
    let referencing_err = |e: jsonschema::ReferencingError| AdapterError::Compile(e.to_string());
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

fn compile_boon(schema_text: &str, resources: &[Resource]) -> Result<Compiled, AdapterError> {
    let compile_err = |e: boon::CompileError| AdapterError::Compile(e.to_string());
    let schema: Value = parse_schema_value(schema_text)?;
    let mut schemas: boon::Schemas = boon::Schemas::new();
    let mut compiler: boon::Compiler = boon::Compiler::new();
    compiler
        .add_resource(BOON_SCHEMA_LOC, schema)
        .map_err(compile_err)?;
    for resource in resources {
        compiler
            .add_resource(&resource.uri, parse_schema_value(&resource.text)?)
            .map_err(compile_err)?;
    }
    let index: boon::SchemaIndex = compiler
        .compile(BOON_SCHEMA_LOC, &mut schemas)
        .map_err(compile_err)?;
    Ok(Compiled::Boon { schemas, index })
}

fn compile_valico(schema_text: &str, resources: &[Resource]) -> Result<Compiled, AdapterError> {
    let compile_err = |e: valico::json_schema::SchemaError| AdapterError::Compile(e.to_string());
    let schema: Value = parse_schema_value(schema_text)?;
    let mut scope: valico::json_schema::Scope = valico::json_schema::Scope::new();
    for resource in resources {
        scope
            .compile(parse_schema_value(&resource.text)?, false)
            .map_err(compile_err)?;
    }
    let url: url::Url = scope.compile(schema, false).map_err(compile_err)?;
    Ok(Compiled::Valico { scope, url })
}

fn compile_jsonschema_valid(schema_text: &str) -> Result<Compiled, AdapterError> {
    let schema: Value = parse_schema_value(schema_text)?;
    JsonschemaValidCompiled::try_new(schema, |schema: &Value| {
        jsonschema_valid::Config::from_schema(schema, None)
            .map_err(|e: jsonschema_valid::ValidationError| AdapterError::Compile(e.to_string()))
    })
    .map(Compiled::JsonschemaValid)
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

/// Our codegen for the schema alone, its `$ref`s to other documents served from `resources` by
/// an in-memory retriever: the resources are documents the schema reaches, not inputs of their
/// own, as in every validating tool's registry.
fn codegen_jsonschemars(schema_text: &str, resources: &[Resource]) -> Result<String, AdapterError> {
    let schema: Schema = serde_json::from_str(schema_text)
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
    pretty_print(type_space.to_stream())
}

/// schemafy codegen as `schemafy_lib::Generator::generate` performs it, from the schema text
/// instead of a file, with the root type named like ours.
fn codegen_schemafy(schema_text: &str) -> Result<String, AdapterError> {
    let schema: schemafy_lib::Schema = serde_json::from_str(schema_text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))?;
    let mut expander: schemafy_lib::Expander<'_> =
        schemafy_lib::Expander::new(Some(ROOT_TYPE_NAME), "::schemafy_core::", &schema);
    pretty_print(expander.expand(&schema))
}

/// Render a generator's token stream as formatted Rust source.
fn pretty_print(tokens: proc_macro2::TokenStream) -> Result<String, AdapterError> {
    let file: syn::File = syn::parse2::<syn::File>(tokens)
        .map_err(|e: syn::Error| AdapterError::Codegen(e.to_string()))?;
    Ok(prettyplease::unparse(&file))
}

#[cfg(test)]
mod tests {
    use super::{Compiled, Tool};
    use crate::fixtures::{Fixture, Resource, SCHEMA_FILE_NAME, default_roots, discover};
    use crate::harness::{Adapter, AdapterError, Operation, check};
    use serde_json::Value;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SCHEMA: &str = r#"{
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    }"#;

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

    /// Distinguishes the directories of fixtures written by concurrently running tests.
    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    /// A fixture written to a fresh directory (removed on drop), so tools that read the schema
    /// file (`jsonschema_code_generator`) see the same schema as every other tool.
    struct DiskFixture(Fixture);

    impl Drop for DiskFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0.dir);
        }
    }

    fn fixture(schema: &str, valid: Option<&str>, invalid: Option<&str>) -> DiskFixture {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "json-schema-rs-benchmark-test-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create fixture dir");
        std::fs::write(dir.join(SCHEMA_FILE_NAME), schema).expect("write schema");
        DiskFixture(Fixture {
            id: "test/inline".to_owned(),
            dir,
            schema_text: schema.to_owned(),
            valid_instance: valid.map(str::to_owned),
            invalid_instance: invalid.map(str::to_owned),
            resources: Vec::new(),
        })
    }

    fn good_fixture() -> DiskFixture {
        fixture(SCHEMA, Some(r#"{"name":"a"}"#), Some(r#"{"name":1}"#))
    }

    /// A fixture whose "valid" instance is actually invalid and vice versa.
    fn swapped_fixture() -> DiskFixture {
        fixture(SCHEMA, Some(r#"{"name":1}"#), Some(r#"{"name":"a"}"#))
    }

    fn name_resource() -> Vec<Resource> {
        vec![Resource {
            uri: "https://example.com/name.json".to_owned(),
            path: PathBuf::new(),
            text: NAME_RESOURCE.to_owned(),
        }]
    }

    /// [`SCHEMA_WITH_REF`] with [`NAME_RESOURCE`] vendored.
    fn fixture_with_resource() -> DiskFixture {
        let mut disk: DiskFixture = fixture(
            SCHEMA_WITH_REF,
            Some(r#"{"name":{"first":"a"}}"#),
            Some(r#"{"name":{"first":1}}"#),
        );
        disk.0.resources = name_resource();
        disk
    }

    /// Every supported cell's check on `fixture`, in tool then operation order.
    fn check_every_supported_cell(fixture: &Fixture) -> Vec<(&'static str, Result<(), String>)> {
        Tool::ALL
            .iter()
            .flat_map(|tool: &Tool| {
                Operation::ALL
                    .iter()
                    .map(move |op: &Operation| (*tool, *op))
            })
            .filter(|(tool, op): &(Tool, Operation)| tool.supports(*op))
            .map(|(tool, op): (Tool, Operation)| (tool.id(), check(tool, fixture, op)))
            .collect()
    }

    /// The expected result of every supported cell: `Ok` except where `failing` names the tool.
    fn every_cell_passes_except(
        failing: &[&str],
        reason: &str,
    ) -> Vec<(&'static str, Result<(), String>)> {
        Tool::ALL
            .iter()
            .flat_map(|tool: &Tool| {
                Operation::ALL
                    .iter()
                    .filter(|op: &&Operation| tool.supports(**op))
                    .map(|_op: &Operation| {
                        let result: Result<(), String> = if failing.contains(&tool.id()) {
                            Err(reason.to_owned())
                        } else {
                            Ok(())
                        };
                        (tool.id(), result)
                    })
            })
            .collect()
    }

    fn verdict(tool: Tool, instance: &str) -> bool {
        let disk: DiskFixture = good_fixture();
        let compiled: Compiled = tool.compile(&disk.0).expect("compile");
        let value: Value = serde_json::from_str(instance).expect("instance");
        Tool::validate(&compiled, &value)
    }

    fn compile_error(tool: Tool, schema: &str) -> Result<Compiled, AdapterError> {
        tool.compile(&fixture(schema, None, None).0)
    }

    fn codegen(tool: Tool, schema: &str) -> Result<String, AdapterError> {
        tool.codegen(&fixture(schema, None, None).0)
    }

    #[test]
    fn tool_ids() {
        let expected: Vec<&str> = vec![
            "jsonschemars",
            "jsonschema",
            "boon",
            "valico",
            "jsonschema-valid",
            "typify",
            "schemafy",
            "jsonschema_code_generator",
        ];
        let actual: Vec<&str> = Tool::ALL.iter().map(|tool: &Tool| tool.id()).collect();
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
            ("valico", "schema_compile"),
            ("valico", "validate_valid"),
            ("valico", "validate_invalid"),
            ("jsonschema-valid", "schema_compile"),
            ("jsonschema-valid", "validate_valid"),
            ("jsonschema-valid", "validate_invalid"),
            ("typify", "codegen"),
            ("schemafy", "codegen"),
            ("jsonschema_code_generator", "codegen"),
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
    fn valico_accepts_valid() {
        let expected: bool = true;
        let actual: bool = verdict(Tool::Valico, r#"{"name":"a"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn valico_rejects_invalid() {
        let expected: bool = false;
        let actual: bool = verdict(Tool::Valico, r#"{"name":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn valico_rejects_an_instance_reaching_an_unresolvable_ref() {
        let disk: DiskFixture = fixture(
            r#"{"properties":{"a":{"$ref":"https://example.com/missing.json"}}}"#,
            None,
            None,
        );
        let compiled: Compiled = Tool::Valico.compile(&disk.0).expect("compile");
        let expected: bool = false;
        let actual: bool = Tool::validate(&compiled, &serde_json::json!({"a": 1}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_valid_accepts_valid() {
        let expected: bool = true;
        let actual: bool = verdict(Tool::JsonschemaValid, r#"{"name":"a"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_valid_rejects_invalid() {
        let expected: bool = false;
        let actual: bool = verdict(Tool::JsonschemaValid, r#"{"name":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> =
            compile_error(Tool::Jsonschema, r#"{"type": 5}"#);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn boon_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> = compile_error(Tool::Boon, r#"{"type": 5}"#);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn valico_compile_rejects_bad_schema() {
        let actual: Result<Compiled, AdapterError> = compile_error(Tool::Valico, r#"{"type": 5}"#);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn compile_rejects_malformed_json() {
        let actual: Result<Compiled, AdapterError> = compile_error(Tool::Boon, "{");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn jsonschema_valid_compile_rejects_malformed_json() {
        let actual: Result<Compiled, AdapterError> = compile_error(Tool::JsonschemaValid, "{");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn typify_compile_unsupported() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: "typify",
            operation: Operation::SchemaCompile,
        };
        let actual: AdapterError = compile_error(Tool::Typify, SCHEMA).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen() {
        let expected: String = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::deserialize_struct!(Root, additional: BTreeMap<String, serde_json::Value>, {
    "name" => name: String,
});

impl Root {
    /// Validates this value against its schema.
    ///
    /// # Errors
    ///
    /// Every error the schema's validator reports, when this value does not satisfy the schema.
    pub fn validate(&self) -> Result<(), json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.validate_serialized(0, self)
    }
}

impl TryFrom<serde_json::Value> for Root {
    type Error = json_schema_rs::TypeValidationError;

    /// Validates `value` against the schema, then deserializes it.
    fn try_from(value: serde_json::Value) -> Result<Self, json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.deserialize(0, value)
    }
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"name":{"type":"string"}},"required":["name"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["json-schema:///#"],
    ],
);

"##
        .to_owned();
        let actual: String = codegen(Tool::JsonSchemaRs, SCHEMA).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen_non_object_root_is_type_alias() {
        let expected: String = "\
//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

"
        .to_owned();
        let actual: String = codegen(Tool::JsonSchemaRs, r#"{"type":"string"}"#).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen_rejects_unresolvable_ref() {
        let actual: Result<String, AdapterError> =
            codegen(Tool::JsonSchemaRs, r##"{"$ref":"#/$defs/missing"}"##);
        assert!(matches!(actual, Err(AdapterError::Codegen(_))));
    }

    #[test]
    fn jsonschemars_codegen_generates_types_from_a_resource() {
        let expected: String = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Name")]
#[json_schema(id = "https://example.com/name.json")]
pub struct Name {
    pub first: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::deserialize_struct!(Name, additional: BTreeMap<String, serde_json::Value>, {
    "first" => first: String,
});

impl Name {
    /// Validates this value against its schema.
    ///
    /// # Errors
    ///
    /// Every error the schema's validator reports, when this value does not satisfy the schema.
    pub fn validate(&self) -> Result<(), json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.validate_serialized(0, self)
    }
}

impl TryFrom<serde_json::Value> for Name {
    type Error = json_schema_rs::TypeValidationError;

    /// Validates `value` against the schema, then deserializes it.
    fn try_from(value: serde_json::Value) -> Result<Self, json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.deserialize(0, value)
    }
}

#[derive(Debug, Clone, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = "https://example.com/root.json")]
pub struct Root {
    pub name: Name,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::deserialize_struct!(Root, additional: BTreeMap<String, serde_json::Value>, {
    "name" => name: Name,
});

impl Root {
    /// Validates this value against its schema.
    ///
    /// # Errors
    ///
    /// Every error the schema's validator reports, when this value does not satisfy the schema.
    pub fn validate(&self) -> Result<(), json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.validate_serialized(1, self)
    }
}

impl TryFrom<serde_json::Value> for Root {
    type Error = json_schema_rs::TypeValidationError;

    /// Validates `value` against the schema, then deserializes it.
    fn try_from(value: serde_json::Value) -> Result<Self, json_schema_rs::TypeValidationError> {
        TYPE_VALIDATORS.deserialize(1, value)
    }
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$id":"https://example.com/root.json","properties":{"name":{"$ref":"name.json"}},"required":["name"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
        json_schema_rs::EmbeddedDocument {
            uri: "https://example.com/name.json",
            json: r#"{"$id":"https://example.com/name.json","properties":{"first":{"type":"string"}},"required":["first"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Loaded,
        },
    ],
    &[
        &["https://example.com/name.json#"],
        &["https://example.com/root.json#"],
    ],
);

"##
        .to_owned();
        let actual: String = Tool::JsonSchemaRs
            .codegen(&fixture_with_resource().0)
            .expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschemars_codegen_without_its_resource_fails() {
        let actual: Result<String, AdapterError> = codegen(Tool::JsonSchemaRs, SCHEMA_WITH_REF);
        assert!(matches!(actual, Err(AdapterError::Codegen(_))));
    }

    #[test]
    fn jsonschemars_compile_without_its_resource_fails() {
        let actual: Result<Compiled, AdapterError> =
            compile_error(Tool::JsonSchemaRs, SCHEMA_WITH_REF);
        assert!(matches!(actual, Err(AdapterError::Compile(_))));
    }

    #[test]
    fn typify_codegen_rejects_malformed_schema() {
        let actual: Result<String, AdapterError> = codegen(Tool::Typify, "[]");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn schemafy_codegen_rejects_malformed_schema() {
        let actual: Result<String, AdapterError> = codegen(Tool::Schemafy, "[]");
        assert!(matches!(actual, Err(AdapterError::SchemaParse(_))));
    }

    #[test]
    fn schemafy_codegen_names_the_root_type() {
        let expected: String = "\
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
pub struct Root {
    pub name: String,
}
"
        .to_owned();
        let actual: String = codegen(Tool::Schemafy, SCHEMA).expect("codegen");
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_code_generator_reads_the_fixture_schema_file() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            Tool::JsonschemaCodeGenerator,
            &good_fixture().0,
            Operation::Codegen,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonschema_code_generator_failure_names_the_fixture_not_its_directory() {
        let disk: DiskFixture = fixture("{", None, None);
        let expected: Result<(), String> = Err(
            "panic: Could not parse test/inline/schema.json: EOF while parsing an object at line 1 column 1"
                .to_owned(),
        );
        let actual: Result<(), String> =
            check(Tool::JsonschemaCodeGenerator, &disk.0, Operation::Codegen);
        assert_eq!(expected, actual);
    }

    #[test]
    fn codegen_unsupported_for_boon() {
        let expected: AdapterError = AdapterError::Unsupported {
            tool: "boon",
            operation: Operation::Codegen,
        };
        let actual: AdapterError = codegen(Tool::Boon, SCHEMA).expect_err("unsupported");
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_every_supported_cell() {
        let expected: Vec<(&str, Result<(), String>)> = every_cell_passes_except(&[], "");
        let actual: Vec<(&str, Result<(), String>)> = check_every_supported_cell(&good_fixture().0);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_skips_a_fixture_with_unresolved_refs() {
        let offline: DiskFixture = fixture(
            r#"{"properties":{"a":{"$ref":"https://example.com/a.json"}}}"#,
            Some("{}"),
            Some("{}"),
        );
        let expected: Result<(), String> = Err(
            "not measured: schema needs external documents the fixture does not vendor (offline run)"
                .to_owned(),
        );
        let actual: Result<(), String> =
            check(Tool::Jsonschema, &offline.0, Operation::SchemaCompile);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_every_cell_through_resources_but_tools_without_extra_documents() {
        let expected: Vec<(&str, Result<(), String>)> = every_cell_passes_except(
            &[
                "jsonschema-valid",
                "typify",
                "schemafy",
                "jsonschema_code_generator",
            ],
            "cannot load the fixture's resource documents",
        );
        let actual: Vec<(&str, Result<(), String>)> =
            check_every_supported_cell(&fixture_with_resource().0);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_committed_package_json_but_tools_without_extra_documents() {
        let committed: PathBuf = default_roots().into_iter().next().expect("committed root");
        let package_json: Fixture = discover(&[committed])
            .expect("discover")
            .into_iter()
            .find(|fixture: &Fixture| fixture.id == "medium/package-json")
            .expect("package-json fixture");
        let expected: Vec<(&str, Result<(), String>)> = every_cell_passes_except(
            &[
                "jsonschema-valid",
                "typify",
                "schemafy",
                "jsonschema_code_generator",
            ],
            "cannot load the fixture's resource documents",
        );
        let actual: Vec<(&str, Result<(), String>)> = check_every_supported_cell(&package_json);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_rejected_valid_instance() {
        let expected: Result<(), String> = Err("wrong verdict: valid instance rejected".to_owned());
        let actual: Result<(), String> =
            check(Tool::Boon, &swapped_fixture().0, Operation::ValidateValid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_accepted_invalid_instance() {
        let accepts_both: DiskFixture =
            fixture(SCHEMA, Some(r#"{"name":"a"}"#), Some(r#"{"name":"b"}"#));
        let expected: Result<(), String> =
            Err("wrong verdict: invalid instance accepted".to_owned());
        let actual: Result<(), String> = check(
            Tool::Jsonschema,
            &accepts_both.0,
            Operation::ValidateInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_skips_validate_invalid_when_valid_instance_is_rejected() {
        let rejects_both: DiskFixture =
            fixture(SCHEMA, Some(r#"{"name":1}"#), Some(r#"{"name":2}"#));
        let expected: Result<(), String> =
            Err("not measured: valid instance was rejected".to_owned());
        let actual: Result<(), String> =
            check(Tool::Boon, &rejects_both.0, Operation::ValidateInvalid);
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_passes_validate_invalid_without_a_valid_instance() {
        let invalid_only: DiskFixture = fixture(SCHEMA, None, Some(r#"{"name":1}"#));
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            Tool::JsonSchemaRs,
            &invalid_only.0,
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
            &fixture(SCHEMA, None, None).0,
            Operation::ValidateInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn check_reports_unsupported() {
        let expected: Result<(), String> = Err("typify does not support validate_valid".to_owned());
        let actual: Result<(), String> =
            check(Tool::Typify, &good_fixture().0, Operation::ValidateValid);
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
}
