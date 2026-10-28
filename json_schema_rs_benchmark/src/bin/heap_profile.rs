//! In-process peak-heap profile of every (tool, fixture, operation) cell, and the
//! authoritative in-process compatibility check.
//!
//! Usage: `heap_profile --output <RAW_DIR>`. Writes two tab-separated files (no header):
//! * `RAW_DIR/inprocess_compat.tsv` — `tool\tfixture\toperation\tstatus\tdetail`, one row per
//!   supported cell; status `pass` | `fail`, detail a one-line reason (empty on pass).
//! * `RAW_DIR/inprocess_heap.tsv` — `tool\tfixture\toperation\tpeak_bytes` for every passing
//!   cell: the peak heap the operation allocated above the heap in use when it started.
//!
//! Each measured cell runs once as warmup (populating lazy statics and caches), then once
//! measured. Inputs (schema text, compiled schema, parsed instance) are prepared before the
//! measurement starts, so only the operation itself is counted. Rows follow fixture id, then
//! operation, then tool order, so output is deterministic.
//!
//! Allocation is counted by `peak_alloc` installed as the global allocator (the workspace
//! forbids `unsafe_code`, which a hand-written `GlobalAlloc` would need).
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use json_schema_rs_benchmark::adapters::{self, Compiled, Operation, Tool};
use json_schema_rs_benchmark::fixtures::{self, Fixture};
use peak_alloc::PeakAlloc;
use serde_json::Value;
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::process::ExitCode;

#[global_allocator]
static PEAK: PeakAlloc = PeakAlloc;

fn main() -> ExitCode {
    let matches: clap::ArgMatches = Command::new("heap_profile")
        .about("Profile in-process peak heap of every benchmarked tool on every fixture")
        .arg(
            Arg::new("output")
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

    match run(&output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("heap_profile: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Profile every cell and write both TSV files under `output`.
fn run(output: &PathBuf) -> Result<(), String> {
    let all_fixtures: Vec<Fixture> =
        fixtures::discover_configured().map_err(|e| format!("fixture discovery failed: {e}"))?;
    let mut compat: String = String::new();
    let mut heap: String = String::new();
    for fixture in &all_fixtures {
        for operation in Operation::ALL {
            for tool in Tool::ALL {
                if !tool.supports(operation) {
                    continue;
                }
                match adapters::check(tool, fixture, operation) {
                    Ok(()) => {
                        let peak_bytes: u64 =
                            measure(tool, fixture, operation).ok_or_else(|| {
                                format!(
                                    "{}/{}/{}: preparation failed after a passing check",
                                    tool.id(),
                                    fixture.id,
                                    operation.id()
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
    let compat_path: PathBuf = output.join("inprocess_compat.tsv");
    let heap_path: PathBuf = output.join("inprocess_heap.tsv");
    fs::write(&compat_path, compat)
        .map_err(|e| format!("cannot write {}: {e}", compat_path.display()))?;
    fs::write(&heap_path, heap)
        .map_err(|e| format!("cannot write {}: {e}", heap_path.display()))?;
    Ok(())
}

/// Warm up once, then return the extra peak heap of one measured run of the cell.
/// `None` when inputs cannot be prepared (never the case for a cell whose check passed).
fn measure(tool: Tool, fixture: &Fixture, operation: Operation) -> Option<u64> {
    match operation {
        Operation::Codegen => {
            let text: &str = &fixture.schema_text;
            Some(peak_of(|| {
                drop(black_box(adapters::codegen(tool, black_box(text))));
            }))
        }
        Operation::SchemaCompile => {
            let text: &str = &fixture.schema_text;
            Some(peak_of(|| {
                drop(black_box(adapters::compile(tool, black_box(text))));
            }))
        }
        Operation::ValidateValid | Operation::ValidateInvalid => {
            let instance: Value = adapters::parse_instance(operation.instance(fixture)?).ok()?;
            let compiled: Compiled = adapters::compile(tool, &fixture.schema_text).ok()?;
            Some(peak_of(|| {
                black_box(adapters::validate(
                    black_box(&compiled),
                    black_box(&instance),
                ));
            }))
        }
    }
}

/// Run `op` once as warmup, then once measured; return the measured run's peak heap above the
/// heap in use at its start.
fn peak_of(mut op: impl FnMut()) -> u64 {
    op();
    let start: usize = PEAK.current_usage();
    PEAK.reset_peak_usage();
    op();
    let peak: usize = PEAK.peak_usage();
    u64::try_from(peak.saturating_sub(start)).unwrap_or(u64::MAX)
}
