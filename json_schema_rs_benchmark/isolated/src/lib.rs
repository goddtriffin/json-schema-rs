//! In-process adapters over the benchmark competitors that are built apart from the main
//! benchmark crate (see `Cargo.toml` for why each is), measured by the same generic harness
//! ([`json_schema_rs_benchmark::harness`]) into the same raw captures.
//!
//! * `corvus-json-schema` (feature `corvus`): `corvus_json_schema::compile_with` to compile, its
//!   resources served by a `resolve_document` callback; validation through
//!   `Validator::evaluate` with a `Detailed` results collector (every failure, with its message:
//!   the full error report, as for every other validator).
//! * jsonschema's compile-time validator (feature `jsonschema-macros`): the
//!   `#[jsonschema::validator]` struct build.rs generated for the fixture, validating through its
//!   `iter_errors` collected into a `Vec`, as the runtime `jsonschema` adapter does. Its schema is
//!   compiled when the benchmark is built, so it has no `schema_compile` cell; "compiling" it at
//!   run time only looks the fixture's validator up.
//! * reverse codegen (feature `reverse-codegen`): our `ToJsonSchema` derive
//!   (`T::json_schema()`) and schemars' (`schemars::schema_for!(T)`, default settings), each on
//!   its own copy of the fixture's model (see `models/gen`). The timed call produces the tool's
//!   in-memory schema; the correctness probe serializes it to JSON and checks it with our
//!   validator against every valid instance of the fixture.
//! * generated types (feature `generated-types`): the fixture's instances deserialized into our
//!   codegen's types (the same copy of the models our derive runs on, and a copy generated with
//!   `constrained_newtypes(false)`), through plain serde and the root type's
//!   `TryFrom<serde_json::Value>` (see `json_schema_rs_benchmark::harness::GeneratedTypes`).

#[cfg(any(
    all(feature = "corvus", feature = "jsonschema-macros"),
    all(
        feature = "corvus",
        any(feature = "reverse-codegen", feature = "generated-types")
    ),
    all(
        feature = "jsonschema-macros",
        any(feature = "reverse-codegen", feature = "generated-types")
    ),
))]
compile_error!(
    "enable one of `corvus`, `jsonschema-macros` and the model features (`reverse-codegen`, \
     `generated-types`, or both) per build"
);
#[cfg(not(any(
    feature = "corvus",
    feature = "jsonschema-macros",
    feature = "reverse-codegen",
    feature = "generated-types"
)))]
compile_error!(
    "enable one of the `corvus`, `jsonschema-macros`, `reverse-codegen` and `generated-types` features"
);

use json_schema_rs_benchmark::fixtures::Fixture;
use json_schema_rs_benchmark::harness::{Adapter, AdapterError, Operation, unsupported};
#[cfg(feature = "generated-types")]
use json_schema_rs_benchmark::harness::{GeneratedTypes, TypesVariant};
use serde_json::Value;
#[cfg(not(feature = "reverse-codegen"))]
use std::convert::Infallible;
use std::fmt;

/// A compile-time validator generated for one fixture, or why it has none.
#[derive(Debug, Clone, Copy)]
pub enum MacroValidator {
    /// The fixture's validator: `true` when the instance is valid.
    Built(fn(&Value) -> bool),
    /// jsonschema's runtime validator rejects the schema (the macro would fail the build), with
    /// the rejection.
    Rejected(&'static str),
}

include!(concat!(env!("OUT_DIR"), "/macro_validators.rs"));

// The tables below hold references to the shards' statics, never copies of their values. A
// static initialized from another crate's static by value gets its own copy of the data, and every
// function pointer in it is a generic instance (each model's entry points) that this crate then
// compiles again: one copy in this library and one in each binary, built one after another with
// one codegen unit each. With copies, the generated-types build took 17 min per side; with
// references, 5 min, the shards' parallel builds plus linking.

/// One shard's table of our derive's models (fixture id to model).
#[cfg(feature = "reverse-codegen")]
type OurModels = &'static [(&'static str, models_shard_0::OurModel)];

/// One shard's table of schemars' models (fixture id to model).
#[cfg(feature = "reverse-codegen")]
type SchemarsModels = &'static [(&'static str, models_shard_0::SchemarsModel)];

/// Every shard's table of our derive's models.
#[cfg(feature = "reverse-codegen")]
static OUR_MODELS: [&OurModels; 8] = [
    &models_shard_0::JSONSCHEMARS,
    &models_shard_1::JSONSCHEMARS,
    &models_shard_2::JSONSCHEMARS,
    &models_shard_3::JSONSCHEMARS,
    &models_shard_4::JSONSCHEMARS,
    &models_shard_5::JSONSCHEMARS,
    &models_shard_6::JSONSCHEMARS,
    &models_shard_7::JSONSCHEMARS,
];

/// Every shard's table of schemars' models.
#[cfg(feature = "reverse-codegen")]
static SCHEMARS_MODELS: [&SchemarsModels; 8] = [
    &models_shard_0::SCHEMARS,
    &models_shard_1::SCHEMARS,
    &models_shard_2::SCHEMARS,
    &models_shard_3::SCHEMARS,
    &models_shard_4::SCHEMARS,
    &models_shard_5::SCHEMARS,
    &models_shard_6::SCHEMARS,
    &models_shard_7::SCHEMARS,
];

/// One shard's table of generated types (fixture id to types).
#[cfg(feature = "generated-types")]
type TypesTable = &'static [(&'static str, models_shard_0::Types)];

/// Every shard's table of the default settings' generated types.
#[cfg(feature = "generated-types")]
static TYPES: [&TypesTable; 8] = [
    &models_shard_0::TYPES,
    &models_shard_1::TYPES,
    &models_shard_2::TYPES,
    &models_shard_3::TYPES,
    &models_shard_4::TYPES,
    &models_shard_5::TYPES,
    &models_shard_6::TYPES,
    &models_shard_7::TYPES,
];

/// Every shard's table of the no-newtypes generated types.
#[cfg(feature = "generated-types")]
static TYPES_NO_NEWTYPES: [&TypesTable; 8] = [
    &models_shard_0::TYPES_NO_NEWTYPES,
    &models_shard_1::TYPES_NO_NEWTYPES,
    &models_shard_2::TYPES_NO_NEWTYPES,
    &models_shard_3::TYPES_NO_NEWTYPES,
    &models_shard_4::TYPES_NO_NEWTYPES,
    &models_shard_5::TYPES_NO_NEWTYPES,
    &models_shard_6::TYPES_NO_NEWTYPES,
    &models_shard_7::TYPES_NO_NEWTYPES,
];

/// A benchmarked tool built in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolatedTool {
    /// The `corvus-json-schema` crate.
    #[cfg(feature = "corvus")]
    Corvus,
    /// The `jsonschema` crate's compile-time validator (`macros` feature).
    #[cfg(feature = "jsonschema-macros")]
    JsonschemaMacros,
    /// Our library: reverse codegen (the `ToJsonSchema` derive) and the generated types.
    #[cfg(any(feature = "reverse-codegen", feature = "generated-types"))]
    JsonSchemaRs,
    /// The `schemars` crate's `JsonSchema` derive.
    #[cfg(feature = "reverse-codegen")]
    Schemars,
}

impl fmt::Display for IsolatedTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A schema compiled by one of this crate's tools.
pub enum IsolatedCompiled {
    /// `corvus-json-schema` validator.
    #[cfg(feature = "corvus")]
    Corvus(corvus_json_schema::Validator),
    /// The fixture's compile-time validator.
    #[cfg(feature = "jsonschema-macros")]
    JsonschemaMacros(fn(&Value) -> bool),
}

/// A fixture's model, as one of this crate's reverse-codegen tools takes it.
#[cfg(feature = "reverse-codegen")]
#[derive(Debug, Clone, Copy)]
pub enum IsolatedModel {
    /// The root type's `ToJsonSchema::json_schema`.
    JsonSchemaRs(fn() -> json_schema_rs::ReverseCodeGenResult),
    /// `schemars::schema_for!` of the root type.
    Schemars(fn() -> schemars::Schema),
}

/// A schema produced by one of this crate's reverse-codegen tools.
#[cfg(feature = "reverse-codegen")]
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "held unboxed: boxing would add an allocation to every timed reverse codegen call"
)]
pub enum IsolatedReversed {
    /// Our schema model, or every problem the derive reported.
    JsonSchemaRs(json_schema_rs::ReverseCodeGenResult),
    /// schemars' schema.
    Schemars(schemars::Schema),
}

#[cfg(feature = "reverse-codegen")]
impl IsolatedReversed {
    /// The schema as a JSON document.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::ReverseCodegen`] when our derive reported errors instead of a
    /// schema, or the schema does not serialize.
    pub fn to_json(&self) -> Result<String, AdapterError> {
        let serialize_error = |e: serde_json::Error| AdapterError::ReverseCodegen(e.to_string());
        match self {
            IsolatedReversed::JsonSchemaRs(Ok(schema)) => {
                serde_json::to_string(schema).map_err(serialize_error)
            }
            IsolatedReversed::JsonSchemaRs(Err(errors)) => Err(AdapterError::ReverseCodegen(
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<String>>()
                    .join("; "),
            )),
            IsolatedReversed::Schemars(schema) => {
                serde_json::to_string(schema).map_err(serialize_error)
            }
        }
    }
}

impl Adapter for IsolatedTool {
    type Compiled = IsolatedCompiled;
    #[cfg(feature = "reverse-codegen")]
    type Model = IsolatedModel;
    #[cfg(not(feature = "reverse-codegen"))]
    type Model = Infallible;
    #[cfg(feature = "reverse-codegen")]
    type Reversed = IsolatedReversed;
    #[cfg(not(feature = "reverse-codegen"))]
    type Reversed = Infallible;

    const ALL: &'static [IsolatedTool] = &[
        #[cfg(feature = "corvus")]
        IsolatedTool::Corvus,
        #[cfg(feature = "jsonschema-macros")]
        IsolatedTool::JsonschemaMacros,
        #[cfg(any(feature = "reverse-codegen", feature = "generated-types"))]
        IsolatedTool::JsonSchemaRs,
        #[cfg(feature = "reverse-codegen")]
        IsolatedTool::Schemars,
    ];

    fn id(self) -> &'static str {
        match self {
            #[cfg(feature = "corvus")]
            IsolatedTool::Corvus => "corvus-json-schema",
            #[cfg(feature = "jsonschema-macros")]
            IsolatedTool::JsonschemaMacros => "jsonschema-macros",
            #[cfg(any(feature = "reverse-codegen", feature = "generated-types"))]
            IsolatedTool::JsonSchemaRs => "jsonschemars",
            #[cfg(feature = "reverse-codegen")]
            IsolatedTool::Schemars => "schemars",
        }
    }

    fn supports(self, operation: Operation) -> bool {
        match self {
            #[cfg(feature = "corvus")]
            IsolatedTool::Corvus => {
                !matches!(operation, Operation::Codegen | Operation::ReverseCodegen)
            }
            #[cfg(feature = "jsonschema-macros")]
            IsolatedTool::JsonschemaMacros => matches!(
                operation,
                Operation::ValidateValid | Operation::ValidateInvalid
            ),
            #[cfg(any(feature = "reverse-codegen", feature = "generated-types"))]
            IsolatedTool::JsonSchemaRs => {
                (cfg!(feature = "reverse-codegen") && operation == Operation::ReverseCodegen)
                    || (cfg!(feature = "generated-types") && operation.generated_types().is_some())
            }
            #[cfg(feature = "reverse-codegen")]
            IsolatedTool::Schemars => operation == Operation::ReverseCodegen,
        }
    }

    fn codegen(self, _fixture: &Fixture) -> Result<String, AdapterError> {
        Err(unsupported(self, Operation::Codegen))
    }

    fn compile(self, fixture: &Fixture) -> Result<IsolatedCompiled, AdapterError> {
        match self {
            #[cfg(feature = "corvus")]
            IsolatedTool::Corvus => corvus::compile(fixture).map(IsolatedCompiled::Corvus),
            #[cfg(feature = "jsonschema-macros")]
            IsolatedTool::JsonschemaMacros => macro_validator(fixture),
            #[cfg(any(feature = "reverse-codegen", feature = "generated-types"))]
            IsolatedTool::JsonSchemaRs => {
                // The model tools compile no schema.
                let _ = fixture;
                Err(unsupported(self, Operation::SchemaCompile))
            }
            #[cfg(feature = "reverse-codegen")]
            IsolatedTool::Schemars => Err(unsupported(self, Operation::SchemaCompile)),
        }
    }

    #[cfg(any(feature = "corvus", feature = "jsonschema-macros"))]
    fn validate(compiled: &IsolatedCompiled, instance: &Value) -> bool {
        match compiled {
            #[cfg(feature = "corvus")]
            IsolatedCompiled::Corvus(validator) => corvus::validate(validator, instance),
            #[cfg(feature = "jsonschema-macros")]
            IsolatedCompiled::JsonschemaMacros(validate) => validate(instance),
        }
    }

    #[cfg(not(any(feature = "corvus", feature = "jsonschema-macros")))]
    fn validate(compiled: &IsolatedCompiled, _instance: &Value) -> bool {
        match *compiled {}
    }

    #[cfg(feature = "reverse-codegen")]
    fn model(self, fixture: &Fixture) -> Result<IsolatedModel, AdapterError> {
        reverse::model(self, &fixture.id)
    }

    #[cfg(feature = "reverse-codegen")]
    fn reverse_codegen(model: IsolatedModel) -> IsolatedReversed {
        match model {
            IsolatedModel::JsonSchemaRs(json_schema) => {
                IsolatedReversed::JsonSchemaRs(json_schema())
            }
            IsolatedModel::Schemars(schema_for) => IsolatedReversed::Schemars(schema_for()),
        }
    }

    #[cfg(not(feature = "reverse-codegen"))]
    fn reverse_codegen(model: Infallible) -> Infallible {
        match model {}
    }

    #[cfg(feature = "reverse-codegen")]
    fn check_reversed(fixture: &Fixture, reversed: &IsolatedReversed) -> Result<(), AdapterError> {
        reverse::check(fixture, reversed)
    }

    #[cfg(not(feature = "reverse-codegen"))]
    fn check_reversed(_fixture: &Fixture, reversed: &Infallible) -> Result<(), AdapterError> {
        match *reversed {}
    }

    #[cfg(feature = "generated-types")]
    fn generated_types(
        self,
        fixture: &Fixture,
        variant: TypesVariant,
    ) -> Result<GeneratedTypes, AdapterError> {
        if !matches!(self, IsolatedTool::JsonSchemaRs) {
            return Err(unsupported(self, Operation::DeserializeValid));
        }
        let tables: &[&TypesTable; 8] = match variant {
            TypesVariant::Default => &TYPES,
            TypesVariant::NoNewtypes => &TYPES_NO_NEWTYPES,
        };
        let entry: models_shard_0::Types = tables
            .iter()
            .flat_map(|table: &&TypesTable| table.iter())
            .find(|(id, _types)| *id == fixture.id)
            .map(|(_id, types)| *types)
            .ok_or_else(|| {
                AdapterError::GeneratedTypes(
                    "fixture was not discovered when the benchmark was built".to_owned(),
                )
            })?;
        entry.map_err(|reason: &str| AdapterError::GeneratedTypes(reason.to_owned()))
    }
}

/// The compile-time validator build.rs generated for `fixture`.
#[cfg(feature = "jsonschema-macros")]
fn macro_validator(fixture: &Fixture) -> Result<IsolatedCompiled, AdapterError> {
    match MACRO_VALIDATORS
        .iter()
        .find(|(id, _validator): &&(&str, MacroValidator)| *id == fixture.id)
    {
        Some((_id, MacroValidator::Built(validate))) => {
            Ok(IsolatedCompiled::JsonschemaMacros(*validate))
        }
        Some((_id, MacroValidator::Rejected(reason))) => {
            Err(AdapterError::Compile((*reason).to_owned()))
        }
        None => Err(AdapterError::Compile(
            "fixture was not discovered when the benchmark was built".to_owned(),
        )),
    }
}

#[cfg(feature = "corvus")]
mod corvus {
    use json_schema_rs_benchmark::fixtures::Fixture;
    use json_schema_rs_benchmark::harness::{AdapterError, parse_schema_value};
    use serde_json::Value;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Compile the fixture's schema, its resources served by URI.
    pub(crate) fn compile(
        fixture: &Fixture,
    ) -> Result<corvus_json_schema::Validator, AdapterError> {
        let schema: Value = parse_schema_value(&fixture.schema_text)?;
        let mut options: corvus_json_schema::CompileOptions =
            corvus_json_schema::CompileOptions::default();
        if !fixture.resources.is_empty() {
            let mut documents: HashMap<String, Value> = HashMap::new();
            for resource in &fixture.resources {
                documents.insert(resource.uri.clone(), parse_schema_value(&resource.text)?);
            }
            options.resolve_document = Some(Arc::new(move |uri: &str| documents.get(uri).cloned()));
        }
        corvus_json_schema::compile_with(&schema, &options).map_err(
            |e: corvus_json_schema::SchemaCompilationError| AdapterError::Compile(e.to_string()),
        )
    }

    /// Evaluate `instance` exhaustively, collecting every failure with its message.
    pub(crate) fn validate(validator: &corvus_json_schema::Validator, instance: &Value) -> bool {
        let mut collector: corvus_json_schema::JsonSchemaResultsCollector =
            corvus_json_schema::JsonSchemaResultsCollector::new(
                corvus_json_schema::ResultsLevel::Detailed,
            );
        validator
            .evaluate(instance, &mut collector)
            .unwrap_or(false)
    }
}

#[cfg(feature = "reverse-codegen")]
pub mod reverse {
    //! Model lookup and the correctness probe of the reverse-codegen tools.

    use super::{IsolatedModel, IsolatedReversed, IsolatedTool, OUR_MODELS, SCHEMARS_MODELS};
    use json_schema_rs::{CompiledSchema, Registry, Schema};
    use json_schema_rs_benchmark::fixtures::{self, Fixture};
    use json_schema_rs_benchmark::harness::AdapterError;
    use serde_json::Value;

    /// `tool`'s model of the fixture `fixture_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::ReverseCodegen`] with why the tool has no model for the fixture.
    pub fn model(tool: IsolatedTool, fixture_id: &str) -> Result<IsolatedModel, AdapterError> {
        /// The entry for `fixture_id` in any shard's table.
        fn find<T: Copy>(
            shards: &[&'static &'static [(&'static str, Result<T, &'static str>)]],
            fixture_id: &str,
        ) -> Result<T, AdapterError> {
            let entry: Result<T, &'static str> = shards
                .iter()
                .flat_map(|shard| shard.iter())
                .find(|(id, _model)| *id == fixture_id)
                .map(|(_id, model)| *model)
                .ok_or_else(|| {
                    AdapterError::ReverseCodegen(
                        "fixture was not discovered when the benchmark was built".to_owned(),
                    )
                })?;
            entry.map_err(|reason: &str| AdapterError::ReverseCodegen(reason.to_owned()))
        }
        match tool {
            IsolatedTool::JsonSchemaRs => {
                find(&OUR_MODELS, fixture_id).map(IsolatedModel::JsonSchemaRs)
            }
            IsolatedTool::Schemars => {
                find(&SCHEMARS_MODELS, fixture_id).map(IsolatedModel::Schemars)
            }
        }
    }

    /// The probe: `reversed`, as JSON, is a schema our validator compiles, and it accepts every
    /// valid instance of `fixture`.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::ReverseCodegen`] naming the first problem.
    pub fn check(fixture: &Fixture, reversed: &IsolatedReversed) -> Result<(), AdapterError> {
        let wrong = |message: String| AdapterError::ReverseCodegen(message);
        let text: String = reversed.to_json()?;
        let schema: Schema = serde_json::from_str(&text)
            .map_err(|e: serde_json::Error| wrong(format!("emitted schema does not parse: {e}")))?;
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .map_err(|e| wrong(format!("emitted schema does not compile: {e}")))?;
        let compiled: CompiledSchema = CompiledSchema::new(&registry)
            .map_err(|e| wrong(format!("emitted schema does not compile: {e}")))?;
        let instances: Vec<(String, String)> = fixtures::valid_instances(fixture)
            .map_err(|e| wrong(format!("cannot read the valid instances: {e}")))?;
        for (name, instance_text) in instances {
            let instance: Value = serde_json::from_str(&instance_text)
                .map_err(|e: serde_json::Error| AdapterError::InstanceParse(e.to_string()))?;
            if compiled.validate(&instance).is_err() {
                return Err(wrong(format!(
                    "emitted schema rejects valid instance {name}"
                )));
            }
        }
        Ok(())
    }
}
