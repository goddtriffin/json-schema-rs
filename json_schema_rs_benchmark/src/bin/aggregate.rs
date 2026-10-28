//! Aggregator: reads the raw captures of both benchmark tiers and writes `results.json`
//! (machine-readable) and `results.md` (human-readable) for commit and review.
//!
//! Inputs, all optional (a missing file contributes nothing):
//!
//! - `--criterion DIR` (usually `target/criterion`): every `*/new/benchmark.json` with its
//!   sibling `estimates.json` and `sample.json` becomes an in-process time measurement.
//! - `RAW_DIR/inprocess_heap.tsv`: in-process peak heap per (tool, fixture, operation).
//! - `RAW_DIR/inprocess_compat.tsv`, `RAW_DIR/cli/compat.tsv`: compatibility matrices.
//! - `RAW_DIR/cli/index.tsv`: one hyperfine export per CLI cell (time and peak RSS).
//! - `RAW_DIR/tool_versions.tsv`: version of each tool.
//!
//! Malformed lines and unreadable per-cell files are skipped with a warning on stderr naming
//! the file (and line); they never fail the run silently. All parsing and rendering logic is
//! in the library; this binary only does file I/O.
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use json_schema_rs_benchmark::{
    BenchmarkResults, CriterionId, IndexEntry, Measurement, Stats, Tier, TsvParse,
    is_criterion_new_benchmark, parse_compat_tsv, parse_criterion_benchmark, parse_criterion_stats,
    parse_heap_tsv, parse_hyperfine_json, parse_index_tsv, parse_tool_versions_tsv,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches: clap::ArgMatches = Command::new("aggregate")
        .about("Aggregate benchmark captures into results.{json,md}")
        .arg(
            Arg::new("input")
                .long("input")
                .value_name("DIR")
                .required(true)
                .help("Raw capture directory (RAW_DIR) produced by the harness driver"),
        )
        .arg(
            Arg::new("criterion")
                .long("criterion")
                .value_name("DIR")
                .help("criterion output directory (usually target/criterion); optional"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_name("DIR")
                .required(true)
                .help("Directory to write results.json and results.md into"),
        )
        .get_matches();

    let input: PathBuf = matches
        .get_one::<String>("input")
        .map(PathBuf::from)
        .expect("required --input");
    let criterion: Option<PathBuf> = matches.get_one::<String>("criterion").map(PathBuf::from);
    let output: PathBuf = matches
        .get_one::<String>("output")
        .map(PathBuf::from)
        .expect("required --output");

    match run(&input, criterion.as_deref(), &output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("aggregate: error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, criterion: Option<&Path>, output: &Path) -> Result<(), String> {
    let mut results: BenchmarkResults = BenchmarkResults::default();

    if let Some(dir) = criterion {
        results.measurements.extend(criterion_measurements(dir)?);
    }

    let heap_path: PathBuf = input.join("inprocess_heap.tsv");
    if let Some(text) = read_optional(&heap_path)? {
        results
            .measurements
            .extend(report_skipped(&heap_path, parse_heap_tsv(&text)));
    }

    let inprocess_compat_path: PathBuf = input.join("inprocess_compat.tsv");
    if let Some(text) = read_optional(&inprocess_compat_path)? {
        results.compat.extend(report_skipped(
            &inprocess_compat_path,
            parse_compat_tsv(&text, Tier::InProcess),
        ));
    }

    let cli_compat_path: PathBuf = input.join("cli").join("compat.tsv");
    if let Some(text) = read_optional(&cli_compat_path)? {
        results.compat.extend(report_skipped(
            &cli_compat_path,
            parse_compat_tsv(&text, Tier::Cli),
        ));
    }

    let index_path: PathBuf = input.join("cli").join("index.tsv");
    if let Some(text) = read_optional(&index_path)? {
        let entries: Vec<IndexEntry> = report_skipped(&index_path, parse_index_tsv(&text));
        results
            .measurements
            .extend(hyperfine_measurements(input, &entries)?);
    }

    let versions_path: PathBuf = input.join("tool_versions.tsv");
    if let Some(text) = read_optional(&versions_path)? {
        results.tool_versions.extend(report_skipped(
            &versions_path,
            parse_tool_versions_tsv(&text),
        ));
    }

    for warning in results.normalize() {
        eprintln!("aggregate: warning: {warning}");
    }

    std::fs::create_dir_all(output)
        .map_err(|e| format!("creating output dir {}: {e}", output.display()))?;

    let json_path: PathBuf = output.join("results.json");
    let mut json: String = serde_json::to_string_pretty(&results)
        .map_err(|e| format!("serializing results.json: {e}"))?;
    json.push('\n');
    std::fs::write(&json_path, json)
        .map_err(|e| format!("writing {}: {e}", json_path.display()))?;

    let md_path: PathBuf = output.join("results.md");
    std::fs::write(&md_path, results.to_markdown())
        .map_err(|e| format!("writing {}: {e}", md_path.display()))?;

    Ok(())
}

/// Print a warning for every skipped line of `path` and return the parsed rows.
fn report_skipped<T>(path: &Path, parsed: TsvParse<T>) -> Vec<T> {
    for skipped in &parsed.skipped {
        eprintln!(
            "aggregate: warning: {}:{}: skipped malformed line: {}",
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

/// Load the hyperfine export of every CLI index entry. An unreadable or malformed export is
/// skipped with a warning naming the file.
fn hyperfine_measurements(
    input: &Path,
    entries: &[IndexEntry],
) -> Result<Vec<Measurement>, String> {
    let mut out: Vec<Measurement> = Vec::new();
    for entry in entries {
        let path: PathBuf = input.join(&entry.json_path);
        let Some(text) = read_optional(&path)? else {
            eprintln!(
                "aggregate: warning: {}: hyperfine export listed in cli/index.tsv is missing",
                path.display()
            );
            continue;
        };
        match parse_hyperfine_json(&text, &entry.tool, &entry.fixture, entry.operation) {
            Ok(measurements) => out.extend(measurements),
            Err(reason) => {
                eprintln!("aggregate: warning: {}: skipped: {reason}", path.display());
            }
        }
    }
    Ok(out)
}

/// Walk a criterion output directory iteratively (explicit stack, no recursion), collect every
/// `*/new/benchmark.json` in sorted path order, and turn each benchmark of a known operation
/// into an in-process time measurement.
fn criterion_measurements(root: &Path) -> Result<Vec<Measurement>, String> {
    if !root.exists() {
        eprintln!(
            "aggregate: warning: {}: criterion directory does not exist; no in-process timings",
            root.display()
        );
        return Ok(Vec::new());
    }

    let mut benchmark_files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries: std::fs::ReadDir =
            std::fs::read_dir(&dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
        for entry in entries {
            let entry: std::fs::DirEntry =
                entry.map_err(|e| format!("reading dir entry in {}: {e}", dir.display()))?;
            let path: PathBuf = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_criterion_new_benchmark(&path) {
                benchmark_files.push(path);
            }
        }
    }
    benchmark_files.sort();

    let mut out: Vec<Measurement> = Vec::new();
    for benchmark_path in &benchmark_files {
        let text: String = std::fs::read_to_string(benchmark_path)
            .map_err(|e| format!("reading {}: {e}", benchmark_path.display()))?;
        let id: CriterionId = match parse_criterion_benchmark(&text) {
            Ok(Some(id)) => id,
            Ok(None) => continue,
            Err(reason) => {
                eprintln!(
                    "aggregate: warning: {}: skipped: {reason}",
                    benchmark_path.display()
                );
                continue;
            }
        };
        let new_dir: &Path = benchmark_path.parent().unwrap_or(root);
        let estimates_path: PathBuf = new_dir.join("estimates.json");
        let sample_path: PathBuf = new_dir.join("sample.json");
        let (Some(estimates), Some(sample)) = (
            read_optional(&estimates_path)?,
            read_optional(&sample_path)?,
        ) else {
            eprintln!(
                "aggregate: warning: {}: skipped: estimates.json or sample.json is missing",
                new_dir.display()
            );
            continue;
        };
        let stats: Stats = match parse_criterion_stats(&estimates, &sample) {
            Ok(stats) => stats,
            Err(reason) => {
                eprintln!(
                    "aggregate: warning: {}: skipped: {reason}",
                    new_dir.display()
                );
                continue;
            }
        };
        out.push(Measurement {
            tier: Tier::InProcess,
            tool: id.tool,
            fixture: id.fixture,
            operation: id.operation,
            metric: json_schema_rs_benchmark::Metric::TimeSecs,
            stats,
        });
    }
    Ok(out)
}
