//! Aggregator: collects the driver's raw captures (tab-separated CLI wall-time/memory
//! measurements, the codegen compat matrix) plus our in-process criterion estimates into a
//! single [`BenchmarkResults`], then writes `results.json` (machine-readable) and
//! `results.md` (human-readable, with a winner marked per category) for commit and review.
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use json_schema_rs_benchmark::{
    BenchmarkResults, Category, Measurement, collapse_measurements, parse_cli_measurements,
    parse_compat, parse_criterion_estimate_secs,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches = Command::new("aggregate")
        .about("Aggregate benchmark captures into results.{json,md}")
        .arg(
            Arg::new("input")
                .long("input")
                .value_name("DIR")
                .required(true)
                .help("Directory containing raw captures produced by the harness driver"),
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
            eprintln!("aggregate: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(input: &Path, criterion: Option<&Path>, output: &Path) -> Result<(), String> {
    let mut results: BenchmarkResults = BenchmarkResults::default();

    // Cross-tool CLI wall-time + peak memory (tab-separated, written by the driver).
    let cli_path: PathBuf = input.join("cli_measurements.tsv");
    if let Some(text) = read_optional(&cli_path)? {
        results.measurements.extend(parse_cli_measurements(&text));
    }

    // Codegen pass/fail compat matrix (tab-separated).
    let compat_path: PathBuf = input.join("compat.tsv");
    if let Some(text) = read_optional(&compat_path)? {
        results.compat.extend(parse_compat(&text));
    }

    // Output-determinism matrix (same tab-separated shape as compat).
    let determinism_path: PathBuf = input.join("determinism.tsv");
    if let Some(text) = read_optional(&determinism_path)? {
        results.determinism.extend(parse_compat(&text));
    }

    // dhat peak heap for our library (a single integer, in bytes).
    let heap_path: PathBuf = input.join("heap_peak.txt");
    if let Some(text) = read_optional(&heap_path)? {
        results.heap_peak_bytes = text.trim().parse::<u64>().ok();
    }

    // Our in-process criterion estimates (schema-compile split, validate, codegen).
    if let Some(dir) = criterion {
        results.measurements.extend(criterion_measurements(dir)?);
    }

    // Collapse duplicate (tool, fixture, category) samples so winners and the detailed
    // table agree (e.g. peak RSS is sampled during both codegen and validation).
    results.measurements = collapse_measurements(&results.measurements);

    std::fs::create_dir_all(output)
        .map_err(|e| format!("creating output dir {}: {e}", output.display()))?;

    let json_path: PathBuf = output.join("results.json");
    let json: String = serde_json::to_string_pretty(&results)
        .map_err(|e| format!("serializing results.json: {e}"))?;
    std::fs::write(&json_path, json)
        .map_err(|e| format!("writing {}: {e}", json_path.display()))?;

    let md_path: PathBuf = output.join("results.md");
    std::fs::write(&md_path, results.to_markdown())
        .map_err(|e| format!("writing {}: {e}", md_path.display()))?;

    Ok(())
}

/// Read a file that may not exist; `Ok(None)` when absent, `Err` on a real I/O failure.
fn read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("reading {}: {e}", path.display())),
    }
}

/// Map a criterion benchmark-group name to the metric category it represents.
fn category_for_group(group: &str) -> Option<Category> {
    match group {
        "schema_compile" => Some(Category::SchemaCompileTimeSecs),
        "validate_valid" => Some(Category::ValidateValidTimeSecs),
        "validate_invalid" => Some(Category::ValidateInvalidTimeSecs),
        "codegen" => Some(Category::CodegenWallTimeSecs),
        _ => None,
    }
}

/// Walk a criterion output directory (iteratively, no recursion) and turn every
/// `<group>/<function>/new/estimates.json` into a measurement for our library.
///
/// The fixture id is `criterion/<function>` so these in-process numbers never collide with
/// the cross-tool CLI fixtures. Only our library (`jsonschemars`) has in-process estimates.
fn criterion_measurements(root: &Path) -> Result<Vec<Measurement>, String> {
    let mut out: Vec<Measurement> = Vec::new();
    if !root.exists() {
        return Ok(out);
    }

    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) => return Err(format!("reading {}: {e}", dir.display())),
        };
        for entry in entries {
            let entry =
                entry.map_err(|e| format!("reading dir entry in {}: {e}", dir.display()))?;
            let path: PathBuf = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) != Some("estimates.json") {
                continue;
            }
            // Expect .../<group>/<function>/new/estimates.json
            let new_dir: &Path = match path.parent() {
                Some(p) if p.file_name().and_then(|n| n.to_str()) == Some("new") => p,
                _ => continue,
            };
            let Some((group, function)) = new_dir.parent().and_then(|func_dir| {
                let function: &str = func_dir.file_name()?.to_str()?;
                let group: &str = func_dir.parent()?.file_name()?.to_str()?;
                Some((group.to_owned(), function.to_owned()))
            }) else {
                continue;
            };
            let Some(category) = category_for_group(&group) else {
                continue;
            };
            let text: String = std::fs::read_to_string(&path)
                .map_err(|e| format!("reading {}: {e}", path.display()))?;
            if let Some(secs) = parse_criterion_estimate_secs(&text) {
                out.push(Measurement {
                    tool: "jsonschemars".to_owned(),
                    fixture: format!("criterion/{function}"),
                    category,
                    value: secs,
                });
            }
        }
    }
    // Deterministic order regardless of filesystem iteration order.
    out.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.fixture.cmp(&b.fixture))
    });
    Ok(out)
}
