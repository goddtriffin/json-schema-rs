//! Gating test: run the library validator against the official JSON Schema Test Suite.
//!
//! The suite keeps every draft's cases together in `validation/tests/`; a case's
//! `compatibility` says which drafts it applies to. Every suite test (including `optional/`,
//! leaving out the unaccepted `proposals/`) is one reported test case per draft 4, 6, 7, 2019-09
//! and 2020-12 it applies to, named `<draft>/<file> :: <group> :: <test>`. Known failures are listed
//! in `tests/suite_xfail.json` (draft → file → group → tests); a build with the
//! `arbitrary-precision` feature, whose numbers are exact beyond 64 bits, reads
//! `tests/suite_xfail_arbitrary_precision.json` instead. The run fails on:
//!
//! - an **unexpected failure**: a test not in the manifest returns the wrong verdict, or its
//!   schema cannot be parsed;
//! - an **unexpected pass**: a test in the manifest now passes, so its entry must be removed;
//! - a **stale entry**: the manifest names a test that does not exist in the suite.
//!
//! Each group's schema gets a [`Registry`] whose retriever serves the group's own
//! `externalSchemas` under their URIs, as the suite specifies. The draft being run is the default
//! draft: a schema without `$schema` uses it, one with `$schema` follows that (so a custom
//! metaschema is found through the registry). A group tagged `format-assertion` or
//! `format-annotation` runs with that `format` behaviour ([`FormatMode`]); any other tag fails
//! the run, so a new tag is handled deliberately. The group's schema is compiled once and every
//! test runs in both modes, [`CompiledSchema::validate`] and [`CompiledSchema::is_valid`]; a
//! case whose two verdicts differ fails, whatever the manifest says. The group's schema is also
//! dereferenced ([`Registry::dereference`]) and the result compiled: a case whose verdict on the
//! dereferenced schema differs fails too, whatever the manifest says. A schema that cannot be
//! dereferenced (one reaching another dialect) skips that check. Each schema is also
//! canonicalized ([`Registry::canonicalize`]): the canonical form must canonicalize to itself
//! and give every case the same verdict.
//!
//! The suite must be cloned at `research/json-schema-test-suite/` (`make vendor_test_suite`);
//! the run hard-fails when it is missing. `make update_suite_xfail` rewrites both manifests from
//! the current results (sets `JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE=1`, once per build).

use json_schema_rs::{
    CompiledSchema, DereferenceError, FormatMode, JsonSchemaSettings, Registry, Schema,
    SpecVersion, ValidateError, ValidationError, validate,
};
use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

mod suite_compatibility;
use suite_compatibility::compatible;

/// The drafts we run: the name in test names and the manifest, the release number the suite's
/// `compatibility` uses for it, and its spec version.
const DRAFTS: [(&str, u32, SpecVersion); 5] = [
    ("draft4", 4, SpecVersion::Draft04),
    ("draft6", 6, SpecVersion::Draft06),
    ("draft7", 7, SpecVersion::Draft07),
    ("draft2019-09", 2019, SpecVersion::Draft201909),
    ("draft2020-12", 2020, SpecVersion::Draft202012),
];

/// The suite directory of draft proposals not (yet) in any draft; left out.
const PROPOSALS_DIR: &str = "proposals";

/// Environment variable that switches the run to rewriting the manifest.
const UPDATE_ENV: &str = "JSON_SCHEMA_RS_SUITE_XFAIL_UPDATE";

/// Known failures: draft → file (relative to `validation/tests/`) → group → test descriptions.
type XfailManifest = BTreeMap<String, BTreeMap<String, BTreeMap<String, BTreeSet<String>>>>;

#[derive(Debug, Deserialize)]
struct SuiteFile {
    tests: Vec<SuiteGroup>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuiteGroup {
    description: String,
    compatibility: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    schema: serde_json::Value,
    #[serde(default)]
    external_schemas: HashMap<String, serde_json::Value>,
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
    /// `is_valid` and `validate` gave different verdicts: a failure the manifest cannot excuse.
    Mismatch(String),
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

/// The known-failure manifest of this build: numbers beyond 64 bits are exact only with the
/// `arbitrary-precision` feature, so the two builds fail different suite tests.
#[cfg(not(feature = "arbitrary-precision"))]
const XFAIL_FILE: &str = "suite_xfail.json";
#[cfg(feature = "arbitrary-precision")]
const XFAIL_FILE: &str = "suite_xfail_arbitrary_precision.json";

fn xfail_path() -> PathBuf {
    manifest_dir().join("tests").join(XFAIL_FILE)
}

/// Collects all `.json` files under `dir` (iteratively, leaving out [`PROPOSALS_DIR`]), sorted
/// by path.
fn collect_json_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in std::fs::read_dir(&current)? {
            let path: PathBuf = entry?.path();
            if path.is_dir() {
                if path.file_name().is_none_or(|name| name != PROPOSALS_DIR) {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Relative path of `file` under `dir`, with `/` separators on every platform.
fn relative_file_name(dir: &Path, file: &Path) -> String {
    let relative: &Path = file.strip_prefix(dir).unwrap_or(file);
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

/// Runs one suite test; a panic in our library is a failure, not a crash of the run. A schema
/// that could not be compiled (or whose registry could not be built) is validated with
/// [`validate`], whose schema error fails every instance and says why.
fn run_test(schema: &Schema, compiled: Option<&CompiledSchema>, test: &SuiteTest) -> Outcome {
    let result = catch_unwind(AssertUnwindSafe(|| match compiled {
        Some(compiled) => {
            let result: Result<(), String> = compiled
                .validate(&test.data)
                .map_err(|errors: Vec<ValidationError>| describe_errors(&errors));
            (result.is_ok() == compiled.is_valid(&test.data)).then_some(result)
        }
        None => Some(
            validate(schema, &test.data).map_err(|error: ValidateError| match error {
                ValidateError::Instance(errors) => describe_errors(&errors),
                ValidateError::Schema(error) => format!("the schema cannot be used: {error}"),
            }),
        ),
    }));
    let result = match result {
        Ok(Some(result)) => Ok(result),
        Ok(None) => {
            return Outcome::Mismatch("is_valid and validate disagree".to_string());
        }
        Err(panic) => Err(panic),
    };
    match (result, test.valid) {
        (Err(_), _) => Outcome::Failed("validator panicked".to_string()),
        (Ok(Ok(())), true) | (Ok(Err(_)), false) => Outcome::Passed,
        (Ok(Ok(())), false) => Outcome::Failed("expected invalid, got valid".to_string()),
        (Ok(Err(description)), true) => {
            Outcome::Failed(format!("expected valid, got invalid: {description}"))
        }
    }
}

/// A group's schema after [`Registry::dereference`].
enum Dereferenced {
    /// The dereferenced schema, compiled.
    Compiled(Box<CompiledSchema>),
    /// The schema reaches another dialect, which dereferencing reports instead of inlining.
    Skipped,
    /// Dereferencing failed otherwise, or its output is not a usable schema.
    Unusable(String),
}

/// Dereferences the schema of `registry` and compiles the result with the same remotes and
/// settings.
fn dereference(
    registry: &Registry<'_>,
    remotes: &HashMap<String, serde_json::Value>,
    settings: &JsonSchemaSettings,
) -> Dereferenced {
    let value: serde_json::Value = match registry.dereference() {
        Ok(value) => value,
        Err(DereferenceError::MixedDialects { .. }) => return Dereferenced::Skipped,
        Err(error) => return Dereferenced::Unusable(format!("dereference failed: {error}")),
    };
    let compiled: Result<CompiledSchema, String> = Schema::new_from_serde_value(&value, settings)
        .map_err(|e| e.to_string())
        .and_then(|schema: Schema| {
            let registry: Registry<'_> = Registry::builder()
                .retriever(remotes)
                .settings(settings.clone())
                .build(&schema)
                .map_err(|e| e.to_string())?;
            CompiledSchema::new(&registry).map_err(|e| e.to_string())
        });
    match compiled {
        Ok(compiled) => Dereferenced::Compiled(Box::new(compiled)),
        Err(error) => {
            Dereferenced::Unusable(format!("the dereferenced schema is unusable: {error}"))
        }
    }
}

/// Canonicalizes the schema of `registry` ([`Registry::canonicalize`]), checks that the
/// canonical form canonicalizes to itself, and compiles it with the same remotes and settings.
fn canonicalize(
    registry: &Registry<'_>,
    remotes: &HashMap<String, serde_json::Value>,
    settings: &JsonSchemaSettings,
) -> Result<CompiledSchema, String> {
    let value: serde_json::Value = registry
        .canonicalize()
        .map_err(|e| format!("canonicalize failed: {e}"))?;
    let schema: Schema = Schema::new_from_serde_value(&value, settings)
        .map_err(|e| format!("the canonical schema is unusable: {e}"))?;
    let canonical: Registry<'_> = Registry::builder()
        .retriever(remotes)
        .settings(settings.clone())
        .build(&schema)
        .map_err(|e| format!("the canonical schema is unusable: {e}"))?;
    let again: serde_json::Value = canonical
        .canonicalize()
        .map_err(|e| format!("canonicalize failed: {e}"))?;
    if again != value {
        return Err(format!(
            "canonicalizing the canonical schema changed it: {value} became {again}"
        ));
    }
    CompiledSchema::new(&canonical).map_err(|e| format!("the canonical schema is unusable: {e}"))
}

/// Why the dereferenced or canonical schema disagrees with `compiled` on `data`, if it does.
fn transformed_verdicts(
    compiled: &CompiledSchema,
    dereferenced: &Dereferenced,
    canonical: &Result<CompiledSchema, String>,
    data: &serde_json::Value,
) -> Option<String> {
    let verdict: bool = compiled.is_valid(data);
    match dereferenced {
        Dereferenced::Compiled(dereferenced) if dereferenced.is_valid(data) != verdict => {
            return Some("the dereferenced schema gives another verdict".to_string());
        }
        Dereferenced::Unusable(reason) => return Some(reason.clone()),
        Dereferenced::Compiled(_) | Dereferenced::Skipped => {}
    }
    match canonical {
        Ok(canonical) if canonical.is_valid(data) != verdict => {
            Some("the canonical schema gives another verdict".to_string())
        }
        Ok(_) => None,
        Err(reason) => Some(reason.clone()),
    }
}

/// The `format` behaviour a group's `tags` ask for: `None` for the draft's default.
///
/// # Errors
///
/// A tag this runner does not know, so a new one is handled deliberately.
fn tagged_format_mode(tags: &[String]) -> Result<Option<FormatMode>, String> {
    let mut mode: Option<FormatMode> = None;
    for tag in tags {
        mode = Some(match tag.as_str() {
            "format-assertion" => FormatMode::Assertion,
            "format-annotation" => FormatMode::Annotation,
            other => return Err(format!("unknown suite tag {other:?}")),
        });
    }
    Ok(mode)
}

/// Evaluates every test in every draft. Errors are reasons the suite itself is unusable.
fn evaluate_suite(root: &Path) -> Result<Vec<Case>, String> {
    let tests_dir: PathBuf = root.join("validation").join("tests");
    if !tests_dir.is_dir() {
        return Err(format!(
            "JSON Schema Test Suite not found at {}. Run `make vendor_test_suite` to clone it.",
            tests_dir.display()
        ));
    }
    let files: Vec<PathBuf> = collect_json_files(&tests_dir)
        .map_err(|e| format!("failed to walk {}: {e}", tests_dir.display()))?;
    if files.is_empty() {
        return Err(format!("no suite files under {}", tests_dir.display()));
    }
    let mut suite: Vec<(String, Vec<SuiteGroup>)> = Vec::with_capacity(files.len());
    for file_path in &files {
        let contents: String = std::fs::read_to_string(file_path)
            .map_err(|e| format!("failed to read {}: {e}", file_path.display()))?;
        let file: SuiteFile = serde_json::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", file_path.display()))?;
        suite.push((relative_file_name(&tests_dir, file_path), file.tests));
    }
    let mut cases: Vec<Case> = Vec::new();
    for (draft, release, spec_version) in DRAFTS {
        // Default settings, as consumers run: the suite uses keys a draft does not define
        // (`$comment` in draft 4) to check that they are ignored, which strict parsing rejects.
        let draft_settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .default_spec_version(spec_version)
            .build();
        for (file, groups) in &suite {
            for group in groups {
                if !compatible(group.compatibility.as_deref(), release)
                    .map_err(|e| format!("{file}: {}: {e}", group.description))?
                {
                    continue;
                }
                let settings: &JsonSchemaSettings = &JsonSchemaSettings {
                    format_mode: tagged_format_mode(&group.tags)
                        .map_err(|e| format!("{file}: {}: {e}", group.description))?
                        .or(draft_settings.format_mode),
                    ..draft_settings.clone()
                };
                let remotes: &HashMap<String, serde_json::Value> = &group.external_schemas;
                let parsed = catch_unwind(AssertUnwindSafe(|| {
                    Schema::new_from_serde_value(&group.schema, settings)
                }));
                let schema: Result<Schema, String> = match parsed {
                    Ok(Ok(schema)) => Ok(schema),
                    Ok(Err(e)) => Err(format!("schema not supported: {e}")),
                    Err(_) => Err("schema parser panicked".to_string()),
                };
                let compiled: Option<(
                    CompiledSchema,
                    Dereferenced,
                    Result<CompiledSchema, String>,
                )> = schema.as_ref().ok().and_then(|schema| {
                    catch_unwind(AssertUnwindSafe(|| {
                        let registry: Registry<'_> = Registry::builder()
                            .retriever(remotes)
                            .settings(settings.clone())
                            .build(schema)
                            .ok()?;
                        let compiled: CompiledSchema = CompiledSchema::new(&registry).ok()?;
                        Some((
                            compiled,
                            dereference(&registry, remotes, settings),
                            canonicalize(&registry, remotes, settings),
                        ))
                    }))
                    .ok()
                    .flatten()
                });
                for test in &group.tests {
                    let outcome: Outcome = match (&schema, &compiled) {
                        (Ok(schema), None) => run_test(schema, None, test),
                        (Ok(schema), Some((compiled, dereferenced, canonical))) => {
                            match run_test(schema, Some(compiled), test) {
                                Outcome::Mismatch(reason) => Outcome::Mismatch(reason),
                                outcome => transformed_verdicts(
                                    compiled,
                                    dereferenced,
                                    canonical,
                                    &test.data,
                                )
                                .map_or(outcome, Outcome::Mismatch),
                            }
                        }
                        (Err(reason), _) => Outcome::Failed(reason.clone()),
                    };
                    cases.push(Case {
                        key: CaseKey {
                            draft: draft.to_string(),
                            file: file.clone(),
                            group: group.description.clone(),
                            test: test.description.clone(),
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
            (_, Outcome::Mismatch(reason)) => Trial::test(name, move || {
                Err(Failed::from(format!("mode mismatch: {reason}")))
            }),
            (false, Outcome::Passed) | (true, Outcome::Failed(_)) => Trial::test(name, || Ok(())),
            (false, Outcome::Failed(reason)) => Trial::test(name, move || {
                Err(Failed::from(format!(
                    "unexpected failure: {reason}. Fix it, or record a known gap in \
                     json_schema_rs/tests/{XFAIL_FILE} (make update_suite_xfail)."
                )))
            }),
            (true, Outcome::Passed) => Trial::test(name, || {
                Err(Failed::from(format!(
                    "unexpected pass: remove this test from json_schema_rs/tests/{XFAIL_FILE} \
                     (make update_suite_xfail)."
                )))
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
        format!("{XFAIL_FILE} lists only suite tests"),
        move || {
            if stale.is_empty() {
                Ok(())
            } else {
                Err(Failed::from(format!(
                    "{XFAIL_FILE} names tests that are not in the suite:\n  {}",
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
            Err(message) => setup_failure(&format!("{XFAIL_FILE} is writable"), message),
        },
        Ok(cases) => match read_manifest() {
            Ok(manifest) => build_trials(cases, &manifest),
            Err(message) => setup_failure(&format!("{XFAIL_FILE} is readable"), message),
        },
    };
    libtest_mimic::run(&args, trials).exit();
}
