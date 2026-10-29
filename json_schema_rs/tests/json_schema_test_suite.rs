//! Gating test: run the library validator against the official JSON Schema Test Suite.
//!
//! Every suite test in drafts 4, 6, 7, 2019-09 and 2020-12 (including `optional/`) is one
//! reported test case, named `<draft>/<file> :: <group> :: <test>`. Known failures are listed
//! in `tests/suite_xfail.json` (draft → file → group → tests). The run fails on:
//!
//! - an **unexpected failure**: a test not in the manifest returns the wrong verdict, or its
//!   schema cannot be parsed;
//! - an **unexpected pass**: a test in the manifest now passes, so its entry must be removed;
//! - a **stale entry**: the manifest names a test that does not exist in the suite.
//!
//! Each group's schema gets a [`Registry`] whose retriever serves the suite's `remotes/` folder
//! under `http://localhost:1234/`, as the suite specifies. Each directory's draft is the default
//! draft: a schema without `$schema` uses it, one with `$schema` follows that (so a custom
//! metaschema is found through the registry).
//!
//! The suite must be cloned at `research/json-schema-test-suite/` (`make vendor_test_suite`);
//! the run hard-fails when it is missing. `make update_suite_xfail` rewrites the manifest from
//! the current results (sets `JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE=1`).

use json_schema_rs::{
    FormatMode, JsonSchemaSettings, Registry, Schema, SpecVersion, ValidationError, validate,
    validate_with_registry,
};
use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// Suite draft directories we run, with the spec version each one targets.
const DRAFTS: [(&str, SpecVersion); 5] = [
    ("draft4", SpecVersion::Draft04),
    ("draft6", SpecVersion::Draft06),
    ("draft7", SpecVersion::Draft07),
    ("draft2019-09", SpecVersion::Draft201909),
    ("draft2020-12", SpecVersion::Draft202012),
];

/// Environment variable that switches the run to rewriting the manifest.
const UPDATE_ENV: &str = "JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE";

/// Known failures: draft → file (relative to the draft directory) → group → test descriptions.
type XfailManifest = BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeSet<String>>>>;

#[derive(Debug, Deserialize)]
struct SuiteGroup {
    description: String,
    schema: serde_json::Value,
    tests: Vec<SuiteTest>,
}

#[derive(Debug, Deserialize)]
struct SuiteTest {
    description: String,
    data: serde_json::Value,
    valid: bool,
}

/// Identity of one suite test; also the manifest key path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CaseKey {
    draft: String,
    file: String,
    group: String,
    test: String,
}

impl CaseKey {
    fn trial_name(&self) -> String {
        format!(
            "{}/{} :: {} :: {}",
            self.draft, self.file, self.group, self.test
        )
    }
}

/// Result of running one suite test against our library.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Passed,
    /// The test failed; the string says why.
    Failed(String),
}

/// One evaluated suite test.
#[derive(Debug)]
struct Case {
    key: CaseKey,
    outcome: Outcome,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Path to the JSON Schema Test Suite root (research/json-schema-test-suite).
fn suite_root() -> PathBuf {
    manifest_dir()
        .join("..")
        .join("research")
        .join("json-schema-test-suite")
}

fn xfail_path() -> PathBuf {
    manifest_dir().join("tests").join("suite_xfail.json")
}

/// Collects all `.json` files under `dir` (iteratively), sorted by path.
fn collect_json_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path: PathBuf = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Relative path of `file` under `draft_dir`, with `/` separators on every platform.
fn relative_file_name(draft_dir: &Path, file: &Path) -> String {
    let relative: &Path = file.strip_prefix(draft_dir).unwrap_or(file);
    let parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.join("/")
}

fn describe_errors(errors: &[ValidationError]) -> String {
    let messages: Vec<String> = errors.iter().map(ToString::to_string).collect();
    messages.join("; ")
}

/// The base URI the suite serves its `remotes/` folder under.
const REMOTES_BASE: &str = "http://localhost:1234/";

/// Every document under `remotes/`, keyed by its URI under [`REMOTES_BASE`].
fn load_remotes(root: &Path) -> Result<HashMap<String, serde_json::Value>, String> {
    let remotes_dir: PathBuf = root.join("remotes");
    let files: Vec<PathBuf> = collect_json_files(&remotes_dir)
        .map_err(|e| format!("failed to walk {}: {e}", remotes_dir.display()))?;
    let mut remotes: HashMap<String, serde_json::Value> = HashMap::new();
    for file in files {
        let contents: String = std::fs::read_to_string(&file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;
        let value: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", file.display()))?;
        let uri: String = format!("{REMOTES_BASE}{}", relative_file_name(&remotes_dir, &file));
        remotes.insert(uri, value);
    }
    Ok(remotes)
}

/// Runs one suite test; a panic in our library is a failure, not a crash of the run. A schema
/// whose registry could not be built is validated with [`validate`], which reports why.
fn run_test(schema: &Schema, registry: Option<&Registry<'_>>, test: &SuiteTest) -> Outcome {
    let result = catch_unwind(AssertUnwindSafe(|| match registry {
        Some(registry) => validate_with_registry(registry, &test.data),
        None => validate(schema, &test.data),
    }));
    match (result, test.valid) {
        (Err(_), _) => Outcome::Failed("validator panicked".to_string()),
        (Ok(Ok(())), true) | (Ok(Err(_)), false) => Outcome::Passed,
        (Ok(Ok(())), false) => Outcome::Failed("expected invalid, got valid".to_string()),
        (Ok(Err(errors)), true) => Outcome::Failed(format!(
            "expected valid, got invalid: {}",
            describe_errors(&errors)
        )),
    }
}

/// Evaluates every test in every draft. Errors are reasons the suite itself is unusable.
fn evaluate_suite(root: &Path) -> Result<Vec<Case>, String> {
    let tests_dir: PathBuf = root.join("tests");
    if !tests_dir.is_dir() {
        return Err(format!(
            "JSON Schema Test Suite not found at {}. Run `make vendor_test_suite` to clone it.",
            tests_dir.display()
        ));
    }
    let remotes: HashMap<String, serde_json::Value> = load_remotes(root)?;
    let mut cases: Vec<Case> = Vec::new();
    for (draft, spec_version) in DRAFTS {
        let draft_dir: PathBuf = tests_dir.join(draft);
        let files: Vec<PathBuf> = collect_json_files(&draft_dir)
            .map_err(|e| format!("failed to walk {}: {e}", draft_dir.display()))?;
        if files.is_empty() {
            return Err(format!("no suite files under {}", draft_dir.display()));
        }
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .default_spec_version(spec_version)
            .build();
        // The suite's `optional/format/` tests are written for implementations that assert
        // `format`, which 2019-09 and 2020-12 do not by default.
        let format_settings: JsonSchemaSettings = JsonSchemaSettings {
            format_mode: Some(FormatMode::Assertion),
            ..settings.clone()
        };
        for file_path in &files {
            let file: String = relative_file_name(&draft_dir, file_path);
            let settings: &JsonSchemaSettings = if file.starts_with("optional/format/") {
                &format_settings
            } else {
                &settings
            };
            let contents: String = std::fs::read_to_string(file_path)
                .map_err(|e| format!("failed to read {}: {e}", file_path.display()))?;
            let groups: Vec<SuiteGroup> = serde_json::from_str(&contents)
                .map_err(|e| format!("failed to parse {}: {e}", file_path.display()))?;
            for group in groups {
                let parsed = catch_unwind(AssertUnwindSafe(|| {
                    Schema::new_from_serde_value(&group.schema, settings)
                }));
                let schema: Result<Schema, String> = match parsed {
                    Ok(Ok(schema)) => Ok(schema),
                    Ok(Err(e)) => Err(format!("schema not supported: {e}")),
                    Err(_) => Err("schema parser panicked".to_string()),
                };
                let registry: Option<Registry<'_>> = schema.as_ref().ok().and_then(|schema| {
                    catch_unwind(AssertUnwindSafe(|| {
                        Registry::builder()
                            .retriever(&remotes)
                            .settings(settings.clone())
                            .build(schema)
                            .ok()
                    }))
                    .ok()
                    .flatten()
                });
                for test in group.tests {
                    let outcome: Outcome = match &schema {
                        Ok(schema) => run_test(schema, registry.as_ref(), &test),
                        Err(reason) => Outcome::Failed(reason.clone()),
                    };
                    cases.push(Case {
                        key: CaseKey {
                            draft: draft.to_string(),
                            file: file.clone(),
                            group: group.description.clone(),
                            test: test.description,
                        },
                        outcome,
                    });
                }
            }
        }
    }
    Ok(cases)
}

fn read_manifest() -> Result<XfailManifest, String> {
    let path: PathBuf = xfail_path();
    let contents: String = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    serde_json::from_str(&contents).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

fn manifest_keys(manifest: &XfailManifest) -> BTreeSet<CaseKey> {
    let mut keys: BTreeSet<CaseKey> = BTreeSet::new();
    for (draft, files) in manifest {
        for (file, groups) in files {
            for (group, tests) in groups {
                for test in tests {
                    keys.insert(CaseKey {
                        draft: draft.clone(),
                        file: file.clone(),
                        group: group.clone(),
                        test: test.clone(),
                    });
                }
            }
        }
    }
    keys
}

/// Rewrites the manifest so it lists exactly the currently failing tests.
fn write_manifest(cases: &[Case]) -> Result<usize, String> {
    let mut manifest: XfailManifest = BTreeMap::new();
    let mut count: usize = 0;
    for case in cases {
        if case.outcome != Outcome::Passed {
            manifest
                .entry(case.key.draft.clone())
                .or_default()
                .entry(case.key.file.clone())
                .or_default()
                .entry(case.key.group.clone())
                .or_default()
                .insert(case.key.test.clone());
            count += 1;
        }
    }
    let mut json: String = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("failed to serialize manifest: {e}"))?;
    json.push('\n');
    let path: PathBuf = xfail_path();
    std::fs::write(&path, json).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    Ok(count)
}

/// A single failing trial carrying a setup error, so the run reports it and exits non-zero.
fn setup_failure(name: &str, message: String) -> Vec<Trial> {
    vec![Trial::test(name, move || Err(Failed::from(message)))]
}

fn build_trials(cases: Vec<Case>, manifest: &XfailManifest) -> Vec<Trial> {
    let mut expected_failures: BTreeSet<CaseKey> = manifest_keys(manifest);
    let mut trials: Vec<Trial> = Vec::with_capacity(cases.len() + 1);
    let mut seen: BTreeSet<CaseKey> = BTreeSet::new();
    let mut duplicates: Vec<String> = Vec::new();
    for case in cases {
        if !seen.insert(case.key.clone()) {
            duplicates.push(case.key.trial_name());
        }
        let xfail: bool = expected_failures.remove(&case.key);
        let name: String = case.key.trial_name();
        let trial: Trial = match (xfail, case.outcome) {
            (false, Outcome::Passed) | (true, Outcome::Failed(_)) => Trial::test(name, || Ok(())),
            (false, Outcome::Failed(reason)) => Trial::test(name, move || {
                Err(Failed::from(format!(
                    "unexpected failure: {reason}. Fix it, or record a known gap in \
                     json_schema_rs/tests/suite_xfail.json (make update_suite_xfail)."
                )))
            }),
            (true, Outcome::Passed) => Trial::test(name, || {
                Err(Failed::from(
                    "unexpected pass: remove this test from json_schema_rs/tests/suite_xfail.json \
                     (make update_suite_xfail)."
                        .to_string(),
                ))
            }),
        };
        trials.push(if xfail {
            trial.with_kind("xfail")
        } else {
            trial
        });
    }
    let stale: Vec<String> = expected_failures.iter().map(CaseKey::trial_name).collect();
    trials.push(Trial::test(
        "suite_xfail.json lists only suite tests",
        move || {
            if stale.is_empty() {
                Ok(())
            } else {
                Err(Failed::from(format!(
                    "suite_xfail.json names tests that are not in the suite:\n  {}",
                    stale.join("\n  ")
                )))
            }
        },
    ));
    trials.push(Trial::test("suite test keys are unique", move || {
        if duplicates.is_empty() {
            Ok(())
        } else {
            Err(Failed::from(format!(
                "suite tests share a draft/file/group/test key:\n  {}",
                duplicates.join("\n  ")
            )))
        }
    }));
    trials
}

fn main() {
    let args: Arguments = Arguments::from_args();
    let trials: Vec<Trial> = match evaluate_suite(&suite_root()) {
        Err(message) => setup_failure("json schema test suite is present", message),
        Ok(cases) if std::env::var_os(UPDATE_ENV).is_some() => match write_manifest(&cases) {
            Ok(count) => {
                eprintln!(
                    "wrote {} ({count} known failures of {} suite tests)",
                    xfail_path().display(),
                    cases.len()
                );
                let manifest: XfailManifest =
                    read_manifest().unwrap_or_else(|message| panic!("{message}"));
                build_trials(cases, &manifest)
            }
            Err(message) => setup_failure("suite_xfail.json is writable", message),
        },
        Ok(cases) => match read_manifest() {
            Ok(manifest) => build_trials(cases, &manifest),
            Err(message) => setup_failure("suite_xfail.json is readable", message),
        },
    };
    libtest_mimic::run(&args, trials).exit();
}
