//! The quick benchmark's report: reads the raw captures of both sides (base commit and working
//! tree) and prints which of one tool's cells moved beyond noise, as the PR body's
//! *Performance* section (see `json_schema_rs_benchmark::compare`).
//!
//! Each side's directory holds what `scripts/quick_benchmark.sh` captured for it:
//!
//! - `heap/inprocess_heap.tsv`: in-process peak heap (one exact value per cell, one round);
//! - `criterion/<round>/...`: one criterion output directory per round, whose `sample.json`
//!   is that cell's round;
//! - `cli/index.tsv` and the hyperfine exports it lists (paths relative to the side's
//!   directory), one export per cell and round.
//!
//! Rows of other tools are ignored, so a side whose harness measured every tool still compares.
//! Malformed lines and unreadable files are skipped with a warning on stderr; a head side with no
//! samples at all is an error, because the run measured nothing.
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use json_schema_rs_benchmark::compare::{Comparison, Samples, add_round, compare};
use json_schema_rs_benchmark::{
    CellKey, CriterionId, HyperfineSamples, IndexEntry, Measurement, Metric, Tier, TsvParse,
    criterion_benchmark_files, parse_criterion_benchmark, parse_criterion_samples, parse_heap_tsv,
    parse_hyperfine_samples, parse_index_tsv,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches: clap::ArgMatches = Command::new("compare")
        .about("Compare one tool's cells between two quick-benchmark captures")
        .arg(
            Arg::new("base")
                .long("base")
                .value_name("DIR")
                .required(true)
                .help("Capture directory of the base side"),
        )
        .arg(
            Arg::new("head")
                .long("head")
                .value_name("DIR")
                .required(true)
                .help("Capture directory of the head side (the working tree)"),
        )
        .arg(
            Arg::new("tool")
                .long("tool")
                .value_name("ID")
                .default_value("jsonschemars")
                .help("The tool whose cells are compared"),
        )
        .arg(
            Arg::new("base-label")
                .long("base-label")
                .value_name("NAME")
                .default_value("main")
                .help("How the report names the base side"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_name("FILE")
                .help("Also write the report to FILE"),
        )
        .arg(
            Arg::new("moved-in-process")
                .long("moved-in-process")
                .value_name("FILE")
                .help("Write the moved in-process cells to FILE, one <operation>/<fixture id> per line"),
        )
        .arg(
            Arg::new("moved-cli")
                .long("moved-cli")
                .value_name("FILE")
                .help("Write the moved CLI cells to FILE, one <operation>/<fixture id> per line"),
        )
        .get_matches();
    let arg = |name: &str| -> String {
        matches
            .get_one::<String>(name)
            .cloned()
            .expect("required or defaulted argument")
    };
    let base: PathBuf = PathBuf::from(arg("base"));
    let head: PathBuf = PathBuf::from(arg("head"));
    let tool: String = arg("tool");
    let base_label: String = arg("base-label");
    let path_arg =
        |name: &str| -> Option<PathBuf> { matches.get_one::<String>(name).map(PathBuf::from) };
    let outputs: Outputs = Outputs {
        report: path_arg("output"),
        moved_in_process: path_arg("moved-in-process"),
        moved_cli: path_arg("moved-cli"),
    };

    match run(&base, &head, &tool, &base_label, &outputs) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("compare: error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Where to write the report and the moved-cell lists, besides printing the report.
struct Outputs {
    report: Option<PathBuf>,
    moved_in_process: Option<PathBuf>,
    moved_cli: Option<PathBuf>,
}

fn run(
    base: &Path,
    head: &Path,
    tool: &str,
    base_label: &str,
    outputs: &Outputs,
) -> Result<(), String> {
    let base_samples: Samples = read_side(base, tool)?;
    let head_samples: Samples = read_side(head, tool)?;
    if head_samples.is_empty() {
        return Err(format!(
            "{}: no sample of {tool} was captured; nothing to compare",
            head.display()
        ));
    }
    let comparison: Comparison = compare(&base_samples, &head_samples);
    let report: String = comparison.to_markdown(base_label);
    let write = |path: &Path, text: &str| -> Result<(), String> {
        std::fs::write(path, text).map_err(|e| format!("writing {}: {e}", path.display()))
    };
    if let Some(path) = &outputs.report {
        write(path, &report)?;
    }
    for (path, tier) in [
        (&outputs.moved_in_process, Tier::InProcess),
        (&outputs.moved_cli, Tier::Cli),
    ] {
        if let Some(path) = path {
            let mut lines: String = comparison.moved_cells(tier).join("\n");
            if !lines.is_empty() {
                lines.push('\n');
            }
            write(path, &lines)?;
        }
    }
    print!("{report}");
    Ok(())
}

/// Every sample of `tool` one side captured, by cell and round.
fn read_side(dir: &Path, tool: &str) -> Result<Samples, String> {
    let mut samples: Samples = Samples::new();

    let heap_path: PathBuf = dir.join("heap").join("inprocess_heap.tsv");
    if let Some(text) = read_optional(&heap_path)? {
        for measurement in report_skipped(&heap_path, parse_heap_tsv(&text)) {
            if measurement.tool == tool {
                add_round(
                    &mut samples,
                    key_of(&measurement),
                    [measurement.stats.median],
                );
            }
        }
    }

    for benchmark_path in criterion_benchmark_files(&dir.join("criterion"))? {
        let text: String = std::fs::read_to_string(&benchmark_path)
            .map_err(|e| format!("reading {}: {e}", benchmark_path.display()))?;
        let id: CriterionId = match parse_criterion_benchmark(&text) {
            Ok(Some(id)) if id.tool == tool => id,
            Ok(_) => continue,
            Err(reason) => {
                eprintln!(
                    "compare: warning: {}: skipped: {reason}",
                    benchmark_path.display()
                );
                continue;
            }
        };
        let sample_path: PathBuf = benchmark_path.with_file_name("sample.json");
        let Some(sample) = read_optional(&sample_path)? else {
            eprintln!("compare: warning: {}: missing", sample_path.display());
            continue;
        };
        match parse_criterion_samples(&sample) {
            Ok(values) => add_round(
                &mut samples,
                CellKey {
                    tier: Tier::InProcess,
                    operation: id.operation,
                    metric: Metric::TimeSecs,
                    fixture: id.fixture,
                },
                values,
            ),
            Err(reason) => {
                eprintln!(
                    "compare: warning: {}: skipped: {reason}",
                    sample_path.display()
                );
            }
        }
    }

    let index_path: PathBuf = dir.join("cli").join("index.tsv");
    if let Some(text) = read_optional(&index_path)? {
        let entries: Vec<IndexEntry> = report_skipped(&index_path, parse_index_tsv(&text));
        for entry in entries
            .into_iter()
            .filter(|entry: &IndexEntry| entry.tool == tool)
        {
            let export_path: PathBuf = dir.join(&entry.json_path);
            let Some(json) = read_optional(&export_path)? else {
                eprintln!("compare: warning: {}: missing", export_path.display());
                continue;
            };
            let exported: HyperfineSamples = match parse_hyperfine_samples(&json) {
                Ok(exported) => exported,
                Err(reason) => {
                    eprintln!(
                        "compare: warning: {}: skipped: {reason}",
                        export_path.display()
                    );
                    continue;
                }
            };
            let cell = |metric: Metric| -> CellKey {
                CellKey {
                    tier: Tier::Cli,
                    operation: entry.operation,
                    metric,
                    fixture: entry.fixture.clone(),
                }
            };
            add_round(&mut samples, cell(Metric::TimeSecs), exported.times);
            if !exported.peak_memory_bytes.is_empty() {
                add_round(
                    &mut samples,
                    cell(Metric::PeakMemoryBytes),
                    exported.peak_memory_bytes,
                );
            }
        }
    }
    Ok(samples)
}

/// The cell a measurement belongs to.
fn key_of(measurement: &Measurement) -> CellKey {
    CellKey {
        tier: measurement.tier,
        operation: measurement.operation,
        metric: measurement.metric,
        fixture: measurement.fixture.clone(),
    }
}

/// Print a warning for every skipped line of `path` and return the parsed rows.
fn report_skipped<T>(path: &Path, parsed: TsvParse<T>) -> Vec<T> {
    for skipped in &parsed.skipped {
        eprintln!(
            "compare: warning: {}:{}: skipped malformed line: {}",
            path.display(),
            skipped.line,
            skipped.reason
        );
    }
    parsed.rows
}

/// Read a file that may not exist; `Ok(None)` when absent, `Err` on a real I/O failure.
fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}
