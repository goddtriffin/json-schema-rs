//! The in-process tier's tool-independent machinery, shared by every crate that links tools.
//!
//! Each tool sits behind an [`Adapter`]: the operations it implements and how it runs each one
//! on a [`Fixture`]. On top of that trait this module decides whether a cell is correct
//! ([`check`]), profiles every cell's peak heap ([`heap_profile_main`]) and times every cell with
//! criterion ([`cross_tool_main`]). The main benchmark crate implements [`Adapter`] for the tools
//! it links ([`crate::adapters::Tool`]); `isolated/` implements it for the tools that must be
//! built apart from them (see design.md, *Benchmark harness*), and both write the same raw
//! captures, which the aggregator merges.

use crate::fixtures::{self, Fixture};
use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, BenchmarkId, Criterion, SamplingMode};
use peak_alloc::PeakAlloc;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::any::Any;
use std::fmt::{self, Write as _};
use std::fs::{self, OpenOptions};
use std::hint::black_box;
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// A benchmarked operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operation {
    /// Schema JSON text -> Rust source text.
    Codegen,
    /// Rust types (a fixture's model, compiled into the build) -> the tool's in-memory JSON
    /// Schema.
    ReverseCodegen,
    /// Schema JSON text -> compiled, ready-to-validate schema.
    SchemaCompile,
    /// Compiled schema + parsed instance -> verdict, on the fixture's valid instance.
    ValidateValid,
    /// Compiled schema + parsed instance -> verdict, on the fixture's invalid instance.
    ValidateInvalid,
    /// The fixture's valid instance text -> its generated root type, through plain serde
    /// (`serde_json::from_str`); types generated with the default settings.
    DeserializeValid,
    /// Parsed valid instance -> the generated root type's `TryFrom<serde_json::Value>`; default
    /// settings.
    TryFromValid,
    /// Parsed invalid instance -> the generated root type's `TryFrom<serde_json::Value>`, which
    /// must reject it; default settings.
    TryFromInvalid,
    /// [`Operation::DeserializeValid`] on types generated with `constrained_newtypes(false)`.
    DeserializeValidNoNewtypes,
    /// [`Operation::TryFromValid`] on types generated with `constrained_newtypes(false)`.
    TryFromValidNoNewtypes,
    /// [`Operation::TryFromInvalid`] on types generated with `constrained_newtypes(false)`.
    TryFromInvalidNoNewtypes,
}

/// Which copy of a fixture's generated types an operation runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypesVariant {
    /// Our codegen's default settings (constrained newtypes on).
    Default,
    /// Our codegen's default settings with `constrained_newtypes(false)`.
    NoNewtypes,
}

/// The entry point of a generated-types operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypesCall {
    /// `serde_json::from_str::<Root>` of the instance text.
    Deserialize,
    /// `Root::try_from(serde_json::Value)`.
    TryFrom,
}

impl Operation {
    /// Every operation, in report order.
    pub const ALL: [Operation; 11] = [
        Operation::Codegen,
        Operation::ReverseCodegen,
        Operation::SchemaCompile,
        Operation::ValidateValid,
        Operation::ValidateInvalid,
        Operation::DeserializeValid,
        Operation::TryFromValid,
        Operation::TryFromInvalid,
        Operation::DeserializeValidNoNewtypes,
        Operation::TryFromValidNoNewtypes,
        Operation::TryFromInvalidNoNewtypes,
    ];

    /// Stable id token used as the criterion group name and in raw capture files.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Operation::Codegen => "codegen",
            Operation::ReverseCodegen => "reverse_codegen",
            Operation::SchemaCompile => "schema_compile",
            Operation::ValidateValid => "validate_valid",
            Operation::ValidateInvalid => "validate_invalid",
            Operation::DeserializeValid => "deserialize_valid",
            Operation::TryFromValid => "try_from_valid",
            Operation::TryFromInvalid => "try_from_invalid",
            Operation::DeserializeValidNoNewtypes => "deserialize_valid_no_newtypes",
            Operation::TryFromValidNoNewtypes => "try_from_valid_no_newtypes",
            Operation::TryFromInvalidNoNewtypes => "try_from_invalid_no_newtypes",
        }
    }

    /// The instance text an operation runs on (`None` for an operation without an instance, or
    /// when the fixture lacks that instance).
    #[must_use]
    pub fn instance(self, fixture: &Fixture) -> Option<&str> {
        match self {
            Operation::Codegen | Operation::ReverseCodegen | Operation::SchemaCompile => None,
            Operation::ValidateValid
            | Operation::DeserializeValid
            | Operation::TryFromValid
            | Operation::DeserializeValidNoNewtypes
            | Operation::TryFromValidNoNewtypes => fixture.valid_instance.as_deref(),
            Operation::ValidateInvalid
            | Operation::TryFromInvalid
            | Operation::TryFromInvalidNoNewtypes => fixture.invalid_instance.as_deref(),
        }
    }

    /// Whether the operation's instance is the fixture's valid one.
    #[must_use]
    pub const fn expects_valid(self) -> bool {
        matches!(
            self,
            Operation::ValidateValid
                | Operation::DeserializeValid
                | Operation::TryFromValid
                | Operation::DeserializeValidNoNewtypes
                | Operation::TryFromValidNoNewtypes
        )
    }

    /// For a generated-types operation, the types it runs on and its entry point; `None` for
    /// every other operation.
    #[must_use]
    pub const fn generated_types(self) -> Option<(TypesVariant, TypesCall)> {
        match self {
            Operation::Codegen
            | Operation::ReverseCodegen
            | Operation::SchemaCompile
            | Operation::ValidateValid
            | Operation::ValidateInvalid => None,
            Operation::DeserializeValid => Some((TypesVariant::Default, TypesCall::Deserialize)),
            Operation::TryFromValid | Operation::TryFromInvalid => {
                Some((TypesVariant::Default, TypesCall::TryFrom))
            }
            Operation::DeserializeValidNoNewtypes => {
                Some((TypesVariant::NoNewtypes, TypesCall::Deserialize))
            }
            Operation::TryFromValidNoNewtypes | Operation::TryFromInvalidNoNewtypes => {
                Some((TypesVariant::NoNewtypes, TypesCall::TryFrom))
            }
        }
    }
}

impl fmt::Display for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// A fixture's generated Rust types, compiled into the build: the root type's entry points.
///
/// Each timed function produces the root value (or the error) and drops it, so the timing covers
/// what a caller pays. The `*_detail` functions are the correctness probe's: the same call, with
/// the error as text.
#[derive(Debug, Clone, Copy)]
pub struct GeneratedTypes {
    /// `serde_json::from_str::<Root>(text).is_ok()`.
    pub deserialize: fn(&str) -> bool,
    /// [`GeneratedTypes::deserialize`] with the error as text.
    pub deserialize_detail: fn(&str) -> Result<(), String>,
    /// `Root::try_from(value).is_ok()`, or why the root type has no `TryFrom<serde_json::Value>`
    /// (a type alias, or self-validating types do not cover it).
    pub try_from: Result<fn(Value) -> bool, &'static str>,
    /// [`GeneratedTypes::try_from`] with the error as text.
    pub try_from_detail: Result<TryFromDetail, &'static str>,
}

/// A root type's `TryFrom<serde_json::Value>`, with the error as text.
pub type TryFromDetail = fn(Value) -> Result<(), String>;

impl GeneratedTypes {
    /// The entry points of root type `T`, which has `TryFrom<serde_json::Value>`.
    #[must_use]
    pub const fn with_try_from<T>() -> GeneratedTypes
    where
        T: DeserializeOwned + TryFrom<Value>,
        <T as TryFrom<Value>>::Error: fmt::Display,
    {
        GeneratedTypes {
            deserialize: |text: &str| serde_json::from_str::<T>(text).is_ok(),
            deserialize_detail: deserialize_detail::<T>,
            try_from: Ok(|value: Value| T::try_from(value).is_ok()),
            try_from_detail: Ok(|value: Value| {
                T::try_from(value)
                    .map(|_root: T| ())
                    .map_err(|e: <T as TryFrom<Value>>::Error| e.to_string())
            }),
        }
    }

    /// The entry points of root type `T`, which has no `TryFrom<serde_json::Value>`, for `reason`.
    #[must_use]
    pub const fn without_try_from<T: DeserializeOwned>(reason: &'static str) -> GeneratedTypes {
        GeneratedTypes {
            deserialize: |text: &str| serde_json::from_str::<T>(text).is_ok(),
            deserialize_detail: deserialize_detail::<T>,
            try_from: Err(reason),
            try_from_detail: Err(reason),
        }
    }
}

/// `serde_json::from_str::<T>(text)`, with the error as text.
fn deserialize_detail<T: DeserializeOwned>(text: &str) -> Result<(), String> {
    serde_json::from_str::<T>(text)
        .map(|_root: T| ())
        .map_err(|e: serde_json::Error| e.to_string())
}

/// One benchmarked tool, linked as a library: the operations it implements and how it runs them.
///
/// [`Adapter::codegen`] and [`Adapter::compile`] start from the fixture's schema text (and its
/// resources' texts) held in memory; [`Adapter::validate`] starts from an already-compiled schema
/// and an already-parsed instance; [`Adapter::reverse_codegen`] starts from the fixture's model
/// (Rust types compiled into the build), already looked up.
pub trait Adapter: Copy + fmt::Display + 'static {
    /// A schema compiled by this tool, held unboxed so compile timings include no adapter-added
    /// allocation.
    type Compiled;

    /// A fixture's model, ready for [`Adapter::reverse_codegen`] (`Infallible` for a tool without
    /// reverse codegen).
    type Model: Copy;

    /// The in-memory schema [`Adapter::reverse_codegen`] produces, in the tool's own type
    /// (`Infallible` for a tool without reverse codegen).
    type Reversed;

    /// Every tool of this kind, in report order.
    const ALL: &'static [Self];

    /// Stable id token used in benchmark ids and raw capture files.
    fn id(self) -> &'static str;

    /// Whether this tool implements `operation`.
    fn supports(self, operation: Operation) -> bool;

    /// Generate Rust source from `fixture`'s schema, its resources reachable by their URIs.
    ///
    /// # Errors
    ///
    /// Returns an [`AdapterError`] when the tool has no codegen, cannot take the fixture's
    /// resources, or fails.
    fn codegen(self, fixture: &Fixture) -> Result<String, AdapterError>;

    /// Compile `fixture`'s schema, ready for [`Adapter::validate`], its resources registered
    /// under their URIs.
    ///
    /// # Errors
    ///
    /// Returns an [`AdapterError`] when the tool has no validator, cannot take the fixture's
    /// resources, or rejects the schema.
    fn compile(self, fixture: &Fixture) -> Result<Self::Compiled, AdapterError>;

    /// Validate `instance` against `compiled` through the tool's full error-report API; `true`
    /// when the instance is valid.
    fn validate(compiled: &Self::Compiled, instance: &Value) -> bool;

    /// Look up the model generated for `fixture`, ready for [`Adapter::reverse_codegen`].
    ///
    /// # Errors
    ///
    /// Returns an [`AdapterError`] when the tool has no reverse codegen, or the fixture has no
    /// model this tool can take (its generation or this tool's derive failed).
    fn model(self, _fixture: &Fixture) -> Result<Self::Model, AdapterError> {
        Err(unsupported(self, Operation::ReverseCodegen))
    }

    /// Generate the JSON Schema of `model`'s root type through the tool's own API.
    fn reverse_codegen(model: Self::Model) -> Self::Reversed;

    /// The correctness probe of a reverse codegen result: `reversed`, as a JSON document, parses
    /// as a schema and accepts every valid instance of `fixture`.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::ReverseCodegen`] naming what is wrong.
    fn check_reversed(fixture: &Fixture, reversed: &Self::Reversed) -> Result<(), AdapterError>;

    /// Look up `fixture`'s generated types of `variant`, compiled into the build.
    ///
    /// # Errors
    ///
    /// Returns an [`AdapterError`] when the tool has no generated types, or the fixture has none
    /// (its codegen failed, or its types were left out of the build).
    fn generated_types(
        self,
        _fixture: &Fixture,
        _variant: TypesVariant,
    ) -> Result<GeneratedTypes, AdapterError> {
        Err(unsupported(self, Operation::DeserializeValid))
    }
}

/// Failure of an adapter call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterError {
    /// The tool does not implement the operation.
    Unsupported {
        /// Id of the tool asked to run the operation.
        tool: &'static str,
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
    /// The tool's reverse codegen failed, or produced a schema that fails the correctness probe.
    ReverseCodegen(String),
    /// The fixture has no generated types to run, with why.
    GeneratedTypes(String),
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
            AdapterError::ReverseCodegen(message) => write!(f, "reverse codegen error: {message}"),
            AdapterError::GeneratedTypes(message) => write!(f, "generated types: {message}"),
            AdapterError::ResourcesUnsupported => f.write_str(RESOURCES_UNSUPPORTED),
            AdapterError::Panic(message) => write!(f, "panic: {message}"),
        }
    }
}

impl std::error::Error for AdapterError {}

/// The error for `tool` asked to run an `operation` it does not implement.
#[must_use]
pub fn unsupported<A: Adapter>(tool: A, operation: Operation) -> AdapterError {
    AdapterError::Unsupported {
        tool: tool.id(),
        operation,
    }
}

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

/// Run `f`, converting a panic into [`AdapterError::Panic`], so one tool that panics on a
/// schema never aborts a run.
///
/// # Errors
///
/// Returns `f`'s error, or [`AdapterError::Panic`] when `f` panics.
pub fn guarded<T>(f: impl FnOnce() -> Result<T, AdapterError>) -> Result<T, AdapterError> {
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

/// Parse schema (or resource) text into a JSON value.
///
/// # Errors
///
/// Returns [`AdapterError::SchemaParse`] when `text` is not valid JSON.
pub fn parse_schema_value(text: &str) -> Result<Value, AdapterError> {
    serde_json::from_str(text)
        .map_err(|e: serde_json::Error| AdapterError::SchemaParse(e.to_string()))
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

/// Verify `tool` is correct on `fixture` for `operation`: codegen succeeds with Rust source,
/// reverse codegen produces a schema that accepts every valid instance, the schema compiles, the
/// valid instance is accepted, or the invalid instance is rejected.
///
/// # Errors
///
/// Returns a short one-line reason when the cell is unsupported, the fixture lacks the
/// needed instance, the tool fails or panics, or the verdict is wrong.
pub fn check<A: Adapter>(tool: A, fixture: &Fixture, operation: Operation) -> Result<(), String> {
    check_inner(tool, fixture, operation)
        .map_err(|reason: String| one_line(&machine_independent(&reason, fixture)))
}

/// `reason` with the fixture's directory (which a tool reading files may quote) replaced by the
/// fixture id, so the committed compatibility matrix is the same on every machine.
fn machine_independent(reason: &str, fixture: &Fixture) -> String {
    let dir: String = fixture.dir.display().to_string();
    if dir.is_empty() {
        reason.to_owned()
    } else {
        reason.replace(&dir, &fixture.id)
    }
}

fn check_inner<A: Adapter>(tool: A, fixture: &Fixture, operation: Operation) -> Result<(), String> {
    if !tool.supports(operation) {
        return Err(unsupported(tool, operation).to_string());
    }
    if !fixture.unresolved_refs().is_empty() {
        return Err(OFFLINE_SKIP.to_owned());
    }
    match operation {
        // Generated source that is not a Rust file is not a successful generation.
        Operation::Codegen => guarded(|| tool.codegen(fixture))
            .and_then(|source: String| {
                syn::parse_file(&source)
                    .map(|_file: syn::File| ())
                    .map_err(|e: syn::Error| {
                        AdapterError::Codegen(format!("generated source is not Rust: {e}"))
                    })
            })
            .map_err(|e: AdapterError| e.to_string()),
        // A fast schema that is wrong is not a result: the probe must pass before it is timed.
        Operation::ReverseCodegen => guarded(|| tool.model(fixture))
            .and_then(|model: A::Model| guarded(|| Ok(A::reverse_codegen(model))))
            .and_then(|reversed: A::Reversed| guarded(|| A::check_reversed(fixture, &reversed)))
            .map_err(|e: AdapterError| e.to_string()),
        Operation::SchemaCompile => guarded(|| tool.compile(fixture))
            .map(|_compiled: A::Compiled| ())
            .map_err(|e: AdapterError| e.to_string()),
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let text: &str = operation
                .instance(fixture)
                .ok_or_else(|| AdapterError::MissingInstance(operation).to_string())?;
            let instance: Value = parse_instance(text).map_err(|e: AdapterError| e.to_string())?;
            let compiled: A::Compiled =
                guarded(|| tool.compile(fixture)).map_err(|e: AdapterError| e.to_string())?;
            let expected_valid: bool = operation.expects_valid();
            // Rejecting the invalid instance proves nothing when the tool also rejects the valid
            // one (e.g. it rejects everything), so that cell is not measured. The CLI tier
            // applies the same rule.
            if !expected_valid && let Some(valid_text) = fixture.valid_instance.as_deref() {
                let valid_instance: Value =
                    parse_instance(valid_text).map_err(|e: AdapterError| e.to_string())?;
                let accepted: bool = guarded(|| Ok(A::validate(&compiled, &valid_instance)))
                    .map_err(|e: AdapterError| e.to_string())?;
                if !accepted {
                    return Err(VALID_REJECTED_SKIP.to_owned());
                }
            }
            let valid: bool = guarded(|| Ok(A::validate(&compiled, &instance)))
                .map_err(|e: AdapterError| e.to_string())?;
            if valid == expected_valid {
                Ok(())
            } else if expected_valid {
                Err("wrong verdict: valid instance rejected".to_owned())
            } else {
                Err("wrong verdict: invalid instance accepted".to_owned())
            }
        }
        Operation::DeserializeValid
        | Operation::TryFromValid
        | Operation::TryFromInvalid
        | Operation::DeserializeValidNoNewtypes
        | Operation::TryFromValidNoNewtypes
        | Operation::TryFromInvalidNoNewtypes => check_generated_types(tool, fixture, operation),
    }
}

/// The probe of a generated-types operation: the call accepts the valid instance, and (for a
/// `try_from_invalid` operation) rejects the invalid one. Rejecting the invalid instance proves
/// nothing when the same types also reject the valid one, so that cell is not measured, as for
/// validation.
fn check_generated_types<A: Adapter>(
    tool: A,
    fixture: &Fixture,
    operation: Operation,
) -> Result<(), String> {
    let Some((variant, call)) = operation.generated_types() else {
        return Err(unsupported(tool, operation).to_string());
    };
    let text: &str = operation
        .instance(fixture)
        .ok_or_else(|| AdapterError::MissingInstance(operation).to_string())?;
    let types: GeneratedTypes = guarded(|| tool.generated_types(fixture, variant))
        .map_err(|e: AdapterError| e.to_string())?;
    let call_once = |text: &str| -> Result<Result<(), String>, AdapterError> {
        match call {
            TypesCall::Deserialize => guarded(|| Ok((types.deserialize_detail)(text))),
            TypesCall::TryFrom => {
                let try_from: TryFromDetail = types
                    .try_from_detail
                    .map_err(|reason: &str| AdapterError::GeneratedTypes(reason.to_owned()))?;
                let instance: Value = parse_instance(text)?;
                guarded(|| Ok(try_from(instance)))
            }
        }
    };
    let run = |text: &str| -> Result<Result<(), String>, String> {
        call_once(text).map_err(|e: AdapterError| e.to_string())
    };
    if operation.expects_valid() {
        return run(text)?
            .map_err(|error: String| format!("wrong verdict: valid instance rejected: {error}"));
    }
    if let Some(valid_text) = fixture.valid_instance.as_deref()
        && run(valid_text)?.is_err()
    {
        return Err(VALID_REJECTED_SKIP.to_owned());
    }
    match run(text)? {
        Ok(()) => Err("wrong verdict: invalid instance accepted".to_owned()),
        Err(_error) => Ok(()),
    }
}

/// Collapse whitespace runs (tabs, newlines) to single spaces so a reason fits a TSV cell.
#[must_use]
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

// --- Heap profile ------------------------------------------------------------------------

/// Entry point of a heap-profile binary: `<bin> --output <RAW_DIR>` profiles every
/// (fixture, operation, tool) cell of `A` and appends two tab-separated files (no header):
///
/// * `RAW_DIR/inprocess_compat.tsv` — `tool\tfixture\toperation\tstatus\tdetail`, one row per
///   supported cell; status `pass` | `fail`, detail a one-line reason (empty on pass).
/// * `RAW_DIR/inprocess_heap.tsv` — `tool\tfixture\toperation\tpeak_bytes` for every passing
///   cell: the peak heap the operation allocated above the heap in use when it started.
///
/// The files are appended to, never truncated, so every crate that links tools adds its rows to
/// one capture; the driver resets `RAW_DIR` before the first. Each measured cell runs once as
/// warmup (populating lazy statics and caches), then once measured. Inputs (compiled schema,
/// parsed instance) are prepared before the measurement starts, so only the operation itself is
/// counted. `peak` must be the binary's global allocator (`PeakAlloc` counts in statics, so
/// any value of it reads the same counters).
///
/// Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
/// and clap's derive macro emits internal `#[allow(...)]` attributes.
///
/// # Panics
///
/// Never: clap exits with a usage error before returning matches without `--output`.
pub fn heap_profile_main<A: Adapter>(peak: PeakAlloc) -> ExitCode {
    let matches: clap::ArgMatches = clap::Command::new("heap_profile")
        .about("Profile in-process peak heap of every benchmarked tool on every fixture")
        .arg(
            clap::Arg::new("output")
                .short('o')
                .long("output")
                .value_name("DIR")
                .required(true)
                .help(
                    "Raw capture directory receiving inprocess_heap.tsv and inprocess_compat.tsv",
                ),
        )
        .get_matches();
    let output: PathBuf = matches
        .get_one::<String>("output")
        .map(PathBuf::from)
        .expect("required --output");
    match heap_profile::<A>(peak, &output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("heap_profile: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Profile every cell of the selected tools of `A` (see [`TOOLS_ENV`]) and append both TSV files
/// under `output`.
fn heap_profile<A: Adapter>(peak: PeakAlloc, output: &Path) -> Result<(), String> {
    let all_fixtures: Vec<Fixture> =
        fixtures::discover_configured().map_err(|e| format!("fixture discovery failed: {e}"))?;
    let tools: Vec<A> = selected_tools::<A>();
    let mut compat: String = String::new();
    let mut heap: String = String::new();
    for fixture in &all_fixtures {
        for operation in Operation::ALL {
            for tool in tools.iter().copied() {
                if !tool.supports(operation) {
                    continue;
                }
                match check(tool, fixture, operation) {
                    Ok(()) => {
                        let peak_bytes: u64 =
                            measure_heap(peak, tool, fixture, operation).ok_or_else(|| {
                                format!(
                                    "{tool}/{}/{operation}: preparation failed after a passing check",
                                    fixture.id
                                )
                            })?;
                        writeln!(compat, "{tool}\t{}\t{operation}\tpass\t", fixture.id)
                            .map_err(|e| e.to_string())?;
                        writeln!(heap, "{tool}\t{}\t{operation}\t{peak_bytes}", fixture.id)
                            .map_err(|e| e.to_string())?;
                        eprintln!("{tool}\t{}\t{operation}\t{peak_bytes} B", fixture.id);
                    }
                    Err(reason) => {
                        writeln!(
                            compat,
                            "{tool}\t{}\t{operation}\tfail\t{reason}",
                            fixture.id
                        )
                        .map_err(|e| e.to_string())?;
                        eprintln!("{tool}\t{}\t{operation}\tfail: {reason}", fixture.id);
                    }
                }
            }
        }
    }
    fs::create_dir_all(output).map_err(|e| format!("cannot create {}: {e}", output.display()))?;
    append(&output.join("inprocess_compat.tsv"), &compat)?;
    append(&output.join("inprocess_heap.tsv"), &heap)
}

/// Append `text` to the file at `path`, creating it when absent.
fn append(path: &Path, text: &str) -> Result<(), String> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file: fs::File| file.write_all(text.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Warm up once, then return the extra peak heap of one measured run of the cell.
/// `None` when inputs cannot be prepared (never the case for a cell whose check passed).
fn measure_heap<A: Adapter>(
    peak: PeakAlloc,
    tool: A,
    fixture: &Fixture,
    operation: Operation,
) -> Option<u64> {
    match operation {
        Operation::Codegen => Some(peak_of(peak, || {
            drop(black_box(tool.codegen(black_box(fixture))));
        })),
        Operation::ReverseCodegen => {
            let model: A::Model = tool.model(fixture).ok()?;
            Some(peak_of(peak, || {
                drop(black_box(A::reverse_codegen(black_box(model))));
            }))
        }
        Operation::SchemaCompile => Some(peak_of(peak, || {
            drop(black_box(tool.compile(black_box(fixture))));
        })),
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let instance: Value = parse_instance(operation.instance(fixture)?).ok()?;
            let compiled: A::Compiled = tool.compile(fixture).ok()?;
            Some(peak_of(peak, || {
                black_box(A::validate(black_box(&compiled), black_box(&instance)));
            }))
        }
        Operation::DeserializeValid
        | Operation::TryFromValid
        | Operation::TryFromInvalid
        | Operation::DeserializeValidNoNewtypes
        | Operation::TryFromValidNoNewtypes
        | Operation::TryFromInvalidNoNewtypes => {
            let (variant, call) = operation.generated_types()?;
            let text: &str = operation.instance(fixture)?;
            let types: GeneratedTypes = tool.generated_types(fixture, variant).ok()?;
            match call {
                TypesCall::Deserialize => Some(peak_of(peak, || {
                    black_box((types.deserialize)(black_box(text)));
                })),
                TypesCall::TryFrom => {
                    let try_from: fn(Value) -> bool = types.try_from.ok()?;
                    let instance: Value = parse_instance(text).ok()?;
                    Some(peak_of_consuming(peak, &instance, |value: Value| {
                        black_box(try_from(black_box(value)));
                    }))
                }
            }
        }
    }
}

/// Run `op` once as warmup, then once measured; return the measured run's peak heap above the
/// heap in use at its start.
fn peak_of(peak: PeakAlloc, mut op: impl FnMut()) -> u64 {
    op();
    let start: usize = peak.current_usage();
    peak.reset_peak_usage();
    op();
    let peak_bytes: usize = peak.peak_usage();
    u64::try_from(peak_bytes.saturating_sub(start)).unwrap_or(u64::MAX)
}

/// [`peak_of`] for an operation that consumes its input: each run gets its own clone of `input`,
/// made before the measured run starts, so only the operation's allocations are counted.
fn peak_of_consuming(peak: PeakAlloc, input: &Value, mut op: impl FnMut(Value)) -> u64 {
    op(input.clone());
    let owned: Value = input.clone();
    let start: usize = peak.current_usage();
    peak.reset_peak_usage();
    op(owned);
    let peak_bytes: usize = peak.peak_usage();
    u64::try_from(peak_bytes.saturating_sub(start)).unwrap_or(u64::MAX)
}

/// Environment variable restricting a run to some tools: their ids, comma-separated. Unset or
/// blank measures every tool. The quick benchmark sets it to our tool, so neither the heap
/// profile nor criterion spends time checking or measuring a competitor.
pub const TOOLS_ENV: &str = "BENCH_TOOLS";

/// The tool ids a [`TOOLS_ENV`] value selects: `None` (every tool) when the value is unset or
/// blank, else each non-blank comma-separated id, trimmed.
#[must_use]
pub fn tool_selection(value: Option<&str>) -> Option<Vec<String>> {
    let ids: Vec<String> = value?
        .split(',')
        .map(str::trim)
        .filter(|id: &&str| !id.is_empty())
        .map(str::to_owned)
        .collect();
    if ids.is_empty() { None } else { Some(ids) }
}

/// The tools of `A` this run measures, in `A::ALL` order (see [`TOOLS_ENV`]).
fn selected_tools<A: Adapter>() -> Vec<A> {
    let selection: Option<Vec<String>> = tool_selection(std::env::var(TOOLS_ENV).ok().as_deref());
    A::ALL
        .iter()
        .copied()
        .filter(|tool: &A| {
            selection
                .as_ref()
                .is_none_or(|ids: &Vec<String>| ids.iter().any(|id: &String| id == tool.id()))
        })
        .collect()
}

/// Environment variable restricting criterion to one cell per tool: `<operation>/<fixture id>`
/// (`validate_valid/small/config`). Unset or blank times every cell. The quick benchmark runs
/// each cell on its own, alternating the two builds, so only that cell is checked and timed (a
/// criterion filter would still run the correctness check of every cell first).
pub const CELL_ENV: &str = "BENCH_CELL";

/// The cell a [`CELL_ENV`] value selects: `None` (every cell) when the value is unset or blank,
/// else the operation token before the first `/` and the fixture id after it.
///
/// # Errors
///
/// Returns a reason when the operation token is unknown or the fixture id is empty.
pub fn cell_selection(value: Option<&str>) -> Result<Option<(Operation, String)>, String> {
    let Some(value) = value
        .map(str::trim)
        .filter(|value: &&str| !value.is_empty())
    else {
        return Ok(None);
    };
    let (token, fixture) = value.split_once('/').unwrap_or((value, ""));
    let operation: Operation = Operation::ALL
        .into_iter()
        .find(|operation: &Operation| operation.id() == token)
        .ok_or_else(|| format!("{CELL_ENV}={value}: unknown operation `{token}`"))?;
    if fixture.is_empty() {
        return Err(format!(
            "{CELL_ENV}={value}: no fixture id (expected `<operation>/<fixture id>`)"
        ));
    }
    Ok(Some((operation, fixture.to_owned())))
}

// --- Criterion timing --------------------------------------------------------------------

/// Samples per benchmark: enough for a stable median at a practical total runtime.
const SAMPLE_SIZE: usize = 20;
/// Warm-up per benchmark.
const WARM_UP: Duration = Duration::from_millis(500);
/// Target measurement time per benchmark (criterion extends it for slow cells).
const MEASUREMENT: Duration = Duration::from_secs(2);

/// Entry point of a criterion bench (`harness = false`): time every correct cell of `A`.
///
/// Benchmark ids are `<operation>/<tool>/<fixture id>`: the criterion group is the operation
/// token, the function id is the tool token, and the parameter is the fixture id. Only cells
/// whose correctness check passes ([`check`]) are timed; every skipped cell prints a one-line
/// `skip` notice to stderr. `codegen` and `schema_compile` start from the fixture's texts held in
/// memory; `validate_valid` and `validate_invalid` start from an already-compiled schema and an
/// already-parsed instance (both prepared outside the timed closure); `reverse_codegen` starts from
/// the fixture's model, looked up outside the timed closure.
///
/// Fixtures come from [`fixtures::discover_configured`] (`BENCH_FIXTURE_ROOTS`, or the committed
/// and generated fixture directories). [`TOOLS_ENV`] restricts the tools and [`CELL_ENV`] the
/// cell. Criterion CLI flags (`--warm-up-time`, `--measurement-time`, `--sample-size`, filters)
/// override the defaults set here.
pub fn cross_tool_main<A: Adapter>() -> ExitCode {
    let cell: Option<(Operation, String)> =
        match cell_selection(std::env::var(CELL_ENV).ok().as_deref()) {
            Ok(cell) => cell,
            Err(reason) => {
                eprintln!("cross_tool: {reason}");
                return ExitCode::FAILURE;
            }
        };
    let mut all_fixtures: Vec<Fixture> = match fixtures::discover_configured() {
        Ok(found) => found,
        Err(err) => {
            eprintln!("cross_tool: fixture discovery failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    if let Some((_, fixture_id)) = &cell {
        all_fixtures.retain(|fixture: &Fixture| &fixture.id == fixture_id);
        if all_fixtures.is_empty() {
            eprintln!("cross_tool: {CELL_ENV}: no fixture `{fixture_id}` under the fixture roots");
            return ExitCode::FAILURE;
        }
    }
    let mut criterion: Criterion = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .warm_up_time(WARM_UP)
        .measurement_time(MEASUREMENT)
        .configure_from_args();
    let tools: Vec<A> = selected_tools::<A>();
    for operation in Operation::ALL {
        if cell
            .as_ref()
            .is_some_and(|(selected, _): &(Operation, String)| *selected != operation)
        {
            continue;
        }
        bench_operation(&mut criterion, operation, &all_fixtures, &tools);
    }
    criterion.final_summary();
    ExitCode::SUCCESS
}

/// Benchmark every correct (tool, fixture) cell of `operation` among `tools` in one group named
/// after it.
fn bench_operation<A: Adapter>(
    criterion: &mut Criterion,
    operation: Operation,
    all_fixtures: &[Fixture],
    tools: &[A],
) {
    let mut group: BenchmarkGroup<'_, WallTime> = criterion.benchmark_group(operation.id());
    // Flat sampling keeps slow cells (whole-schema codegen and compilation of large schemas)
    // from ballooning into thousands of iterations per sample.
    if matches!(operation, Operation::Codegen | Operation::SchemaCompile) {
        group.sampling_mode(SamplingMode::Flat);
    }
    for fixture in all_fixtures {
        for tool in tools.iter().copied() {
            if !tool.supports(operation) {
                continue;
            }
            if let Err(reason) = check(tool, fixture, operation) {
                eprintln!("skip {operation}/{tool}/{}: {reason}", fixture.id);
                continue;
            }
            bench_cell(&mut group, operation, tool, fixture);
        }
    }
    group.finish();
}

/// Register one benchmark; `check` has already confirmed every call below succeeds.
fn bench_cell<A: Adapter>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    operation: Operation,
    tool: A,
    fixture: &Fixture,
) {
    let id: BenchmarkId = BenchmarkId::new(tool.id(), &fixture.id);
    match operation {
        Operation::Codegen => {
            group.bench_with_input(id, fixture, |b, fixture: &Fixture| {
                b.iter(|| tool.codegen(black_box(fixture)));
            });
        }
        Operation::ReverseCodegen => {
            let Ok(model) = tool.model(fixture) else {
                eprintln!("skip {operation}/{tool}/{}: preparation failed", fixture.id);
                return;
            };
            group.bench_with_input(id, &model, |b, model: &A::Model| {
                b.iter(|| A::reverse_codegen(black_box(*model)));
            });
        }
        Operation::SchemaCompile => {
            group.bench_with_input(id, fixture, |b, fixture: &Fixture| {
                b.iter(|| tool.compile(black_box(fixture)));
            });
        }
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let prepared: Option<(A::Compiled, Value)> = operation
                .instance(fixture)
                .and_then(|text: &str| parse_instance(text).ok())
                .and_then(|instance: Value| {
                    tool.compile(fixture)
                        .ok()
                        .map(|compiled: A::Compiled| (compiled, instance))
                });
            let Some(prepared) = prepared else {
                eprintln!("skip {operation}/{tool}/{}: preparation failed", fixture.id);
                return;
            };
            group.bench_with_input(
                id,
                &prepared,
                |b, (compiled, instance): &(A::Compiled, Value)| {
                    b.iter(|| A::validate(black_box(compiled), black_box(instance)));
                },
            );
        }
        Operation::DeserializeValid
        | Operation::TryFromValid
        | Operation::TryFromInvalid
        | Operation::DeserializeValidNoNewtypes
        | Operation::TryFromValidNoNewtypes
        | Operation::TryFromInvalidNoNewtypes => {
            bench_generated_types(group, id, operation, tool, fixture);
        }
    }
}

/// Register one generated-types benchmark. `deserialize` starts from the instance text; `try_from`
/// from a clone of the parsed instance, made outside the timing (criterion's `iter_batched`).
fn bench_generated_types<A: Adapter>(
    group: &mut BenchmarkGroup<'_, WallTime>,
    id: BenchmarkId,
    operation: Operation,
    tool: A,
    fixture: &Fixture,
) {
    let prepared: Option<(TypesCall, GeneratedTypes, &str)> =
        operation.generated_types().and_then(|(variant, call)| {
            let text: &str = operation.instance(fixture)?;
            let types: GeneratedTypes = tool.generated_types(fixture, variant).ok()?;
            Some((call, types, text))
        });
    let Some((call, types, text)) = prepared else {
        eprintln!("skip {operation}/{tool}/{}: preparation failed", fixture.id);
        return;
    };
    match call {
        TypesCall::Deserialize => {
            group.bench_with_input(id, text, |b, text: &str| {
                b.iter(|| (types.deserialize)(black_box(text)));
            });
        }
        TypesCall::TryFrom => {
            let (Ok(try_from), Ok(instance)) = (types.try_from, parse_instance(text)) else {
                eprintln!("skip {operation}/{tool}/{}: preparation failed", fixture.id);
                return;
            };
            group.bench_with_input(id, &instance, |b, instance: &Value| {
                b.iter_batched(
                    || instance.clone(),
                    |value: Value| try_from(black_box(value)),
                    BatchSize::SmallInput,
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Adapter, AdapterError, GeneratedTypes, Operation, TypesVariant, VALID_REJECTED_SKIP,
        cell_selection, check, guarded, machine_independent, one_line, tool_selection, unsupported,
    };
    use crate::fixtures::Fixture;
    use serde_json::Value;
    use std::convert::Infallible;
    use std::fmt;
    use std::path::PathBuf;

    /// A generated root type for the probe tests: deserializes from any object; its
    /// `TryFrom<Value>` accepts only `{"ok": true}`.
    #[derive(serde::Deserialize)]
    struct Probe {}

    impl TryFrom<Value> for Probe {
        type Error = String;

        fn try_from(value: Value) -> Result<Probe, String> {
            if value == serde_json::json!({"ok": true}) {
                Ok(Probe {})
            } else {
                Err(format!("rejected {value}"))
            }
        }
    }

    /// A tool whose generated types are [`Probe`] (with or without its `TryFrom`), or none.
    #[derive(Debug, Clone, Copy)]
    enum TypesTool {
        WithTryFrom,
        WithoutTryFrom,
        NoTypes,
    }

    impl fmt::Display for TypesTool {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.id())
        }
    }

    impl Adapter for TypesTool {
        type Compiled = Infallible;
        type Model = Infallible;
        type Reversed = Infallible;

        const ALL: &'static [TypesTool] = &[TypesTool::WithTryFrom];

        fn id(self) -> &'static str {
            "types"
        }

        fn supports(self, operation: Operation) -> bool {
            operation.generated_types().is_some()
        }

        fn codegen(self, _fixture: &Fixture) -> Result<String, AdapterError> {
            Err(unsupported(self, Operation::Codegen))
        }

        fn compile(self, _fixture: &Fixture) -> Result<Infallible, AdapterError> {
            Err(unsupported(self, Operation::SchemaCompile))
        }

        fn validate(compiled: &Infallible, _instance: &Value) -> bool {
            match *compiled {}
        }

        fn reverse_codegen(model: Infallible) -> Infallible {
            model
        }

        fn check_reversed(_fixture: &Fixture, reversed: &Infallible) -> Result<(), AdapterError> {
            match *reversed {}
        }

        fn generated_types(
            self,
            _fixture: &Fixture,
            _variant: TypesVariant,
        ) -> Result<GeneratedTypes, AdapterError> {
            match self {
                TypesTool::WithTryFrom => Ok(GeneratedTypes::with_try_from::<Probe>()),
                TypesTool::WithoutTryFrom => Ok(GeneratedTypes::without_try_from::<Probe>(
                    "the root type has no TryFrom<serde_json::Value>",
                )),
                TypesTool::NoTypes => Err(AdapterError::GeneratedTypes("no model".to_owned())),
            }
        }
    }

    /// A fixture with the given valid and invalid instance texts.
    fn instances_fixture(valid: &str, invalid: &str) -> Fixture {
        Fixture {
            id: "small/probe".to_owned(),
            dir: PathBuf::new(),
            schema_text: "{}".to_owned(),
            valid_instance: Some(valid.to_owned()),
            invalid_instance: Some(invalid.to_owned()),
            resources: Vec::new(),
        }
    }

    fn fixture_in(dir: &str) -> Fixture {
        Fixture {
            id: "small/config".to_owned(),
            dir: PathBuf::from(dir),
            schema_text: String::new(),
            valid_instance: None,
            invalid_instance: None,
            resources: Vec::new(),
        }
    }

    #[test]
    fn operation_ids() {
        let expected: Vec<&str> = vec![
            "codegen",
            "reverse_codegen",
            "schema_compile",
            "validate_valid",
            "validate_invalid",
            "deserialize_valid",
            "try_from_valid",
            "try_from_invalid",
            "deserialize_valid_no_newtypes",
            "try_from_valid_no_newtypes",
            "try_from_invalid_no_newtypes",
        ];
        let actual: Vec<&str> = Operation::ALL
            .iter()
            .map(|op: &Operation| op.id())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn guarded_turns_panic_into_error() {
        let expected: Result<(), AdapterError> = Err(AdapterError::Panic("boom".to_owned()));
        let actual: Result<(), AdapterError> = guarded(|| panic!("boom"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_line_collapses_whitespace() {
        let expected: String = "a b c".to_owned();
        let actual: String = one_line("a\tb\n\n c");
        assert_eq!(expected, actual);
    }

    #[test]
    fn machine_independent_replaces_the_fixture_directory_with_its_id() {
        let expected: String = "cannot parse small/config/schema.json: eof".to_owned();
        let actual: String = machine_independent(
            "cannot parse /home/me/fixtures/small/config/schema.json: eof",
            &fixture_in("/home/me/fixtures/small/config"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn machine_independent_keeps_a_reason_without_the_directory() {
        let expected: String = "compile error: bad".to_owned();
        let actual: String = machine_independent("compile error: bad", &fixture_in(""));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_selection_is_every_cell_when_unset() {
        let expected: Result<Option<(Operation, String)>, String> = Ok(None);
        let actual: Result<Option<(Operation, String)>, String> = cell_selection(None);
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_selection_is_every_cell_when_blank() {
        let expected: Result<Option<(Operation, String)>, String> = Ok(None);
        let actual: Result<Option<(Operation, String)>, String> = cell_selection(Some("  "));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_selection_splits_the_operation_from_a_nested_fixture_id() {
        let expected: Result<Option<(Operation, String)>, String> =
            Ok(Some((Operation::ValidateValid, "small/config".to_owned())));
        let actual: Result<Option<(Operation, String)>, String> =
            cell_selection(Some("validate_valid/small/config"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_selection_rejects_an_unknown_operation() {
        let expected: Result<Option<(Operation, String)>, String> =
            Err("BENCH_CELL=validate/small/config: unknown operation `validate`".to_owned());
        let actual: Result<Option<(Operation, String)>, String> =
            cell_selection(Some("validate/small/config"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cell_selection_rejects_a_missing_fixture_id() {
        let expected: Result<Option<(Operation, String)>, String> = Err(
            "BENCH_CELL=codegen: no fixture id (expected `<operation>/<fixture id>`)".to_owned(),
        );
        let actual: Result<Option<(Operation, String)>, String> = cell_selection(Some("codegen"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tool_selection_is_every_tool_when_unset() {
        let expected: Option<Vec<String>> = None;
        let actual: Option<Vec<String>> = tool_selection(None);
        assert_eq!(expected, actual);
    }

    #[test]
    fn tool_selection_is_every_tool_when_blank() {
        let expected: Option<Vec<String>> = None;
        let actual: Option<Vec<String>> = tool_selection(Some(" , "));
        assert_eq!(expected, actual);
    }

    #[test]
    fn tool_selection_lists_each_trimmed_id() {
        let expected: Option<Vec<String>> =
            Some(vec!["jsonschemars".to_owned(), "boon".to_owned()]);
        let actual: Option<Vec<String>> = tool_selection(Some("jsonschemars, boon,"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_deserialize_valid_passes_when_the_instance_deserializes() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::DeserializeValid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_deserialize_valid_fails_when_the_instance_does_not_deserialize() {
        let expected: Result<(), String> = Err(
            "wrong verdict: valid instance rejected: invalid type: integer `1`, expected struct Probe at line 1 column 1"
                .to_owned(),
        );
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture("1", r#"{"ok": false}"#),
            Operation::DeserializeValid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_valid_passes_when_try_from_accepts() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::TryFromValidNoNewtypes,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_valid_fails_when_try_from_rejects() {
        let expected: Result<(), String> =
            Err(r#"wrong verdict: valid instance rejected: rejected {"ok":false}"#.to_owned());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": false}"#, r#"{"ok": false}"#),
            Operation::TryFromValid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_invalid_passes_when_try_from_rejects() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::TryFromInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_invalid_fails_when_try_from_accepts() {
        let expected: Result<(), String> =
            Err("wrong verdict: invalid instance accepted".to_owned());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": true}"#),
            Operation::TryFromInvalidNoNewtypes,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_invalid_is_not_measured_when_the_valid_instance_is_rejected() {
        let expected: Result<(), String> = Err(VALID_REJECTED_SKIP.to_owned());
        let actual: Result<(), String> = check(
            TypesTool::WithTryFrom,
            &instances_fixture(r#"{"ok": false}"#, r#"{"ok": false}"#),
            Operation::TryFromInvalid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_try_from_fails_when_the_root_type_has_none() {
        let expected: Result<(), String> =
            Err("generated types: the root type has no TryFrom<serde_json::Value>".to_owned());
        let actual: Result<(), String> = check(
            TypesTool::WithoutTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::TryFromValid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_deserialize_runs_when_the_root_type_has_no_try_from() {
        let expected: Result<(), String> = Ok(());
        let actual: Result<(), String> = check(
            TypesTool::WithoutTryFrom,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::DeserializeValidNoNewtypes,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_fail_when_the_fixture_has_none() {
        let expected: Result<(), String> = Err("generated types: no model".to_owned());
        let actual: Result<(), String> = check(
            TypesTool::NoTypes,
            &instances_fixture(r#"{"ok": true}"#, r#"{"ok": false}"#),
            Operation::DeserializeValid,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generated_types_of_each_operation() {
        let expected: Vec<Option<(TypesVariant, super::TypesCall)>> = vec![
            None,
            None,
            None,
            None,
            None,
            Some((TypesVariant::Default, super::TypesCall::Deserialize)),
            Some((TypesVariant::Default, super::TypesCall::TryFrom)),
            Some((TypesVariant::Default, super::TypesCall::TryFrom)),
            Some((TypesVariant::NoNewtypes, super::TypesCall::Deserialize)),
            Some((TypesVariant::NoNewtypes, super::TypesCall::TryFrom)),
            Some((TypesVariant::NoNewtypes, super::TypesCall::TryFrom)),
        ];
        let actual: Vec<Option<(TypesVariant, super::TypesCall)>> = Operation::ALL
            .iter()
            .map(|operation: &Operation| operation.generated_types())
            .collect();
        assert_eq!(expected, actual);
    }
}
