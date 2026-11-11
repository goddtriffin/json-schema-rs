//! Measures our codegen's generated code for every fixture (#113; see
//! `json_schema_rs_benchmark::code_size`) and writes `code_size.tsv`
//! (`fixture\tsettings\tmetric\tvalue`, sorted).
//!
//! `--generated DIR` holds what the CLI generated, one directory per settings token and fixture:
//! `DIR/<settings>/<fixture id, '/' as "__">/` (`default`, `no-newtypes`). Every `.rs` file under
//! a fixture's directory counts. A fixture whose directory is missing (its codegen failed) is
//! left out with a warning, so the comparison lists it as measured on one side only. The schema's
//! own size comes from the fixtures (`BENCH_FIXTURE_ROOTS`).
//!
//! Uses clap's builder API (not derive) because the workspace forbids `allow_attributes`
//! and clap's derive macro emits internal `#[allow(...)]` attributes.

use clap::{Arg, Command};
use json_schema_rs_benchmark::code_size::{
    CodeSize, CodeSizeRow, Settings, code_size_rows, code_size_tsv, measure_schema, measure_source,
};
use json_schema_rs_benchmark::fixtures::{Fixture, Resource, discover_configured};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let matches: clap::ArgMatches = Command::new("code_size")
        .about("Measure our codegen's generated code for every fixture")
        .arg(
            Arg::new("generated")
                .long("generated")
                .value_name("DIR")
                .required(true)
                .help("Generated code, as DIR/<settings>/<fixture id with / as __>/"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .value_name("FILE")
                .required(true)
                .help("Where to write code_size.tsv"),
        )
        .get_matches();
    let arg = |name: &str| -> PathBuf {
        PathBuf::from(matches.get_one::<String>(name).expect("required argument"))
    };
    match run(&arg("generated"), &arg("output")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("code_size: error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(generated: &Path, output: &Path) -> Result<(), String> {
    let fixtures: Vec<Fixture> = discover_configured().map_err(|e| e.to_string())?;
    let mut rows: Vec<CodeSizeRow> = Vec::new();
    for fixture in &fixtures {
        let mut schema: CodeSize = CodeSize::new();
        measure_schema(
            std::iter::once(fixture.schema_text.as_str()).chain(
                fixture
                    .resources
                    .iter()
                    .map(|resource: &Resource| resource.text.as_str()),
            ),
            &mut schema,
        );
        for settings in Settings::ALL {
            let dir: PathBuf = generated
                .join(settings.token())
                .join(fixture.id.replace('/', "__"));
            if !dir.is_dir() {
                eprintln!(
                    "code_size: warning: {}: not generated; {} ({}) left out",
                    dir.display(),
                    fixture.id,
                    settings.token()
                );
                continue;
            }
            let mut size: CodeSize = schema.clone();
            for file in rust_files(&dir)? {
                let source: String = std::fs::read_to_string(&file)
                    .map_err(|e| format!("reading {}: {e}", file.display()))?;
                measure_source(&source, &mut size);
            }
            rows.extend(code_size_rows(&fixture.id, settings, &size));
        }
    }
    rows.sort();
    std::fs::write(output, code_size_tsv(&rows))
        .map_err(|e| format!("writing {}: {e}", output.display()))
}

/// Every `.rs` file under `root`, walked with an explicit stack.
fn rust_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries: std::fs::ReadDir =
            std::fs::read_dir(&dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
        for entry in entries {
            let path: PathBuf = entry
                .map_err(|e| format!("reading {}: {e}", dir.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }
    Ok(found)
}
