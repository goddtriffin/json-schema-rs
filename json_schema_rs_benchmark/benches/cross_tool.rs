//! In-process criterion timing of every tool, linked as a library, on every fixture.
//!
//! Benchmark ids are `<operation>/<tool>/<fixture id>`: the criterion group is the operation
//! token, the function id is the tool token, and the parameter is the fixture id. Only cells
//! whose correctness check passes ([`adapters::check`]) are timed; every skipped cell prints a
//! one-line `skip` notice to stderr.
//!
//! Timed work per operation:
//! * `codegen`, `schema_compile` — start from the schema TEXT (and the texts of the fixture's
//!   resource documents) held in memory.
//! * `validate_valid`, `validate_invalid` — start from an already-compiled schema and an
//!   already-parsed instance (both prepared outside the timed closure).
//!
//! Fixtures come from [`fixtures::discover_configured`] (`BENCH_FIXTURE_ROOTS`, or the committed
//! and generated fixture directories). Criterion CLI flags (`--warm-up-time`,
//! `--measurement-time`, `--sample-size`, filters) override the defaults set here.

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, black_box};
use json_schema_rs_benchmark::adapters::{self, Compiled, Operation, Tool};
use json_schema_rs_benchmark::fixtures::{self, Fixture};
use serde_json::Value;
use std::process::ExitCode;
use std::time::Duration;

/// Samples per benchmark: enough for a stable median at a practical total runtime.
const SAMPLE_SIZE: usize = 20;
/// Warm-up per benchmark.
const WARM_UP: Duration = Duration::from_millis(500);
/// Target measurement time per benchmark (criterion extends it for slow cells).
const MEASUREMENT: Duration = Duration::from_secs(2);

fn main() -> ExitCode {
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
    for operation in Operation::ALL {
        bench_operation(&mut criterion, operation, &all_fixtures);
    }
    criterion.final_summary();
    ExitCode::SUCCESS
}

/// Benchmark every correct (tool, fixture) cell of `operation` in one group named after it.
fn bench_operation(criterion: &mut Criterion, operation: Operation, all_fixtures: &[Fixture]) {
    let mut group: BenchmarkGroup<'_, WallTime> = criterion.benchmark_group(operation.id());
    // Flat sampling keeps slow cells (whole-schema codegen and compilation of large schemas)
    // from ballooning into thousands of iterations per sample.
    if matches!(operation, Operation::Codegen | Operation::SchemaCompile) {
        group.sampling_mode(SamplingMode::Flat);
    }
    for fixture in all_fixtures {
        for tool in Tool::ALL {
            if !tool.supports(operation) {
                continue;
            }
            if let Err(reason) = adapters::check(tool, fixture, operation) {
                eprintln!(
                    "skip {}/{}/{}: {reason}",
                    operation.id(),
                    tool.id(),
                    fixture.id
                );
                continue;
            }
            bench_cell(&mut group, operation, tool, fixture);
        }
    }
    group.finish();
}

/// Register one benchmark; `check` has already confirmed every call below succeeds.
fn bench_cell(
    group: &mut BenchmarkGroup<'_, WallTime>,
    operation: Operation,
    tool: Tool,
    fixture: &Fixture,
) {
    let id: BenchmarkId = BenchmarkId::new(tool.id(), &fixture.id);
    match operation {
        Operation::Codegen => {
            group.bench_with_input(id, fixture, |b, fixture: &Fixture| {
                b.iter(|| {
                    adapters::codegen(
                        tool,
                        black_box(&fixture.schema_text),
                        black_box(&fixture.resources),
                    )
                });
            });
        }
        Operation::SchemaCompile => {
            group.bench_with_input(id, fixture, |b, fixture: &Fixture| {
                b.iter(|| {
                    adapters::compile(
                        tool,
                        black_box(&fixture.schema_text),
                        black_box(&fixture.resources),
                    )
                });
            });
        }
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let Some(prepared) = prepare_validation(operation, tool, fixture) else {
                eprintln!(
                    "skip {}/{}/{}: preparation failed",
                    operation.id(),
                    tool.id(),
                    fixture.id
                );
                return;
            };
            group.bench_with_input(
                id,
                &prepared,
                |b, (compiled, instance): &(Compiled, Value)| {
                    b.iter(|| adapters::validate(black_box(compiled), black_box(instance)));
                },
            );
        }
    }
}

/// Compile the schema and parse the instance outside the timed closure.
fn prepare_validation(
    operation: Operation,
    tool: Tool,
    fixture: &Fixture,
) -> Option<(Compiled, Value)> {
    let instance: Value = adapters::parse_instance(operation.instance(fixture)?).ok()?;
    let compiled: Compiled =
        adapters::compile(tool, &fixture.schema_text, &fixture.resources).ok()?;
    Some((compiled, instance))
}
