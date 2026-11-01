//! File I/O: locating and loading Bowtie reports, suite files, the fixture manifest and
//! fixture documents, and writing the outputs.
//!
//! Report layout under `--reports-dir`:
//!
//! - `suite/<draft>.jsonl` (required) for every draft in [`crate::DRAFTS`], from
//!   `bowtie suite <suite>/tests/<draft>`;
//! - `suite/<draft>__optional.jsonl` (required) for every draft's optional suite
//!   (`tests/<draft>/optional/`); every case in it is reported as optional;
//! - `fixtures/<fixture id with / replaced by __>__valid.jsonl` and `...__invalid.jsonl`
//!   (required) for every manifest fixture, from `bowtie run` on one case per expectation: the
//!   schema, its instances in manifest order, and the fixture's resources as the registry.

use crate::error::DifferentialError;
use crate::messages::{MessageCase, fixture_cases, select_suite_case};
use crate::report::{BowtieReport, parse_report};
use crate::suite::{SuiteFile, SuiteFileCase, SuiteIndex};
use crate::{DRAFTS, DifferentialReport, FixtureInput, SuiteInput, build_report, markdown};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// The draft whose suite files feed the curated error-message comparison.
pub const MESSAGE_DRAFT: &str = "draft2020-12";

/// Command-line configuration of the report builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Directory holding `suite/` and `fixtures/` Bowtie reports.
    pub reports_dir: PathBuf,
    /// Root of the JSON Schema Test Suite checkout (contains `tests/`).
    pub suite_dir: PathBuf,
    /// Benchmark fixtures directory (contains `manifest.json`).
    pub fixtures_dir: PathBuf,
    /// Our implementation id in the Bowtie reports.
    pub ours: String,
    /// Directory `results.json` and `results.md` are written to.
    pub out_dir: PathBuf,
}

/// One manifest entry (unknown manifest fields are ignored).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ManifestFixture {
    /// Fixture id, e.g. `small/config`.
    pub id: String,
    /// Schema path relative to the fixtures directory.
    pub schema: String,
    /// Valid instance paths relative to the fixtures directory.
    pub valid_instances: Vec<String>,
    /// Invalid instance paths relative to the fixtures directory.
    pub invalid_instances: Vec<String>,
}

/// Path of the required suite report for `draft`.
#[must_use]
pub fn suite_report_path(reports_dir: &Path, draft: &str) -> PathBuf {
    reports_dir.join("suite").join(format!("{draft}.jsonl"))
}

/// Path of the optional-suite report for `draft`.
#[must_use]
pub fn optional_suite_report_path(reports_dir: &Path, draft: &str) -> PathBuf {
    reports_dir
        .join("suite")
        .join(format!("{draft}__optional.jsonl"))
}

/// Path of a fixture report; `kind` is `valid` or `invalid`.
#[must_use]
pub fn fixture_report_path(reports_dir: &Path, fixture_id: &str, kind: &str) -> PathBuf {
    reports_dir
        .join("fixtures")
        .join(format!("{}__{kind}.jsonl", fixture_id.replace('/', "__")))
}

fn io_error(path: &Path, source: std::io::Error) -> DifferentialError {
    DifferentialError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn read_text(path: &Path) -> Result<String, DifferentialError> {
    fs::read_to_string(path).map_err(|e: std::io::Error| io_error(path, e))
}

fn read_json<T>(path: &Path) -> Result<T, DifferentialError>
where
    T: for<'de> Deserialize<'de>,
{
    let text: String = read_text(path)?;
    serde_json::from_str(&text).map_err(|source: serde_json::Error| DifferentialError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Load a Bowtie report and check that `ours` participates in it.
///
/// # Errors
///
/// [`DifferentialError::MissingReport`] when the file does not exist, plus I/O, parse, and
/// missing-implementation errors.
pub fn load_report(path: &Path, ours: &str) -> Result<BowtieReport, DifferentialError> {
    if !path.is_file() {
        return Err(DifferentialError::MissingReport {
            path: path.to_path_buf(),
        });
    }
    let text: String = read_text(path)?;
    let report: BowtieReport =
        parse_report(&text).map_err(|source: crate::report::ReportError| {
            DifferentialError::Report {
                path: path.to_path_buf(),
                source,
            }
        })?;
    if !report.implementations.contains_key(ours) {
        return Err(DifferentialError::MissingImplementation {
            ours: ours.to_string(),
            path: path.to_path_buf(),
            available: report.implementations.keys().cloned().collect(),
        });
    }
    Ok(report)
}

/// Load every `.json` file under `<suite_dir>/tests/<draft>/` (optional subdirectories
/// included), sorted by draft-relative path.
///
/// # Errors
///
/// I/O and JSON errors for the directory walk and each file.
pub fn load_suite_files(
    suite_dir: &Path,
    draft: &str,
) -> Result<Vec<SuiteFile>, DifferentialError> {
    let root: PathBuf = suite_dir.join("tests").join(draft);
    let mut stack: Vec<PathBuf> = vec![root.clone()];
    let mut paths: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        let entries: fs::ReadDir =
            fs::read_dir(&dir).map_err(|e: std::io::Error| io_error(&dir, e))?;
        for entry in entries {
            let entry: fs::DirEntry = entry.map_err(|e: std::io::Error| io_error(&dir, e))?;
            let path: PathBuf = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e: &std::ffi::OsStr| e == "json")
            {
                paths.push(path);
            }
        }
    }
    let mut files: Vec<SuiteFile> = Vec::with_capacity(paths.len());
    for path in paths {
        let relative: String = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .components()
            .map(|c: std::path::Component<'_>| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<String>>()
            .join("/");
        let cases: Vec<SuiteFileCase> = read_json(&path)?;
        files.push(SuiteFile {
            path: relative,
            cases,
        });
    }
    files.sort_by(|a: &SuiteFile, b: &SuiteFile| a.path.cmp(&b.path));
    Ok(files)
}

/// Load the benchmark fixture manifest.
///
/// # Errors
///
/// I/O and JSON errors.
pub fn load_manifest(fixtures_dir: &Path) -> Result<Vec<ManifestFixture>, DifferentialError> {
    read_json(&fixtures_dir.join("manifest.json"))
}

/// Load every input, build the report, and render the markdown.
///
/// # Errors
///
/// Any loading error, a missing report, or a fixture/report mismatch.
pub fn run(config: &Config) -> Result<(DifferentialReport, String), DifferentialError> {
    let mut suites: Vec<SuiteInput> = Vec::with_capacity(DRAFTS.len());
    let mut messages: Vec<MessageCase> = Vec::new();
    for draft in DRAFTS {
        let report: BowtieReport =
            load_report(&suite_report_path(&config.reports_dir, draft), &config.ours)?;
        let optional_report: BowtieReport = load_report(
            &optional_suite_report_path(&config.reports_dir, draft),
            &config.ours,
        )?;
        let files: Vec<SuiteFile> = load_suite_files(&config.suite_dir, draft)?;
        if draft == MESSAGE_DRAFT {
            messages.extend(
                files
                    .iter()
                    .filter_map(|f: &SuiteFile| select_suite_case(draft, f)),
            );
        }
        suites.push(SuiteInput {
            draft: draft.to_string(),
            report,
            optional_report: Some(optional_report),
            index: SuiteIndex::new(&files),
        });
    }

    let manifest: Vec<ManifestFixture> = load_manifest(&config.fixtures_dir)?;
    let mut fixtures: Vec<FixtureInput> = Vec::with_capacity(manifest.len());
    for fixture in &manifest {
        let valid: BowtieReport = load_report(
            &fixture_report_path(&config.reports_dir, &fixture.id, "valid"),
            &config.ours,
        )?;
        let invalid: BowtieReport = load_report(
            &fixture_report_path(&config.reports_dir, &fixture.id, "invalid"),
            &config.ours,
        )?;
        let schema: Value = read_json(&config.fixtures_dir.join(&fixture.schema))?;
        let mut invalid_instances: Vec<(String, Value)> =
            Vec::with_capacity(fixture.invalid_instances.len());
        for path in &fixture.invalid_instances {
            invalid_instances.push((path.clone(), read_json(&config.fixtures_dir.join(path))?));
        }
        messages.extend(fixture_cases(
            &fixture.id,
            &fixture.schema,
            &schema,
            &invalid_instances,
        ));
        fixtures.push(FixtureInput {
            id: fixture.id.clone(),
            valid: (valid, fixture.valid_instances.clone()),
            invalid: (invalid, fixture.invalid_instances.clone()),
        });
    }

    let report: DifferentialReport = build_report(&config.ours, &suites, &fixtures, messages)?;
    let markdown: String = markdown::render(&report);
    Ok((report, markdown))
}

/// Write `results.json` and `results.md` into `out_dir` (created when missing).
///
/// # Errors
///
/// I/O errors, or a JSON serialization error.
pub fn write_outputs(
    out_dir: &Path,
    report: &DifferentialReport,
    markdown: &str,
) -> Result<(), DifferentialError> {
    fs::create_dir_all(out_dir).map_err(|e: std::io::Error| io_error(out_dir, e))?;
    let json_path: PathBuf = out_dir.join("results.json");
    let mut json: String =
        serde_json::to_string_pretty(report).map_err(|source: serde_json::Error| {
            DifferentialError::Json {
                path: json_path.clone(),
                source,
            }
        })?;
    json.push('\n');
    fs::write(&json_path, json).map_err(|e: std::io::Error| io_error(&json_path, e))?;
    let md_path: PathBuf = out_dir.join("results.md");
    fs::write(&md_path, markdown).map_err(|e: std::io::Error| io_error(&md_path, e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{fixture_report_path, optional_suite_report_path, suite_report_path};
    use std::path::{Path, PathBuf};

    #[test]
    fn report_paths() {
        let reports: &Path = Path::new("r");
        let expected: Vec<PathBuf> = vec![
            PathBuf::from("r/suite/draft2019-09.jsonl"),
            PathBuf::from("r/suite/draft7__optional.jsonl"),
            PathBuf::from("r/fixtures/massive__k8s-deployment__invalid.jsonl"),
        ];
        let actual: Vec<PathBuf> = vec![
            suite_report_path(reports, "draft2019-09"),
            optional_suite_report_path(reports, "draft7"),
            fixture_report_path(reports, "massive/k8s-deployment", "invalid"),
        ];
        assert_eq!(expected, actual);
    }
}
