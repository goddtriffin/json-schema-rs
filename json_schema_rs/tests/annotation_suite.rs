//! Gating test: run [`CompiledSchema::annotations`] against the official JSON Schema
//! annotation test suite (`annotations/tests/` of the JSON Schema Test Suite).
//!
//! Every test case runs once for each draft its `compatibility` admits (4, 6, 7, 2019-09 and
//! 2020-12; the suite numbers them `4`, `6`, `7`, `2019` and `2020`), with that draft as the
//! default draft. Each test is one reported test case, named
//! `<draft>/<file> :: <case> :: <instance>`; it passes when every assertion holds: the
//! annotations the instance collects at the assertion's `location` for its `keyword`, keyed by
//! the schema location that contributed each one, equal `expected` (an empty `expected` means
//! none). A schema location is the subschema's location: the keyword's absolute location when a
//! reference led to it, otherwise its location as evaluated, without the keyword itself. The
//! suite lists them in an order the specification does not define, so they compare as maps.
//!
//! The suite must be cloned at `research/json-schema-test-suite/` (`make vendor_test_suite`);
//! the run hard-fails when it is missing.

use json_schema_rs::{
    Annotation, CompiledSchema, JsonSchemaSettings, Registry, Schema, SpecVersion,
};
use libtest_mimic::{Arguments, Failed, Trial};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// The drafts we run, with the release number the suite's `compatibility` uses for each.
const DRAFTS: [(&str, u32, SpecVersion); 5] = [
    ("draft4", 4, SpecVersion::Draft04),
    ("draft6", 6, SpecVersion::Draft06),
    ("draft7", 7, SpecVersion::Draft07),
    ("draft2019-09", 2019, SpecVersion::Draft201909),
    ("draft2020-12", 2020, SpecVersion::Draft202012),
];

/// The retrieval URI of each case's schema.
const BASE_URI: &str = "https://json-schema.org/annotation-suite/schema.json";

#[derive(Debug, Deserialize)]
struct SuiteFile {
    suite: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestCase {
    description: String,
    compatibility: Option<String>,
    schema: Value,
    #[serde(default)]
    external_schemas: HashMap<String, Value>,
    tests: Vec<SuiteTest>,
}

#[derive(Debug, Deserialize)]
struct SuiteTest {
    instance: Value,
    assertions: Vec<Assertion>,
}

#[derive(Debug, Deserialize)]
struct Assertion {
    location: String,
    keyword: String,
    expected: BTreeMap<String, Value>,
}

fn suite_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("research")
        .join("json-schema-test-suite")
        .join("annotations")
        .join("tests")
}

/// Whether `compatibility` (comma-separated `N`, `<=N` or `=N`) admits release `release`.
fn compatible(compatibility: Option<&str>, release: u32) -> Result<bool, String> {
    let Some(compatibility) = compatibility else {
        return Ok(true);
    };
    let mut admits: bool = true;
    for constraint in compatibility.split(',').map(str::trim) {
        let (number, test): (&str, fn(u32, u32) -> bool) =
            if let Some(number) = constraint.strip_prefix("<=") {
                (number, |release, bound| release <= bound)
            } else if let Some(number) = constraint.strip_prefix('=') {
                (number, |release, bound| release == bound)
            } else {
                (constraint, |release, bound| release >= bound)
            };
        let bound: u32 = number
            .parse()
            .map_err(|e| format!("bad compatibility {compatibility:?}: {e}"))?;
        admits &= test(release, bound);
    }
    Ok(admits)
}

/// `text` with every `%XX` escape decoded.
fn percent_decode(text: &str) -> String {
    let bytes: &[u8] = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i: usize = 0;
    while i < bytes.len() {
        let escape: Option<u8> = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        if let Some(byte) = escape {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The schema location that contributed `annotation`, as a decoded JSON Pointer.
fn schema_location(annotation: &Annotation) -> String {
    let keyword_location: String = match &annotation.absolute_keyword_location {
        Some(absolute) => percent_decode(absolute.split_once('#').map_or("", |(_, f)| f)),
        None => annotation.keyword_location.to_string(),
    };
    let suffix: String = format!(
        "/{}",
        annotation.keyword.replace('~', "~0").replace('/', "~1")
    );
    keyword_location
        .strip_suffix(suffix.as_str())
        .unwrap_or(&keyword_location)
        .to_string()
}

/// Runs one test: compiles the schema for `spec_version` and checks every assertion.
fn run_test(case: &TestCase, spec_version: SpecVersion, test: &SuiteTest) -> Result<(), String> {
    let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .default_spec_version(spec_version)
        .build();
    let schema: Schema = Schema::new_from_serde_value(&case.schema, &settings)
        .map_err(|e| format!("schema not supported: {}", e.first()))?;
    // A base URI, so a keyword a reference led to has an absolute location to key it by.
    let registry: Registry<'_> = Registry::builder()
        .base_uri(BASE_URI.to_string())
        .retriever(&case.external_schemas)
        .settings(settings)
        .build(&schema)
        .map_err(|e| format!("registry failed: {}", e.first()))?;
    let compiled: CompiledSchema = CompiledSchema::builder()
        .annotations(true)
        .build(&registry)
        .map_err(|e| format!("compile failed: {}", e.first()))?;
    let annotations: Vec<Annotation> = compiled.annotations(&test.instance).map_err(|errors| {
        format!(
            "instance is invalid: {}",
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>()
                .join("; ")
        )
    })?;
    let mut failures: Vec<String> = Vec::new();
    for assertion in &test.assertions {
        let expected: BTreeMap<String, Value> = assertion
            .expected
            .iter()
            .map(|(location, value)| {
                let pointer: &str = location.strip_prefix('#').unwrap_or(location);
                (percent_decode(pointer), value.clone())
            })
            .collect();
        let actual: BTreeMap<String, Value> = annotations
            .iter()
            .filter(|a| {
                a.keyword == assertion.keyword
                    && a.instance_location.to_string() == assertion.location
            })
            .map(|a| (schema_location(a), a.value.clone()))
            .collect();
        if expected != actual {
            failures.push(format!(
                "at {:?}, keyword {:?}: expected {expected:?}, got {actual:?}",
                assertion.location, assertion.keyword
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// One trial per test and admitted draft.
fn trials(dir: &Path) -> Result<Vec<Trial>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| {
            format!(
                "annotation suite not found at {}: {e}. Run `make vendor_test_suite` to clone it.",
                dir.display()
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("no annotation suite files under {}", dir.display()));
    }
    let mut trials: Vec<Trial> = Vec::new();
    for path in files {
        let file: String = path
            .file_name()
            .map_or_default(|n| n.to_string_lossy().into_owned());
        let contents: String = std::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let suite: SuiteFile = serde_json::from_str(&contents)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        let cases: Vec<std::sync::Arc<TestCase>> =
            suite.suite.into_iter().map(std::sync::Arc::new).collect();
        for (draft, release, spec_version) in DRAFTS {
            for case in &cases {
                if !compatible(case.compatibility.as_deref(), release)? {
                    continue;
                }
                for index in 0..case.tests.len() {
                    let case: std::sync::Arc<TestCase> = std::sync::Arc::clone(case);
                    let name: String = format!(
                        "{draft}/{file} :: {} :: {}",
                        case.description, case.tests[index].instance
                    );
                    trials.push(Trial::test(name, move || {
                        run_test(&case, spec_version, &case.tests[index]).map_err(Failed::from)
                    }));
                }
            }
        }
    }
    Ok(trials)
}

fn main() {
    let args: Arguments = Arguments::from_args();
    let trials: Vec<Trial> = trials(&suite_dir()).unwrap_or_else(|message| {
        vec![Trial::test("annotation suite is present", move || {
            Err(Failed::from(message))
        })]
    });
    libtest_mimic::run(&args, trials).exit();
}
