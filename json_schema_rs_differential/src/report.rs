//! Parsing of Bowtie's line-delimited JSON report format.
//!
//! A report produced by `bowtie suite`, `bowtie run` or `bowtie validate` is a sequence of JSON lines
//! (see Bowtie's `bowtie/schemas/report.json` and `bowtie/_commands.py`):
//!
//! - exactly one **header** first: `{"implementations": {...}, "dialect": ..., "bowtie_version": ..., "started": ...}`;
//! - **case** lines: `{"seq": S, "case": {"description", "schema", "tests": [{"description", "instance", "valid"?}]}}`;
//! - **result** lines: `{"seq": S, "implementation": ID, "expected": [bool | null | [...]], ...}` followed by
//!   one of three shapes:
//!   - `"results": [...]` — one entry per test: `{"valid": bool}` (optionally with `annotations`),
//!     `{"skipped": true, "message"?, "issue_url"?}`, or `{"errored": true, "context": {...}}`;
//!   - `"skipped": true, "message"?, "issue_url"?` — the whole case was skipped;
//!   - `"errored": true, "context": {...}, "message"?, "caught"?` — the whole case errored (with
//!     `caught: false` when Bowtie itself had to rescue the harness from a crash);
//! - a **footer** last: `{"did_fail_fast": bool}`.
//!
//! Every per-case outcome is normalized to one [`Verdict`] per test so whole-case skips and
//! errors expand to every test in the case. Tests an implementation never reported on (for
//! example after a fail-fast stop) are recorded as errored so they are never silently dropped.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;

/// Maximum number of characters kept from an error context when summarizing it.
const ERROR_SUMMARY_MAX_CHARS: usize = 300;

/// A malformed line or structural inconsistency in a Bowtie report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportError {
    /// 1-based line number the problem was detected on (0 when it concerns the whole report).
    pub line: usize,
    /// Human-readable description of the problem.
    pub message: String,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for ReportError {}

/// Metadata Bowtie records about one participating implementation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplementationInfo {
    /// Implementation name (e.g. `jsonschema`).
    pub name: String,
    /// Implementation language (e.g. `rust`).
    pub language: String,
    /// Implementation version, when the harness reports one.
    pub version: Option<String>,
}

/// One test (instance) of a test case.
#[derive(Debug, Clone, PartialEq)]
pub struct CaseTest {
    /// The test description (empty for `bowtie validate` reports).
    pub description: String,
    /// The instance being validated.
    pub instance: Value,
    /// The expected validity, when known (from the case or the result line's `expected`).
    pub expected: Option<bool>,
}

/// One test case (a schema plus its tests).
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// Bowtie's sequence identifier for the case, rendered as a string.
    pub seq: String,
    /// The case description.
    pub description: String,
    /// The case schema.
    pub schema: Value,
    /// The tests of the case, in report order.
    pub tests: Vec<CaseTest>,
}

/// The normalized outcome of one implementation on one test.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    /// The implementation considered the instance valid.
    Valid,
    /// The implementation considered the instance invalid.
    Invalid,
    /// The implementation skipped the test (for us: an unsupported feature).
    Skipped {
        /// The skip message (empty when the harness gave none).
        message: String,
    },
    /// The implementation (or its harness) errored on the test.
    Errored {
        /// A one-line summary of the error context.
        message: String,
    },
}

impl Verdict {
    /// The real validity verdict, or `None` for skipped and errored outcomes.
    #[must_use]
    pub fn validity(&self) -> Option<bool> {
        match self {
            Verdict::Valid => Some(true),
            Verdict::Invalid => Some(false),
            Verdict::Skipped { .. } | Verdict::Errored { .. } => None,
        }
    }
}

/// A fully parsed Bowtie report.
#[derive(Debug, Clone, PartialEq)]
pub struct BowtieReport {
    /// The dialect URI the run targeted.
    pub dialect: String,
    /// Metadata for every implementation in the run, by implementation id.
    pub implementations: BTreeMap<String, ImplementationInfo>,
    /// Every case, in report order.
    pub cases: Vec<Case>,
    /// Per implementation id, per case seq: one verdict per test of the case.
    pub verdicts: BTreeMap<String, BTreeMap<String, Vec<Verdict>>>,
    /// Whether Bowtie stopped the run early because of `--fail-fast`/`--max-fail`/`--max-error`.
    pub did_fail_fast: bool,
}

/// Parse a complete Bowtie report from its JSONL text.
///
/// # Errors
///
/// Returns a [`ReportError`] when the text is empty, a line is not valid JSON or matches no
/// known line shape, a case or result is duplicated, a result references an unknown case or
/// implementation, or the footer is missing.
pub fn parse_report(text: &str) -> Result<BowtieReport, ReportError> {
    let mut header: Option<(String, BTreeMap<String, ImplementationInfo>)> = None;
    let mut cases: Vec<Case> = Vec::new();
    let mut case_index: BTreeMap<String, usize> = BTreeMap::new();
    let mut raw_results: BTreeMap<String, BTreeMap<String, RawResult>> = BTreeMap::new();
    let mut did_fail_fast: Option<bool> = None;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number: usize = index + 1;
        let trimmed: &str = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if did_fail_fast.is_some() {
            return Err(line_error(line_number, "content after the report footer"));
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|e: serde_json::Error| {
            line_error(line_number, &format!("invalid JSON: {e}"))
        })?;
        let Value::Object(object) = value else {
            return Err(line_error(line_number, "line is not a JSON object"));
        };
        if header.is_none() {
            header = Some(parse_header(&object).map_err(|m: String| line_error(line_number, &m))?);
            continue;
        }
        if let Some(case_value) = object.get("case") {
            let case: Case =
                parse_case(&object, case_value).map_err(|m: String| line_error(line_number, &m))?;
            if case_index.contains_key(&case.seq) {
                return Err(line_error(
                    line_number,
                    &format!("duplicate case seq {}", case.seq),
                ));
            }
            case_index.insert(case.seq.clone(), cases.len());
            cases.push(case);
        } else if let Some(flag) = object.get("did_fail_fast") {
            let flag: bool = flag
                .as_bool()
                .ok_or_else(|| line_error(line_number, "did_fail_fast is not a boolean"))?;
            did_fail_fast = Some(flag);
        } else if object.contains_key("implementation") {
            let raw: RawResult =
                parse_result(&object).map_err(|m: String| line_error(line_number, &m))?;
            let Some(position) = case_index.get(&raw.seq).copied() else {
                return Err(line_error(
                    line_number,
                    &format!("result for unknown case seq {}", raw.seq),
                ));
            };
            fill_expected(&mut cases[position], &raw.expected);
            let per_case: &mut BTreeMap<String, RawResult> =
                raw_results.entry(raw.implementation.clone()).or_default();
            if per_case.contains_key(&raw.seq) {
                return Err(line_error(
                    line_number,
                    &format!(
                        "duplicate result for implementation {} and case seq {}",
                        raw.implementation, raw.seq
                    ),
                ));
            }
            per_case.insert(raw.seq.clone(), raw);
        } else {
            return Err(line_error(
                line_number,
                "line matches no known Bowtie report shape",
            ));
        }
    }

    let Some((dialect, implementations)) = header else {
        return Err(line_error(0, "empty report (no header line)"));
    };
    let Some(did_fail_fast) = did_fail_fast else {
        return Err(line_error(
            0,
            "missing report footer (did_fail_fast); the Bowtie run did not finish",
        ));
    };
    for implementation in raw_results.keys() {
        if !implementations.contains_key(implementation) {
            return Err(line_error(
                0,
                &format!("results for implementation {implementation} missing from the header"),
            ));
        }
    }
    let verdicts: BTreeMap<String, BTreeMap<String, Vec<Verdict>>> =
        expand_verdicts(&implementations, &cases, &raw_results);
    Ok(BowtieReport {
        dialect,
        implementations,
        cases,
        verdicts,
        did_fail_fast,
    })
}

/// A result line before expansion to per-test verdicts.
#[derive(Debug, Clone, PartialEq)]
struct RawResult {
    seq: String,
    implementation: String,
    expected: Vec<Option<bool>>,
    outcome: RawOutcome,
}

/// The three shapes a result line can take.
#[derive(Debug, Clone, PartialEq)]
enum RawOutcome {
    PerTest(Vec<Verdict>),
    CaseSkipped(String),
    CaseErrored(String),
}

fn line_error(line: usize, message: &str) -> ReportError {
    ReportError {
        line,
        message: message.to_string(),
    }
}

fn seq_key(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn string_field(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_string)
}

fn parse_header(
    object: &Map<String, Value>,
) -> Result<(String, BTreeMap<String, ImplementationInfo>), String> {
    let Some(Value::Object(raw_implementations)) = object.get("implementations") else {
        return Err(
            "first line is not a report header (no \"implementations\" object)".to_string(),
        );
    };
    let dialect: String = string_field(object, "dialect")
        .ok_or_else(|| "report header has no \"dialect\" string".to_string())?;
    let mut implementations: BTreeMap<String, ImplementationInfo> = BTreeMap::new();
    for (id, info) in raw_implementations {
        let empty: Map<String, Value> = Map::new();
        let info: &Map<String, Value> = info.as_object().unwrap_or(&empty);
        implementations.insert(
            id.clone(),
            ImplementationInfo {
                name: string_field(info, "name").unwrap_or_else(|| id.clone()),
                language: string_field(info, "language").unwrap_or_default(),
                version: string_field(info, "version"),
            },
        );
    }
    Ok((dialect, implementations))
}

fn parse_case(object: &Map<String, Value>, case_value: &Value) -> Result<Case, String> {
    let seq: String = object
        .get("seq")
        .map(seq_key)
        .ok_or_else(|| "case line has no \"seq\"".to_string())?;
    let Value::Object(case) = case_value else {
        return Err("\"case\" is not an object".to_string());
    };
    let description: String = string_field(case, "description").unwrap_or_default();
    let schema: Value = case
        .get("schema")
        .cloned()
        .ok_or_else(|| format!("case {seq} has no \"schema\""))?;
    let Some(Value::Array(raw_tests)) = case.get("tests") else {
        return Err(format!("case {seq} has no \"tests\" array"));
    };
    let mut tests: Vec<CaseTest> = Vec::with_capacity(raw_tests.len());
    for raw_test in raw_tests {
        let Value::Object(test) = raw_test else {
            return Err(format!("case {seq} has a test that is not an object"));
        };
        tests.push(CaseTest {
            description: string_field(test, "description").unwrap_or_default(),
            instance: test.get("instance").cloned().unwrap_or(Value::Null),
            expected: test.get("valid").and_then(Value::as_bool),
        });
    }
    Ok(Case {
        seq,
        description,
        schema,
        tests,
    })
}

fn parse_result(object: &Map<String, Value>) -> Result<RawResult, String> {
    let seq: String = object
        .get("seq")
        .map(seq_key)
        .ok_or_else(|| "result line has no \"seq\"".to_string())?;
    let implementation: String = string_field(object, "implementation")
        .ok_or_else(|| "result line \"implementation\" is not a string".to_string())?;
    let expected: Vec<Option<bool>> = match object.get("expected") {
        Some(Value::Array(items)) => items.iter().map(Value::as_bool).collect(),
        _ => Vec::new(),
    };
    let outcome: RawOutcome = if object.get("errored").and_then(Value::as_bool) == Some(true) {
        RawOutcome::CaseErrored(case_error_message(object))
    } else if object.get("skipped").and_then(Value::as_bool) == Some(true) {
        RawOutcome::CaseSkipped(string_field(object, "message").unwrap_or_default())
    } else if let Some(Value::Array(results)) = object.get("results") {
        let mut verdicts: Vec<Verdict> = Vec::with_capacity(results.len());
        for result in results {
            verdicts.push(parse_test_result(result)?);
        }
        RawOutcome::PerTest(verdicts)
    } else {
        return Err(format!(
            "result for case seq {seq} has neither \"results\", \"skipped\" nor \"errored\""
        ));
    };
    Ok(RawResult {
        seq,
        implementation,
        expected,
        outcome,
    })
}

fn parse_test_result(value: &Value) -> Result<Verdict, String> {
    let Value::Object(result) = value else {
        return Err("test result is not an object".to_string());
    };
    if result.get("skipped").and_then(Value::as_bool) == Some(true) {
        return Ok(Verdict::Skipped {
            message: string_field(result, "message").unwrap_or_default(),
        });
    }
    if result.get("errored").and_then(Value::as_bool) == Some(true) {
        return Ok(Verdict::Errored {
            message: summarize_context(result.get("context")),
        });
    }
    match result.get("valid").and_then(Value::as_bool) {
        Some(true) => Ok(Verdict::Valid),
        Some(false) => Ok(Verdict::Invalid),
        None => Err("test result has no boolean \"valid\" and is not skipped/errored".to_string()),
    }
}

/// Summarize a whole-case error: the top-level `message` (when non-empty) wins over the
/// context, and uncaught errors (Bowtie had to rescue a crashing harness) are marked as such.
fn case_error_message(object: &Map<String, Value>) -> String {
    let message: String = string_field(object, "message")
        .filter(|m: &String| !m.trim().is_empty())
        .map_or_else(
            || summarize_context(object.get("context")),
            |m: String| summarize(&m),
        );
    if object.get("caught").and_then(Value::as_bool) == Some(false) {
        format!("uncaught: {message}")
    } else {
        message
    }
}

/// Reduce a Bowtie error context to one line: prefer `message`, then `stderr`, then
/// `traceback`, then the compact JSON of the whole context.
fn summarize_context(context: Option<&Value>) -> String {
    let Some(context) = context else {
        return "errored (no context)".to_string();
    };
    if let Value::Object(map) = context {
        for key in ["message", "stderr", "traceback"] {
            if let Some(text) = map.get(key).and_then(Value::as_str)
                && !text.trim().is_empty()
            {
                return summarize(text);
            }
        }
    }
    summarize(&context.to_string())
}

/// First non-empty line of `text`, truncated to [`ERROR_SUMMARY_MAX_CHARS`] characters.
fn summarize(text: &str) -> String {
    let line: &str = text
        .lines()
        .map(str::trim)
        .find(|l: &&str| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() > ERROR_SUMMARY_MAX_CHARS {
        let truncated: String = line.chars().take(ERROR_SUMMARY_MAX_CHARS).collect();
        format!("{truncated}...")
    } else {
        line.to_string()
    }
}

/// Fill in missing case-test expectations from a result line's `expected` array.
fn fill_expected(case: &mut Case, expected: &[Option<bool>]) {
    for (test, from_result) in case.tests.iter_mut().zip(expected) {
        if test.expected.is_none() {
            test.expected = *from_result;
        }
    }
}

fn expand_verdicts(
    implementations: &BTreeMap<String, ImplementationInfo>,
    cases: &[Case],
    raw_results: &BTreeMap<String, BTreeMap<String, RawResult>>,
) -> BTreeMap<String, BTreeMap<String, Vec<Verdict>>> {
    let empty: BTreeMap<String, RawResult> = BTreeMap::new();
    let mut verdicts: BTreeMap<String, BTreeMap<String, Vec<Verdict>>> = BTreeMap::new();
    for implementation in implementations.keys() {
        let per_case: &BTreeMap<String, RawResult> =
            raw_results.get(implementation).unwrap_or(&empty);
        let mut expanded: BTreeMap<String, Vec<Verdict>> = BTreeMap::new();
        for case in cases {
            let test_count: usize = case.tests.len();
            let case_verdicts: Vec<Verdict> =
                match per_case.get(&case.seq).map(|r: &RawResult| &r.outcome) {
                    None => vec![
                        Verdict::Errored {
                            message: "no result reported".to_string(),
                        };
                        test_count
                    ],
                    Some(RawOutcome::CaseSkipped(message)) => vec![
                        Verdict::Skipped {
                            message: message.clone(),
                        };
                        test_count
                    ],
                    Some(RawOutcome::CaseErrored(message)) => vec![
                        Verdict::Errored {
                            message: message.clone(),
                        };
                        test_count
                    ],
                    Some(RawOutcome::PerTest(results)) => {
                        let mut padded: Vec<Verdict> =
                            results.iter().take(test_count).cloned().collect();
                        while padded.len() < test_count {
                            padded.push(Verdict::Errored {
                                message: "no result reported".to_string(),
                            });
                        }
                        padded
                    }
                };
            expanded.insert(case.seq.clone(), case_verdicts);
        }
        verdicts.insert(implementation.clone(), expanded);
    }
    verdicts
}

#[cfg(test)]
mod tests {
    use super::{
        BowtieReport, Case, CaseTest, ImplementationInfo, ReportError, Verdict, parse_report,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    const HEADER: &str = r#"{"implementations": {"a": {"name": "alpha", "language": "rust", "version": "1.0"}, "b": {"name": "beta", "language": "python"}}, "bowtie_version": "x", "metadata": {}, "dialect": "https://json-schema.org/draft/2020-12/schema", "started": "2026-01-01T00:00:00+00:00"}"#;
    const CASE: &str = r#"{"seq": 1, "case": {"description": "min", "schema": {"minimum": 1}, "tests": [{"description": "ok", "instance": 2, "valid": true}, {"description": "bad", "instance": 0, "valid": false}]}}"#;
    const FOOTER: &str = r#"{"did_fail_fast": false}"#;

    fn implementations() -> BTreeMap<String, ImplementationInfo> {
        BTreeMap::from([
            (
                "a".to_string(),
                ImplementationInfo {
                    name: "alpha".to_string(),
                    language: "rust".to_string(),
                    version: Some("1.0".to_string()),
                },
            ),
            (
                "b".to_string(),
                ImplementationInfo {
                    name: "beta".to_string(),
                    language: "python".to_string(),
                    version: None,
                },
            ),
        ])
    }

    fn min_case() -> Case {
        Case {
            seq: "1".to_string(),
            description: "min".to_string(),
            schema: json!({"minimum": 1}),
            tests: vec![
                CaseTest {
                    description: "ok".to_string(),
                    instance: json!(2),
                    expected: Some(true),
                },
                CaseTest {
                    description: "bad".to_string(),
                    instance: json!(0),
                    expected: Some(false),
                },
            ],
        }
    }

    fn report_with(results: &[&str]) -> Result<BowtieReport, ReportError> {
        let mut lines: Vec<&str> = vec![HEADER, CASE];
        lines.extend_from_slice(results);
        lines.push(FOOTER);
        parse_report(&lines.join("\n"))
    }

    fn verdicts_of(report: &BowtieReport, implementation: &str) -> Vec<Verdict> {
        report.verdicts[implementation]["1"].clone()
    }

    #[test]
    fn parses_full_report() {
        let actual: Result<BowtieReport, ReportError> = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"valid": false}]}"#,
            r#"{"seq": 1, "implementation": "b", "expected": [true, false], "results": [{"valid": true}, {"valid": true}]}"#,
        ]);
        let expected: Result<BowtieReport, ReportError> = Ok(BowtieReport {
            dialect: "https://json-schema.org/draft/2020-12/schema".to_string(),
            implementations: implementations(),
            cases: vec![min_case()],
            verdicts: BTreeMap::from([
                (
                    "a".to_string(),
                    BTreeMap::from([("1".to_string(), vec![Verdict::Valid, Verdict::Invalid])]),
                ),
                (
                    "b".to_string(),
                    BTreeMap::from([("1".to_string(), vec![Verdict::Valid, Verdict::Valid])]),
                ),
            ]),
            did_fail_fast: false,
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn per_test_skip_and_error_results() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"skipped": true, "message": "unsupported: x"}, {"errored": true, "context": {"message": "boom\nmore"}}]}"#,
        ])
        .expect("valid report");
        let expected: Vec<Verdict> = vec![
            Verdict::Skipped {
                message: "unsupported: x".to_string(),
            },
            Verdict::Errored {
                message: "boom".to_string(),
            },
        ];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn whole_case_skip_expands_to_every_test() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "skipped": true, "message": "unsupported keyword \"minimum\" at #"}"#,
        ])
        .expect("valid report");
        let skipped: Verdict = Verdict::Skipped {
            message: "unsupported keyword \"minimum\" at #".to_string(),
        };
        let expected: Vec<Verdict> = vec![skipped.clone(), skipped];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn whole_case_skip_without_message() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "skipped": true}"#,
        ])
        .expect("valid report");
        let skipped: Verdict = Verdict::Skipped {
            message: String::new(),
        };
        let expected: Vec<Verdict> = vec![skipped.clone(), skipped];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn whole_case_error_prefers_message() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "errored": true, "context": {"traceback": "tb"}, "message": "compile failed", "caught": true}"#,
        ])
        .expect("valid report");
        let errored: Verdict = Verdict::Errored {
            message: "compile failed".to_string(),
        };
        let expected: Vec<Verdict> = vec![errored.clone(), errored];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn whole_case_uncaught_error_falls_back_to_context() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "errored": true, "context": {"stderr": "\n  panicked at x\n"}, "message": "", "caught": false}"#,
        ])
        .expect("valid report");
        let errored: Verdict = Verdict::Errored {
            message: "uncaught: panicked at x".to_string(),
        };
        let expected: Vec<Verdict> = vec![errored.clone(), errored];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn error_context_without_known_keys_is_serialized() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"errored": true, "context": {"code": 3}}]}"#,
        ])
        .expect("valid report");
        let expected: Vec<Verdict> = vec![
            Verdict::Valid,
            Verdict::Errored {
                message: r#"{"code":3}"#.to_string(),
            },
        ];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn error_without_context() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"errored": true}]}"#,
        ])
        .expect("valid report");
        let expected: Vec<Verdict> = vec![
            Verdict::Valid,
            Verdict::Errored {
                message: "errored (no context)".to_string(),
            },
        ];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn long_error_is_truncated() {
        let long: String = "x".repeat(305);
        let line: String = format!(
            r#"{{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{{"valid": true}}, {{"errored": true, "context": {{"message": "{long}"}}}}]}}"#
        );
        let report: BowtieReport = report_with(&[line.as_str()]).expect("valid report");
        let expected: Vec<Verdict> = vec![
            Verdict::Valid,
            Verdict::Errored {
                message: format!("{}...", "x".repeat(300)),
            },
        ];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_results_count_as_flags() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true, "annotations": []}, {"valid": false}]}"#,
        ])
        .expect("valid report");
        let expected: Vec<Verdict> = vec![Verdict::Valid, Verdict::Invalid];
        let actual: Vec<Verdict> = verdicts_of(&report, "a");
        assert_eq!(expected, actual);
    }

    #[test]
    fn missing_implementation_results_are_errored() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"valid": false}]}"#,
        ])
        .expect("valid report");
        let missing: Verdict = Verdict::Errored {
            message: "no result reported".to_string(),
        };
        let expected: Vec<Verdict> = vec![missing.clone(), missing];
        let actual: Vec<Verdict> = verdicts_of(&report, "b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn short_results_are_padded_and_long_results_truncated() {
        let report: BowtieReport = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}]}"#,
            r#"{"seq": 1, "implementation": "b", "expected": [true, false], "results": [{"valid": true}, {"valid": false}, {"valid": true}]}"#,
        ])
        .expect("valid report");
        let expected: (Vec<Verdict>, Vec<Verdict>) = (
            vec![
                Verdict::Valid,
                Verdict::Errored {
                    message: "no result reported".to_string(),
                },
            ],
            vec![Verdict::Valid, Verdict::Invalid],
        );
        let actual: (Vec<Verdict>, Vec<Verdict>) =
            (verdicts_of(&report, "a"), verdicts_of(&report, "b"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn missing_expectation_is_filled_from_result_line() {
        let text: String = [
            HEADER,
            r#"{"seq": "s1", "case": {"description": "bowtie validate", "schema": {}, "tests": [{"description": "", "instance": 1}, {"description": "", "instance": 2}]}}"#,
            r#"{"seq": "s1", "implementation": "a", "expected": [false, null], "results": [{"valid": true}, {"valid": true}]}"#,
            FOOTER,
        ]
        .join("\n");
        let report: BowtieReport = parse_report(&text).expect("valid report");
        let expected: Vec<Option<bool>> = vec![Some(false), None];
        let actual: Vec<Option<bool>> = report.cases[0]
            .tests
            .iter()
            .map(|t: &CaseTest| t.expected)
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn blank_lines_are_ignored_and_fail_fast_is_recorded() {
        let text: String = [HEADER, "", CASE, "   ", r#"{"did_fail_fast": true}"#, ""].join("\n");
        let report: BowtieReport = parse_report(&text).expect("valid report");
        let expected: bool = true;
        let actual: bool = report.did_fail_fast;
        assert_eq!(expected, actual);
    }

    fn error_of(text: &str) -> Result<BowtieReport, ReportError> {
        parse_report(text)
    }

    fn err(line: usize, message: &str) -> Result<BowtieReport, ReportError> {
        Err(ReportError {
            line,
            message: message.to_string(),
        })
    }

    #[test]
    fn empty_report_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(0, "empty report (no header line)");
        let actual: Result<BowtieReport, ReportError> = error_of("\n\n");
        assert_eq!(expected, actual);
    }

    #[test]
    fn missing_footer_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(
            0,
            "missing report footer (did_fail_fast); the Bowtie run did not finish",
        );
        let actual: Result<BowtieReport, ReportError> = error_of(&[HEADER, CASE].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_json_is_an_error() {
        let expected: Result<BowtieReport, ReportError> =
            err(2, "invalid JSON: expected ident at line 1 column 2");
        let actual: Result<BowtieReport, ReportError> = error_of(&[HEADER, "nope"].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_object_line_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(1, "line is not a JSON object");
        let actual: Result<BowtieReport, ReportError> = error_of("[1]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_header_first_line_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(
            1,
            "first line is not a report header (no \"implementations\" object)",
        );
        let actual: Result<BowtieReport, ReportError> = error_of(CASE);
        assert_eq!(expected, actual);
    }

    #[test]
    fn header_without_dialect_is_an_error() {
        let expected: Result<BowtieReport, ReportError> =
            err(1, "report header has no \"dialect\" string");
        let actual: Result<BowtieReport, ReportError> = error_of(r#"{"implementations": {}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_line_shape_is_an_error() {
        let expected: Result<BowtieReport, ReportError> =
            err(2, "line matches no known Bowtie report shape");
        let actual: Result<BowtieReport, ReportError> =
            error_of(&[HEADER, r#"{"what": 1}"#].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_case_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(3, "duplicate case seq 1");
        let actual: Result<BowtieReport, ReportError> = error_of(&[HEADER, CASE, CASE].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_result_is_an_error() {
        let result: &str = r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"valid": false}]}"#;
        let expected: Result<BowtieReport, ReportError> =
            err(4, "duplicate result for implementation a and case seq 1");
        let actual: Result<BowtieReport, ReportError> =
            error_of(&[HEADER, CASE, result, result, FOOTER].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn result_for_unknown_case_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(2, "result for unknown case seq 9");
        let actual: Result<BowtieReport, ReportError> = error_of(
            &[
                HEADER,
                r#"{"seq": 9, "implementation": "a", "expected": [], "results": []}"#,
            ]
            .join("\n"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn result_for_unknown_implementation_is_an_error() {
        let expected: Result<BowtieReport, ReportError> =
            err(0, "results for implementation zzz missing from the header");
        let actual: Result<BowtieReport, ReportError> = report_with(&[
            r#"{"seq": 1, "implementation": "zzz", "expected": [true, false], "results": [{"valid": true}, {"valid": false}]}"#,
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn result_without_outcome_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(
            3,
            "result for case seq 1 has neither \"results\", \"skipped\" nor \"errored\"",
        );
        let actual: Result<BowtieReport, ReportError> =
            report_with(&[r#"{"seq": 1, "implementation": "a", "expected": [true, false]}"#]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_result_without_valid_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(
            3,
            "test result has no boolean \"valid\" and is not skipped/errored",
        );
        let actual: Result<BowtieReport, ReportError> = report_with(&[
            r#"{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{}, {}]}"#,
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn content_after_footer_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(4, "content after the report footer");
        let actual: Result<BowtieReport, ReportError> =
            error_of(&[HEADER, CASE, FOOTER, CASE].join("\n"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn case_without_tests_is_an_error() {
        let expected: Result<BowtieReport, ReportError> = err(2, "case 1 has no \"tests\" array");
        let actual: Result<BowtieReport, ReportError> = error_of(
            &[
                HEADER,
                r#"{"seq": 1, "case": {"description": "d", "schema": {}}}"#,
            ]
            .join("\n"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn verdict_validity() {
        let expected: Vec<Option<bool>> = vec![Some(true), Some(false), None, None];
        let actual: Vec<Option<bool>> = [
            Verdict::Valid,
            Verdict::Invalid,
            Verdict::Skipped {
                message: String::new(),
            },
            Verdict::Errored {
                message: String::new(),
            },
        ]
        .iter()
        .map(Verdict::validity)
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn report_error_display() {
        let expected: Vec<String> = vec!["line 3: bad".to_string(), "whole".to_string()];
        let actual: Vec<String> = vec![
            ReportError {
                line: 3,
                message: "bad".to_string(),
            }
            .to_string(),
            ReportError {
                line: 0,
                message: "whole".to_string(),
            }
            .to_string(),
        ];
        assert_eq!(expected, actual);
    }
}
