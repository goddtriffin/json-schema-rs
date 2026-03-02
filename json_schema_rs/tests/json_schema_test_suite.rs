//! Integration test: run the library validator against the official JSON Schema Test Suite.
//!
//! Requires the suite to be cloned at `research/json-schema-test-suite/` (run
//! `make vendor_test_suite` first). This test is ignored by default; run with
//! `make test_json_schema_suite` or `cargo test --test json_schema_test_suite -- --ignored`.

use json_schema_rs::{JsonSchema, JsonSchemaSettings, validate};
use serde::Deserialize;
use std::path::PathBuf;

/// Path to the JSON Schema Test Suite root (research/json-schema-test-suite).
fn suite_root() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR set by Cargo");
    PathBuf::from(&manifest_dir)
        .join("..")
        .join("research")
        .join("json-schema-test-suite")
}

#[derive(Debug, Deserialize)]
struct TestCase {
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

/// Per-category failure counts for the test suite run.
#[derive(Debug, Default, PartialEq, Eq)]
struct SuiteFailureCounts {
    /// Number of files we failed to read.
    read_file: u64,
    /// Number of files we failed to parse into test cases.
    parse: u64,
    /// Number of cases where we failed to construct a `JsonSchema`.
    construct_schema: u64,
    /// Number of tests where the validation result was wrong.
    validation_result: u64,
}

impl SuiteFailureCounts {
    fn total(&self) -> u64 {
        self.read_file + self.parse + self.construct_schema + self.validation_result
    }
}

/// Recursively collect all `.json` files under `dir`.
fn collect_json_files(dir: &std::path::Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            files.push(path);
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires research/json-schema-test-suite; run make vendor_test_suite then make test_json_schema_suite"]
fn json_schema_test_suite() {
    let root = suite_root();
    let tests_dir = root.join("tests");

    assert!(
        root.exists() && root.is_dir(),
        "JSON Schema Test Suite not found at {}. Run `make vendor_test_suite` to clone it.",
        root.display()
    );
    assert!(
        tests_dir.exists() && tests_dir.is_dir(),
        "JSON Schema Test Suite tests directory not found at {}. Run `make vendor_test_suite` to clone it.",
        tests_dir.display()
    );

    let mut json_files: Vec<PathBuf> = Vec::new();
    collect_json_files(&tests_dir, &mut json_files)
        .unwrap_or_else(|e| panic!("failed to walk {}: {}", tests_dir.display(), e));

    let mut passed: u64 = 0;
    let mut counts = SuiteFailureCounts::default();
    let mut failed_details: Vec<(PathBuf, String, String)> = Vec::new();
    let schema_settings = JsonSchemaSettings::builder()
        .disallow_unknown_fields(true)
        .build();

    for file_path in &json_files {
        let contents = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("failed to read {}: {}", file_path.display(), e);
                counts.read_file += 1;
                continue;
            }
        };
        let cases: Vec<TestCase> = match serde_json::from_str(&contents) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("failed to parse {}: {}", file_path.display(), e);
                counts.parse += 1;
                continue;
            }
        };
        for case in cases {
            let schema: JsonSchema =
                if let Ok(s) = JsonSchema::new_from_serde_value(&case.schema, &schema_settings) {
                    s
                } else {
                    counts.construct_schema += 1;
                    continue;
                };
            for t in case.tests {
                let actual_valid = validate(&schema, &t.data).is_ok();
                if actual_valid == t.valid {
                    passed += 1;
                } else {
                    counts.validation_result += 1;
                    let rel_path = file_path
                        .strip_prefix(&root)
                        .unwrap_or(file_path)
                        .display()
                        .to_string();
                    failed_details.push((rel_path.into(), case.description.clone(), t.description));
                }
            }
        }
    }

    let total_failures = counts.total();
    let total = passed + total_failures;
    eprintln!(
        "JSON Schema Test Suite: passed: {passed}, failed: {total_failures}, total: {total} \
         (read_file: {}, parse: {}, construct_schema: {}, validation_result: {})",
        counts.read_file, counts.parse, counts.construct_schema, counts.validation_result
    );
    if !failed_details.is_empty() && failed_details.len() <= 50 {
        for (path, case_desc, test_desc) in &failed_details {
            eprintln!("  {} | {} | {}", path.display(), case_desc, test_desc);
        }
    } else if failed_details.len() > 50 {
        eprintln!("  ({} failed tests omitted)", failed_details.len());
    }

    let expected = SuiteFailureCounts {
        read_file: 0,
        parse: 0,
        construct_schema: 0,
        validation_result: 0,
    };
    assert_eq!(
        counts, expected,
        "JSON Schema Test Suite failure breakdown (passed: {passed}, total: {total})"
    );
}
