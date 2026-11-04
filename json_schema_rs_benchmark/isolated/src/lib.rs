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

#[cfg(all(feature = "corvus", feature = "jsonschema-macros"))]
compile_error!("enable exactly one of the `corvus` and `jsonschema-macros` features per build");
#[cfg(not(any(feature = "corvus", feature = "jsonschema-macros")))]
compile_error!("enable one of the `corvus` and `jsonschema-macros` features");

use json_schema_rs_benchmark::fixtures::Fixture;
use json_schema_rs_benchmark::harness::{Adapter, AdapterError, Operation, unsupported};
use serde_json::Value;
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

/// A benchmarked tool built in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsolatedTool {
    /// The `corvus-json-schema` crate.
    #[cfg(feature = "corvus")]
    Corvus,
    /// The `jsonschema` crate's compile-time validator (`macros` feature).
    #[cfg(feature = "jsonschema-macros")]
    JsonschemaMacros,
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

impl Adapter for IsolatedTool {
    type Compiled = IsolatedCompiled;

    const ALL: &'static [IsolatedTool] = &[
        #[cfg(feature = "corvus")]
        IsolatedTool::Corvus,
        #[cfg(feature = "jsonschema-macros")]
        IsolatedTool::JsonschemaMacros,
    ];

    fn id(self) -> &'static str {
        match self {
            #[cfg(feature = "corvus")]
            IsolatedTool::Corvus => "corvus-json-schema",
            #[cfg(feature = "jsonschema-macros")]
            IsolatedTool::JsonschemaMacros => "jsonschema-macros",
        }
    }

    fn supports(self, operation: Operation) -> bool {
        match self {
            #[cfg(feature = "corvus")]
            IsolatedTool::Corvus => operation != Operation::Codegen,
            #[cfg(feature = "jsonschema-macros")]
            IsolatedTool::JsonschemaMacros => matches!(
                operation,
                Operation::ValidateValid | Operation::ValidateInvalid
            ),
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
        }
    }

    fn validate(compiled: &IsolatedCompiled, instance: &Value) -> bool {
        match compiled {
            #[cfg(feature = "corvus")]
            IsolatedCompiled::Corvus(validator) => corvus::validate(validator, instance),
            #[cfg(feature = "jsonschema-macros")]
            IsolatedCompiled::JsonschemaMacros(validate) => validate(instance),
        }
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
