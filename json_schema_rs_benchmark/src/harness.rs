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
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, SamplingMode};
use peak_alloc::PeakAlloc;
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
}

impl Operation {
    /// Every operation, in report order.
    pub const ALL: [Operation; 5] = [
        Operation::Codegen,
        Operation::ReverseCodegen,
        Operation::SchemaCompile,
        Operation::ValidateValid,
        Operation::ValidateInvalid,
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
        }
    }

    /// The instance text a validation operation runs on (`None` for non-validation
    /// operations, or when the fixture lacks that instance).
    #[must_use]
    pub fn instance(self, fixture: &Fixture) -> Option<&str> {
        match self {
            Operation::Codegen | Operation::ReverseCodegen | Operation::SchemaCompile => None,
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
            let expected_valid: bool = operation == Operation::ValidateValid;
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
#[must_use]
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
/// and generated fixture directories). Criterion CLI flags (`--warm-up-time`,
/// `--measurement-time`, `--sample-size`, filters) override the defaults set here.
#[must_use]
pub fn cross_tool_main<A: Adapter>() -> ExitCode {
    let all_fixtures: Vec<Fixture> = match fixtures::discover_configured() {
        Ok(found) => found,
        Err(err) => {
            eprintln!("cross_tool: fixture discovery failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    let mut criterion: Criterion = Criterion::default()
        .sample_size(SAMPLE_SIZE)
        .warm_up_time(WARM_UP)
        .measurement_time(MEASUREMENT)
        .configure_from_args();
    let tools: Vec<A> = selected_tools::<A>();
    for operation in Operation::ALL {
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
    }
}

#[cfg(test)]
mod tests {
    use super::{AdapterError, Operation, guarded, machine_independent, one_line, tool_selection};
    use crate::fixtures::Fixture;
    use std::path::PathBuf;

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
}
