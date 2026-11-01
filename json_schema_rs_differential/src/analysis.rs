//! Pure analysis over normalized Bowtie results: per-test records, tallies, compliance,
//! coverage vs. correctness, majority verdicts, disagreement lists, and the unsupported-feature
//! breakdown.
//!
//! Definitions used throughout:
//!
//! - **passed**: a real verdict (`valid`/`invalid`) equal to the expected validity;
//! - **failed**: a real verdict different from the expected validity;
//! - **skipped** / **errored**: no real verdict; for our harness a skip means an unsupported
//!   feature (a coverage gap) and an error means a harness or library crash;
//! - **compliance**: passed / all tests;
//! - **coverage** (ours): tests not skipped / all tests;
//! - **correctness** (ours): passed / (passed + failed), i.e. accuracy on the tests we ran;
//! - **majority**: among the *competitors* with a real verdict on a test, the verdict held by a
//!   strict majority; no strict majority (including a tie or zero voters) is "no consensus".

use crate::report::{BowtieReport, Verdict};
use crate::suite::{SuiteIndex, is_optional_path};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// One test with every implementation's verdict on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestRecord {
    /// The draft (e.g. `draft2020-12`) or fixture id the test belongs to.
    pub group: String,
    /// Bowtie's case seq, unique within the report the test came from.
    pub case_seq: String,
    /// The suite file (relative to `tests/<draft>/`) or fixture schema the case came from.
    pub file: Option<String>,
    /// Whether the test belongs to the optional suite.
    pub optional: bool,
    /// The case description.
    pub case_description: String,
    /// The test description.
    pub test_description: String,
    /// The expected validity, when known.
    pub expected: Option<bool>,
    /// Verdict per implementation id.
    pub verdicts: BTreeMap<String, Verdict>,
}

/// Flatten a suite report into per-test records, mapping each case back to its suite file.
///
/// `optional_report` marks every case as optional regardless of its mapped path (used for
/// reports produced from the optional suite).
#[must_use]
pub fn suite_records(
    draft: &str,
    report: &BowtieReport,
    index: &SuiteIndex,
    optional_report: bool,
) -> Vec<TestRecord> {
    let mut records: Vec<TestRecord> = Vec::new();
    for case in &report.cases {
        let file: Option<String> = index
            .locate(&case.description, &case.schema)
            .map(str::to_string);
        let optional: bool = optional_report || file.as_deref().is_some_and(is_optional_path);
        for (test_index, test) in case.tests.iter().enumerate() {
            records.push(TestRecord {
                group: draft.to_string(),
                case_seq: case.seq.clone(),
                file: file.clone(),
                optional,
                case_description: case.description.clone(),
                test_description: test.description.clone(),
                expected: test.expected,
                verdicts: verdicts_for(report, &case.seq, test_index),
            });
        }
    }
    records.sort_by(|a: &TestRecord, b: &TestRecord| {
        (a.file.is_none(), &a.file).cmp(&(b.file.is_none(), &b.file))
    });
    records
}

/// Flatten a `bowtie validate` fixture report into per-instance records.
///
/// The manifest is authoritative for the expectation (`expected_valid`) and for naming each
/// instance (`instance_paths`, in the order they were passed to `bowtie validate`).
///
/// # Errors
///
/// Returns a description of the mismatch when the report does not hold exactly one case with
/// one test per instance path.
pub fn fixture_records(
    fixture_id: &str,
    report: &BowtieReport,
    expected_valid: bool,
    instance_paths: &[String],
) -> Result<Vec<TestRecord>, String> {
    let [case] = report.cases.as_slice() else {
        return Err(format!(
            "expected exactly one case in the {} report, found {}",
            validity_word(expected_valid),
            report.cases.len()
        ));
    };
    if case.tests.len() != instance_paths.len() {
        return Err(format!(
            "manifest lists {} {} instances but the report has {} tests",
            instance_paths.len(),
            validity_word(expected_valid),
            case.tests.len()
        ));
    }
    let records: Vec<TestRecord> = instance_paths
        .iter()
        .enumerate()
        .map(|(test_index, path): (usize, &String)| TestRecord {
            group: fixture_id.to_string(),
            case_seq: case.seq.clone(),
            file: None,
            optional: false,
            case_description: format!("{} instances", validity_word(expected_valid)),
            test_description: path.clone(),
            expected: Some(expected_valid),
            verdicts: verdicts_for(report, &case.seq, test_index),
        })
        .collect();
    Ok(records)
}

fn validity_word(valid: bool) -> &'static str {
    if valid { "valid" } else { "invalid" }
}

fn verdicts_for(report: &BowtieReport, seq: &str, test_index: usize) -> BTreeMap<String, Verdict> {
    let mut verdicts: BTreeMap<String, Verdict> = BTreeMap::new();
    for (implementation, per_case) in &report.verdicts {
        let verdict: Verdict = per_case
            .get(seq)
            .and_then(|v: &Vec<Verdict>| v.get(test_index))
            .cloned()
            .unwrap_or_else(|| Verdict::Errored {
                message: "no result reported".to_string(),
            });
        verdicts.insert(implementation.clone(), verdict);
    }
    verdicts
}

/// Outcome counts for one implementation over a set of tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    /// All tests considered.
    pub total: u64,
    /// Real verdict equal to the expectation.
    pub passed: u64,
    /// Real verdict different from the expectation.
    pub failed: u64,
    /// Skipped tests.
    pub skipped: u64,
    /// Errored tests.
    pub errored: u64,
    /// Real verdicts on tests without an expectation (neither passed nor failed).
    pub unexpected: u64,
}

impl Tally {
    /// Count one test.
    pub fn add(&mut self, expected: Option<bool>, verdict: &Verdict) {
        self.total += 1;
        match (verdict.validity(), expected) {
            (Some(actual), Some(wanted)) if actual == wanted => self.passed += 1,
            (Some(_), Some(_)) => self.failed += 1,
            (Some(_), None) => self.unexpected += 1,
            (None, _) => {
                if matches!(verdict, Verdict::Skipped { .. }) {
                    self.skipped += 1;
                } else {
                    self.errored += 1;
                }
            }
        }
    }

    /// Add another tally's counts to this one.
    pub fn merge(&mut self, other: &Tally) {
        self.total += other.total;
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
        self.errored += other.errored;
        self.unexpected += other.unexpected;
    }
}

/// A fraction with its percentage pre-rendered (two decimals, rounded half up) so the JSON
/// output is byte-stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ratio {
    /// The numerator.
    pub numerator: u64,
    /// The denominator.
    pub denominator: u64,
    /// The percentage, e.g. `"97.53%"`; `None` when the denominator is zero.
    pub percent: Option<String>,
}

impl Ratio {
    /// Build a ratio and render its percentage.
    #[must_use]
    pub fn new(numerator: u64, denominator: u64) -> Self {
        let percent: Option<String> = if denominator == 0 {
            None
        } else {
            let basis_points: u128 = (u128::from(numerator) * 20_000 + u128::from(denominator))
                / (2 * u128::from(denominator));
            Some(format!("{}.{:02}%", basis_points / 100, basis_points % 100))
        };
        Ratio {
            numerator,
            denominator,
            percent,
        }
    }

    /// The percentage, or `n/a` when the denominator is zero.
    #[must_use]
    pub fn display(&self) -> &str {
        self.percent.as_deref().unwrap_or("n/a")
    }
}

/// Compliance of one implementation over a set of tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplementationStats {
    /// Outcome counts.
    pub tally: Tally,
    /// passed / total.
    pub compliance: Ratio,
}

impl ImplementationStats {
    /// Derive the stats from a tally.
    #[must_use]
    pub fn from_tally(tally: Tally) -> Self {
        ImplementationStats {
            compliance: Ratio::new(tally.passed, tally.total),
            tally,
        }
    }
}

/// Our implementation's stats, separating coverage from correctness.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OurStats {
    /// Outcome counts.
    pub tally: Tally,
    /// passed / total.
    pub compliance: Ratio,
    /// (total - skipped) / total: share of tests we ran rather than skipped as unsupported.
    pub coverage: Ratio,
    /// passed / (passed + failed): share of the tests we ran that matched the expectation.
    pub correctness: Ratio,
    /// Tests where our real verdict differs from the competitors' majority.
    pub disagreements_with_majority: u64,
    /// Tests where our real verdict differs from the expectation (equals `tally.failed`).
    pub wrong_vs_expected: u64,
}

/// The competitors' majority verdict on one test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Majority {
    /// The strict-majority verdict (`true` = valid), or `None` for no consensus.
    pub verdict: Option<bool>,
    /// Competitors holding the most common verdict (the majority when there is one).
    pub agreeing: u64,
    /// Competitors with a real verdict (skipped and errored competitors do not vote).
    pub voters: u64,
}

/// The majority verdict among every implementation except `ours`.
#[must_use]
pub fn majority(verdicts: &BTreeMap<String, Verdict>, ours: &str) -> Majority {
    let mut valid: u64 = 0;
    let mut invalid: u64 = 0;
    for (implementation, verdict) in verdicts {
        if implementation == ours {
            continue;
        }
        match verdict.validity() {
            Some(true) => valid += 1,
            Some(false) => invalid += 1,
            None => {}
        }
    }
    let voters: u64 = valid + invalid;
    let verdict: Option<bool> = if valid * 2 > voters {
        Some(true)
    } else if invalid * 2 > voters {
        Some(false)
    } else {
        None
    };
    Majority {
        verdict,
        agreeing: valid.max(invalid),
        voters,
    }
}

/// How a disagreement should be triaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Triage {
    /// We differ from the expectation and the majority matches it.
    LikelyOurBug,
    /// We match the expectation and the majority does not.
    CompetitorsDifferFromExpected,
    /// Neither we nor the majority match the expectation.
    Ambiguous,
    /// Competitors have no strict majority.
    NoConsensus,
    /// The test has no expectation.
    NoExpectation,
    /// We, the majority, and the expectation all agree.
    Agreement,
}

impl Triage {
    /// Classify our verdict against the expectation and the majority.
    #[must_use]
    pub fn classify(expected: Option<bool>, ours: bool, majority: Option<bool>) -> Self {
        let Some(expected) = expected else {
            return Triage::NoExpectation;
        };
        let Some(majority) = majority else {
            return Triage::NoConsensus;
        };
        match (ours == expected, majority == expected) {
            (false, true) => Triage::LikelyOurBug,
            (false, false) => Triage::Ambiguous,
            (true, false) => Triage::CompetitorsDifferFromExpected,
            (true, true) => Triage::Agreement,
        }
    }

    /// Short human-readable label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Triage::LikelyOurBug => "likely our bug",
            Triage::CompetitorsDifferFromExpected => "competitors differ from expected",
            Triage::Ambiguous => "ambiguous",
            Triage::NoConsensus => "no consensus",
            Triage::NoExpectation => "no expectation",
            Triage::Agreement => "agreement",
        }
    }
}

/// One test of a [`DisagreementCase`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisagreementTest {
    /// The test description.
    pub test_description: String,
    /// The expected validity, when known.
    pub expected: Option<bool>,
    /// Our verdict (`true` = valid).
    pub ours: bool,
    /// The competitors' strict-majority verdict, `None` for no consensus.
    pub majority: Option<bool>,
    /// Competitors holding the most common verdict.
    pub agreeing: u64,
    /// Competitors with a real verdict.
    pub voters: u64,
}

/// Consecutive tests of one case (same file and triage) where our real verdict differs from
/// the majority or from the expectation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisagreementCase {
    /// Draft or fixture id.
    pub group: String,
    /// Suite file, when known.
    pub file: Option<String>,
    /// Whether the tests belong to the optional suite.
    pub optional: bool,
    /// The case description.
    pub case_description: String,
    /// The triage shared by every test of the entry.
    pub triage: Triage,
    /// The tests, in record order.
    pub tests: Vec<DisagreementTest>,
}

/// Total number of tests across disagreement cases.
#[must_use]
pub fn test_count(cases: &[DisagreementCase]) -> u64 {
    cases
        .iter()
        .map(|c: &DisagreementCase| u64::try_from(c.tests.len()).unwrap_or(u64::MAX))
        .sum()
}

fn push_disagreement(
    into: &mut Vec<DisagreementCase>,
    record: &TestRecord,
    triage: Triage,
    test: DisagreementTest,
) {
    if let Some(last) = into.last_mut()
        && last.group == record.group
        && last.file == record.file
        && last.optional == record.optional
        && last.case_description == record.case_description
        && last.triage == triage
    {
        last.tests.push(test);
        return;
    }
    into.push(DisagreementCase {
        group: record.group.clone(),
        file: record.file.clone(),
        optional: record.optional,
        case_description: record.case_description.clone(),
        triage,
        tests: vec![test],
    });
}

/// A test on which our harness errored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErroredTest {
    /// Draft or fixture id.
    pub group: String,
    /// Suite file, when known.
    pub file: Option<String>,
    /// The case description.
    pub case_description: String,
    /// The test description.
    pub test_description: String,
    /// The error summary.
    pub message: String,
}

/// Tests in one suite file (or fixture) where our verdict differs from the expectation but
/// agrees with the competitor majority (the majority contradicts the expectation too). These
/// are low-signal (typically optional format tests most implementations do not assert), so
/// they are aggregated per file rather than listed per test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WrongWithMajority {
    /// Draft or fixture id.
    pub group: String,
    /// Suite file, when known.
    pub file: Option<String>,
    /// Whether the tests belong to the optional suite.
    pub optional: bool,
    /// Number of tests aggregated.
    pub tests: u64,
    /// Number of tests per majority size, keyed `"<agreeing>/<voters>"`.
    pub majority: BTreeMap<String, u64>,
}

/// Disagreement lists and errors for our implementation over a set of records.
///
/// Every test where our real verdict differs from the expectation appears in exactly one of
/// `disagreements_with_majority` (the majority differs from us), `wrong_without_consensus`
/// (no competitor majority), or `wrong_with_majority` (the majority agrees with us).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct OurFindings {
    /// Tests where our verdict differs from a competitor majority.
    pub disagreements_with_majority: Vec<DisagreementCase>,
    /// Tests where our verdict differs from the expectation and competitors have no majority.
    pub wrong_without_consensus: Vec<DisagreementCase>,
    /// Tests where our verdict differs from the expectation but agrees with the majority,
    /// aggregated per file in order of first appearance.
    pub wrong_with_majority: Vec<WrongWithMajority>,
    /// Tests our harness errored on.
    pub errored: Vec<ErroredTest>,
}

impl OurFindings {
    fn add_wrong_with_majority(&mut self, record: &TestRecord, majority: Majority) {
        let key: String = format!("{}/{}", majority.agreeing, majority.voters);
        let existing: Option<&mut WrongWithMajority> =
            self.wrong_with_majority
                .iter_mut()
                .find(|w: &&mut WrongWithMajority| {
                    w.group == record.group
                        && w.file == record.file
                        && w.optional == record.optional
                });
        if let Some(entry) = existing {
            entry.tests += 1;
            *entry.majority.entry(key).or_default() += 1;
        } else {
            self.wrong_with_majority.push(WrongWithMajority {
                group: record.group.clone(),
                file: record.file.clone(),
                optional: record.optional,
                tests: 1,
                majority: BTreeMap::from([(key, 1)]),
            });
        }
    }
}

/// Collect every disagreement and error of `ours` over `records`, in record order.
#[must_use]
pub fn our_findings<'r, I>(records: I, ours: &str) -> OurFindings
where
    I: IntoIterator<Item = &'r TestRecord>,
{
    let mut findings: OurFindings = OurFindings::default();
    for record in records {
        let Some(verdict) = record.verdicts.get(ours) else {
            continue;
        };
        if let Verdict::Errored { message } = verdict {
            findings.errored.push(ErroredTest {
                group: record.group.clone(),
                file: record.file.clone(),
                case_description: record.case_description.clone(),
                test_description: record.test_description.clone(),
                message: message.clone(),
            });
            continue;
        }
        let Some(our_verdict) = verdict.validity() else {
            continue;
        };
        let majority: Majority = majority(&record.verdicts, ours);
        let wrong: bool = record.expected.is_some_and(|e: bool| e != our_verdict);
        let against_majority: bool = majority.verdict.is_some_and(|m: bool| m != our_verdict);
        if !wrong && !against_majority {
            continue;
        }
        if wrong && majority.verdict == Some(our_verdict) {
            findings.add_wrong_with_majority(record, majority);
            continue;
        }
        let triage: Triage = Triage::classify(record.expected, our_verdict, majority.verdict);
        let test: DisagreementTest = DisagreementTest {
            test_description: record.test_description.clone(),
            expected: record.expected,
            ours: our_verdict,
            majority: majority.verdict,
            agreeing: majority.agreeing,
            voters: majority.voters,
        };
        if against_majority {
            push_disagreement(
                &mut findings.disagreements_with_majority,
                record,
                triage,
                test,
            );
        } else {
            push_disagreement(&mut findings.wrong_without_consensus, record, triage, test);
        }
    }
    findings
}

/// Per-implementation compliance plus our coverage/correctness over a set of records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionReport {
    /// Number of tests in the section.
    pub tests: u64,
    /// Compliance per implementation id (including ours).
    pub implementations: BTreeMap<String, ImplementationStats>,
    /// Our stats.
    pub ours: OurStats,
}

/// Tally every implementation over `records` and derive our stats.
#[must_use]
pub fn section_report(records: &[&TestRecord], ours: &str) -> SectionReport {
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for record in records {
        for (implementation, verdict) in &record.verdicts {
            tallies
                .entry(implementation.clone())
                .or_default()
                .add(record.expected, verdict);
        }
    }
    let our_tally: Tally = tallies.get(ours).copied().unwrap_or_default();
    let findings: OurFindings = our_findings(records.iter().copied(), ours);
    let tests: u64 = u64::try_from(records.len()).unwrap_or(u64::MAX);
    SectionReport {
        tests,
        implementations: tallies
            .into_iter()
            .map(|(id, tally): (String, Tally)| (id, ImplementationStats::from_tally(tally)))
            .collect(),
        ours: our_stats(our_tally, &findings),
    }
}

fn our_stats(tally: Tally, findings: &OurFindings) -> OurStats {
    OurStats {
        tally,
        compliance: Ratio::new(tally.passed, tally.total),
        coverage: Ratio::new(tally.total - tally.skipped, tally.total),
        correctness: Ratio::new(tally.passed, tally.passed + tally.failed),
        disagreements_with_majority: test_count(&findings.disagreements_with_majority),
        wrong_vs_expected: tally.failed,
    }
}

/// One reason our harness skipped tests, with its impact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnsupportedReason {
    /// The normalized reason, e.g. `keyword "dependentSchemas"` or `schema: boolean schema`.
    pub reason: String,
    /// Skipped tests attributed to the reason.
    pub tests: u64,
    /// Distinct cases with at least one test skipped for the reason.
    pub cases: u64,
    /// Skipped tests per draft (or fixture id).
    pub by_group: BTreeMap<String, u64>,
}

/// Normalize one skip message from our harness to a grouping reason.
///
/// The harness contract: `unsupported keyword "<key>" at <path>` (grouped by keyword, the path
/// is dropped), `unsupported schema: <detail>` (grouped by detail), and `unsupported $ref:
/// "<ref>": <reason>` (grouped by reason, the reference is dropped). Anything else is kept
/// verbatim under `unrecognized skip: `.
#[must_use]
pub fn skip_reason(message: &str) -> String {
    if let Some(rest) = message.strip_prefix("unsupported keyword ") {
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(end) = quoted.find('"')
        {
            return format!("keyword \"{}\"", &quoted[..end]);
        }
        return format!("keyword {rest}");
    }
    if let Some(detail) = message.strip_prefix("unsupported schema: ") {
        return format!("schema: {detail}");
    }
    if let Some(detail) = message.strip_prefix("unsupported $ref: ") {
        // `"<ref>": <reason>` groups by reason; the specific reference is dropped.
        if detail.starts_with('"')
            && let Some(end) = detail.find("\": ")
        {
            return format!("$ref: {}", &detail[end + 3..]);
        }
        return format!("$ref: {detail}");
    }
    if message.trim().is_empty() {
        return "skipped without a message".to_string();
    }
    format!("unrecognized skip: {message}")
}

/// Group every test `ours` skipped by [`skip_reason`], sorted by impact (most tests first,
/// then reason).
#[must_use]
pub fn unsupported_breakdown<'r, I>(records: I, ours: &str) -> Vec<UnsupportedReason>
where
    I: IntoIterator<Item = &'r TestRecord>,
{
    let mut tests: BTreeMap<String, u64> = BTreeMap::new();
    let mut cases: BTreeMap<String, BTreeSet<(String, String)>> = BTreeMap::new();
    let mut by_group: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    for record in records {
        let Some(Verdict::Skipped { message }) = record.verdicts.get(ours) else {
            continue;
        };
        let reason: String = skip_reason(message);
        *tests.entry(reason.clone()).or_default() += 1;
        cases
            .entry(reason.clone())
            .or_default()
            .insert((record.group.clone(), record.case_seq.clone()));
        *by_group
            .entry(reason)
            .or_default()
            .entry(record.group.clone())
            .or_default() += 1;
    }
    let mut reasons: Vec<UnsupportedReason> = tests
        .into_iter()
        .map(|(reason, count): (String, u64)| UnsupportedReason {
            cases: cases
                .get(&reason)
                .map_or(0, |c: &BTreeSet<(String, String)>| {
                    u64::try_from(c.len()).unwrap_or(u64::MAX)
                }),
            by_group: by_group.remove(&reason).unwrap_or_default(),
            tests: count,
            reason,
        })
        .collect();
    reasons.sort_by(|a: &UnsupportedReason, b: &UnsupportedReason| {
        b.tests.cmp(&a.tests).then_with(|| a.reason.cmp(&b.reason))
    });
    reasons
}

#[cfg(test)]
mod tests {
    use super::{
        DisagreementCase, DisagreementTest, ErroredTest, ImplementationStats, Majority,
        OurFindings, OurStats, Ratio, SectionReport, Tally, TestRecord, Triage, UnsupportedReason,
        WrongWithMajority, fixture_records, majority, our_findings, section_report, skip_reason,
        suite_records, unsupported_breakdown,
    };
    use crate::report::{BowtieReport, Case, CaseTest, Verdict};
    use crate::suite::{SuiteFile, SuiteFileCase, SuiteIndex};
    use serde_json::json;
    use std::collections::BTreeMap;

    const OURS: &str = "ours";

    fn skipped(message: &str) -> Verdict {
        Verdict::Skipped {
            message: message.to_string(),
        }
    }

    fn errored(message: &str) -> Verdict {
        Verdict::Errored {
            message: message.to_string(),
        }
    }

    fn verdicts(entries: &[(&str, Verdict)]) -> BTreeMap<String, Verdict> {
        entries
            .iter()
            .map(|(id, v): &(&str, Verdict)| ((*id).to_string(), v.clone()))
            .collect()
    }

    fn record(
        group: &str,
        seq: &str,
        expected: Option<bool>,
        entries: &[(&str, Verdict)],
    ) -> TestRecord {
        TestRecord {
            group: group.to_string(),
            case_seq: seq.to_string(),
            file: Some("f.json".to_string()),
            optional: false,
            case_description: format!("case {seq}"),
            test_description: "test".to_string(),
            expected,
            verdicts: verdicts(entries),
        }
    }

    fn two_case_report() -> BowtieReport {
        BowtieReport {
            dialect: "d".to_string(),
            implementations: BTreeMap::new(),
            cases: vec![
                Case {
                    seq: "1".to_string(),
                    description: "unmapped".to_string(),
                    schema: json!({}),
                    tests: vec![CaseTest {
                        description: "t1".to_string(),
                        instance: json!(1),
                        expected: Some(true),
                    }],
                },
                Case {
                    seq: "2".to_string(),
                    description: "bignum".to_string(),
                    schema: json!({"type": "integer"}),
                    tests: vec![CaseTest {
                        description: "t2".to_string(),
                        instance: json!(2),
                        expected: Some(false),
                    }],
                },
            ],
            verdicts: BTreeMap::from([(
                OURS.to_string(),
                BTreeMap::from([("1".to_string(), vec![Verdict::Valid])]),
            )]),
            did_fail_fast: false,
        }
    }

    fn bignum_index() -> SuiteIndex {
        SuiteIndex::new(&[SuiteFile {
            path: "optional/bignum.json".to_string(),
            cases: vec![SuiteFileCase {
                description: "bignum".to_string(),
                schema: json!({"type": "integer"}),
                tests: Vec::new(),
            }],
        }])
    }

    #[test]
    fn suite_records_map_files_and_sort_unmapped_last() {
        let expected: Vec<TestRecord> = vec![
            TestRecord {
                group: "draft7".to_string(),
                case_seq: "2".to_string(),
                file: Some("optional/bignum.json".to_string()),
                optional: true,
                case_description: "bignum".to_string(),
                test_description: "t2".to_string(),
                expected: Some(false),
                verdicts: verdicts(&[(OURS, errored("no result reported"))]),
            },
            TestRecord {
                group: "draft7".to_string(),
                case_seq: "1".to_string(),
                file: None,
                optional: false,
                case_description: "unmapped".to_string(),
                test_description: "t1".to_string(),
                expected: Some(true),
                verdicts: verdicts(&[(OURS, Verdict::Valid)]),
            },
        ];
        let actual: Vec<TestRecord> =
            suite_records("draft7", &two_case_report(), &bignum_index(), false);
        assert_eq!(expected, actual);
    }

    #[test]
    fn suite_records_from_optional_report_are_all_optional() {
        let expected: Vec<bool> = vec![true, true];
        let actual: Vec<bool> =
            suite_records("draft7", &two_case_report(), &SuiteIndex::default(), true)
                .iter()
                .map(|r: &TestRecord| r.optional)
                .collect();
        assert_eq!(expected, actual);
    }

    fn fixture_report(test_count: usize) -> BowtieReport {
        BowtieReport {
            dialect: "d".to_string(),
            implementations: BTreeMap::new(),
            cases: vec![Case {
                seq: "7".to_string(),
                description: "bowtie validate".to_string(),
                schema: json!({}),
                tests: vec![
                    CaseTest {
                        description: String::new(),
                        instance: json!(1),
                        expected: Some(false),
                    };
                    test_count
                ],
            }],
            verdicts: BTreeMap::from([(
                OURS.to_string(),
                BTreeMap::from([("7".to_string(), vec![Verdict::Invalid; test_count])]),
            )]),
            did_fail_fast: false,
        }
    }

    #[test]
    fn fixture_records_name_instances_from_manifest() {
        let expected: Result<Vec<TestRecord>, String> = Ok(vec![TestRecord {
            group: "small/config".to_string(),
            case_seq: "7".to_string(),
            file: None,
            optional: false,
            case_description: "invalid instances".to_string(),
            test_description: "small/config/instances/invalid/a.json".to_string(),
            expected: Some(false),
            verdicts: verdicts(&[(OURS, Verdict::Invalid)]),
        }]);
        let actual: Result<Vec<TestRecord>, String> = fixture_records(
            "small/config",
            &fixture_report(1),
            false,
            &["small/config/instances/invalid/a.json".to_string()],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn fixture_records_reject_count_mismatch() {
        let expected: Result<Vec<TestRecord>, String> =
            Err("manifest lists 1 valid instances but the report has 2 tests".to_string());
        let actual: Result<Vec<TestRecord>, String> =
            fixture_records("x", &fixture_report(2), true, &["a".to_string()]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn fixture_records_reject_multiple_cases() {
        let mut report: BowtieReport = fixture_report(1);
        report.cases.push(report.cases[0].clone());
        let expected: Result<Vec<TestRecord>, String> =
            Err("expected exactly one case in the invalid report, found 2".to_string());
        let actual: Result<Vec<TestRecord>, String> =
            fixture_records("x", &report, false, &["a".to_string()]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn tally_counts_every_outcome() {
        let mut actual: Tally = Tally::default();
        actual.add(Some(true), &Verdict::Valid);
        actual.add(Some(true), &Verdict::Invalid);
        actual.add(Some(false), &skipped("s"));
        actual.add(Some(false), &errored("e"));
        actual.add(None, &Verdict::Valid);
        actual.add(None, &skipped("s"));
        let expected: Tally = Tally {
            total: 6,
            passed: 1,
            failed: 1,
            skipped: 2,
            errored: 1,
            unexpected: 1,
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn tally_merge_sums_fields() {
        let mut actual: Tally = Tally {
            total: 1,
            passed: 1,
            failed: 0,
            skipped: 0,
            errored: 0,
            unexpected: 0,
        };
        actual.merge(&Tally {
            total: 5,
            passed: 1,
            failed: 1,
            skipped: 1,
            errored: 1,
            unexpected: 1,
        });
        let expected: Tally = Tally {
            total: 6,
            passed: 2,
            failed: 1,
            skipped: 1,
            errored: 1,
            unexpected: 1,
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn ratio_rounds_half_up() {
        let expected: Vec<Option<String>> = vec![
            Some("66.67%".to_string()),
            Some("100.00%".to_string()),
            Some("0.00%".to_string()),
            Some("0.01%".to_string()),
            None,
        ];
        let actual: Vec<Option<String>> = [(2, 3), (5, 5), (0, 4), (1, 15_000), (0, 0)]
            .iter()
            .map(|(n, d): &(u64, u64)| Ratio::new(*n, *d).percent)
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn ratio_display_zero_denominator() {
        let expected: &str = "n/a";
        let ratio: Ratio = Ratio::new(0, 0);
        let actual: &str = ratio.display();
        assert_eq!(expected, actual);
    }

    #[test]
    fn majority_valid() {
        let expected: Majority = Majority {
            verdict: Some(true),
            agreeing: 2,
            voters: 3,
        };
        let actual: Majority = majority(
            &verdicts(&[
                (OURS, Verdict::Invalid),
                ("a", Verdict::Valid),
                ("b", Verdict::Valid),
                ("c", Verdict::Invalid),
            ]),
            OURS,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn majority_invalid_ignores_skipped_and_errored() {
        let expected: Majority = Majority {
            verdict: Some(false),
            agreeing: 1,
            voters: 1,
        };
        let actual: Majority = majority(
            &verdicts(&[
                (OURS, Verdict::Valid),
                ("a", Verdict::Invalid),
                ("b", skipped("x")),
                ("c", errored("y")),
            ]),
            OURS,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn majority_tie_is_no_consensus() {
        let expected: Majority = Majority {
            verdict: None,
            agreeing: 1,
            voters: 2,
        };
        let actual: Majority = majority(
            &verdicts(&[("a", Verdict::Invalid), ("b", Verdict::Valid)]),
            OURS,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn majority_without_voters_is_no_consensus() {
        let expected: Majority = Majority {
            verdict: None,
            agreeing: 0,
            voters: 0,
        };
        let actual: Majority = majority(
            &verdicts(&[
                (OURS, Verdict::Valid),
                ("a", skipped("x")),
                ("b", errored("y")),
            ]),
            OURS,
        );
        assert_eq!(expected, actual);
    }

    fn disagreement(
        seq: &str,
        expected: Option<bool>,
        ours: bool,
        majority: Majority,
        triage: Triage,
    ) -> DisagreementCase {
        DisagreementCase {
            group: "draft7".to_string(),
            file: Some("f.json".to_string()),
            optional: false,
            case_description: format!("case {seq}"),
            triage,
            tests: vec![DisagreementTest {
                test_description: "test".to_string(),
                expected,
                ours,
                majority: majority.verdict,
                agreeing: majority.agreeing,
                voters: majority.voters,
            }],
        }
    }

    fn findings_records() -> Vec<TestRecord> {
        vec![
            // 1: we are wrong, majority agrees with expected -> both lists.
            record(
                "draft7",
                "1",
                Some(false),
                &[
                    (OURS, Verdict::Valid),
                    ("a", Verdict::Invalid),
                    ("b", Verdict::Invalid),
                ],
            ),
            // 2: we match expected, majority disagrees with expected -> majority list only.
            record(
                "draft7",
                "2",
                Some(true),
                &[(OURS, Verdict::Valid), ("a", Verdict::Invalid)],
            ),
            // 3: we are wrong, no consensus -> expected list only.
            record(
                "draft7",
                "3",
                Some(true),
                &[
                    (OURS, Verdict::Invalid),
                    ("a", Verdict::Invalid),
                    ("b", Verdict::Valid),
                ],
            ),
            // 4: agreement everywhere -> nothing.
            record(
                "draft7",
                "4",
                Some(true),
                &[(OURS, Verdict::Valid), ("a", Verdict::Valid)],
            ),
            // 5: skipped -> nothing.
            record(
                "draft7",
                "5",
                Some(true),
                &[(OURS, skipped("s")), ("a", Verdict::Invalid)],
            ),
            // 6: errored -> errored list.
            record(
                "draft7",
                "6",
                Some(true),
                &[(OURS, errored("boom")), ("a", Verdict::Valid)],
            ),
            // 7: no expectation, disagrees with majority -> majority list only.
            record(
                "draft7",
                "7",
                None,
                &[(OURS, Verdict::Valid), ("a", Verdict::Invalid)],
            ),
            // 8: ours absent -> nothing.
            record("draft7", "8", Some(true), &[("a", Verdict::Invalid)]),
            // 9 and 10: we are wrong but the majority agrees with us -> aggregated per file.
            record(
                "draft7",
                "9",
                Some(true),
                &[(OURS, Verdict::Invalid), ("a", Verdict::Invalid)],
            ),
            record(
                "draft7",
                "10",
                Some(false),
                &[
                    (OURS, Verdict::Valid),
                    ("a", Verdict::Valid),
                    ("b", Verdict::Valid),
                ],
            ),
        ]
    }

    #[test]
    fn our_findings_lists_every_kind() {
        let expected: OurFindings = OurFindings {
            disagreements_with_majority: vec![
                disagreement(
                    "1",
                    Some(false),
                    true,
                    Majority {
                        verdict: Some(false),
                        agreeing: 2,
                        voters: 2,
                    },
                    Triage::LikelyOurBug,
                ),
                disagreement(
                    "2",
                    Some(true),
                    true,
                    Majority {
                        verdict: Some(false),
                        agreeing: 1,
                        voters: 1,
                    },
                    Triage::CompetitorsDifferFromExpected,
                ),
                disagreement(
                    "7",
                    None,
                    true,
                    Majority {
                        verdict: Some(false),
                        agreeing: 1,
                        voters: 1,
                    },
                    Triage::NoExpectation,
                ),
            ],
            wrong_without_consensus: vec![disagreement(
                "3",
                Some(true),
                false,
                Majority {
                    verdict: None,
                    agreeing: 1,
                    voters: 2,
                },
                Triage::NoConsensus,
            )],
            wrong_with_majority: vec![WrongWithMajority {
                group: "draft7".to_string(),
                file: Some("f.json".to_string()),
                optional: false,
                tests: 2,
                majority: BTreeMap::from([("1/1".to_string(), 1), ("2/2".to_string(), 1)]),
            }],
            errored: vec![ErroredTest {
                group: "draft7".to_string(),
                file: Some("f.json".to_string()),
                case_description: "case 6".to_string(),
                test_description: "test".to_string(),
                message: "boom".to_string(),
            }],
        };
        let actual: OurFindings = our_findings(&findings_records(), OURS);
        assert_eq!(expected, actual);
    }

    #[test]
    fn section_report_tallies_every_implementation() {
        let records: Vec<TestRecord> = findings_records();
        let refs: Vec<&TestRecord> = records.iter().collect();
        let our_tally: Tally = Tally {
            total: 9,
            passed: 2,
            failed: 4,
            skipped: 1,
            errored: 1,
            unexpected: 1,
        };
        let a_tally: Tally = Tally {
            total: 10,
            passed: 3,
            failed: 6,
            skipped: 0,
            errored: 0,
            unexpected: 1,
        };
        let b_tally: Tally = Tally {
            total: 3,
            passed: 2,
            failed: 1,
            skipped: 0,
            errored: 0,
            unexpected: 0,
        };
        let expected: SectionReport = SectionReport {
            tests: 10,
            implementations: BTreeMap::from([
                ("a".to_string(), ImplementationStats::from_tally(a_tally)),
                ("b".to_string(), ImplementationStats::from_tally(b_tally)),
                (OURS.to_string(), ImplementationStats::from_tally(our_tally)),
            ]),
            ours: OurStats {
                tally: our_tally,
                compliance: Ratio::new(2, 9),
                coverage: Ratio::new(8, 9),
                correctness: Ratio::new(2, 6),
                disagreements_with_majority: 3,
                wrong_vs_expected: 4,
            },
        };
        let actual: SectionReport = section_report(&refs, OURS);
        assert_eq!(expected, actual);
    }

    #[test]
    fn section_report_without_ours_is_empty_stats() {
        let records: Vec<TestRecord> = vec![record("g", "1", Some(true), &[("a", Verdict::Valid)])];
        let refs: Vec<&TestRecord> = records.iter().collect();
        let expected: OurStats = OurStats {
            tally: Tally::default(),
            compliance: Ratio::new(0, 0),
            coverage: Ratio::new(0, 0),
            correctness: Ratio::new(0, 0),
            disagreements_with_majority: 0,
            wrong_vs_expected: 0,
        };
        let actual: OurStats = section_report(&refs, OURS).ours;
        assert_eq!(expected, actual);
    }

    #[test]
    fn skip_reason_normalizes_contract_messages() {
        let expected: Vec<String> = vec![
            "keyword \"dependentSchemas\"".to_string(),
            "keyword unquoted at #".to_string(),
            "keyword \"broken".to_string(),
            "schema: boolean schema".to_string(),
            "$ref: remote reference http://x".to_string(),
            "$ref: remote references are not supported".to_string(),
            "skipped without a message".to_string(),
            "unrecognized skip: nope".to_string(),
        ];
        let actual: Vec<String> = [
            "unsupported keyword \"dependentSchemas\" at /properties/a",
            "unsupported keyword unquoted at #",
            "unsupported keyword \"broken",
            "unsupported schema: boolean schema",
            "unsupported $ref: remote reference http://x",
            "unsupported $ref: \"http://localhost:1234/a.json\": remote references are not supported",
            "  ",
            "nope",
        ]
        .iter()
        .map(|m: &&str| skip_reason(m))
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unsupported_breakdown_groups_and_sorts_by_impact() {
        let records: Vec<TestRecord> = vec![
            record(
                "draft7",
                "1",
                Some(true),
                &[(OURS, skipped("unsupported keyword \"if\" at #"))],
            ),
            record(
                "draft7",
                "1",
                Some(true),
                &[(OURS, skipped("unsupported keyword \"if\" at #"))],
            ),
            record(
                "draft2020-12",
                "4",
                Some(true),
                &[(OURS, skipped("unsupported keyword \"if\" at #/then"))],
            ),
            record(
                "draft7",
                "2",
                Some(true),
                &[(OURS, skipped("unsupported schema: boolean schema"))],
            ),
            record(
                "draft7",
                "3",
                Some(true),
                &[(OURS, skipped("unsupported schema: a"))],
            ),
            record("draft7", "9", Some(true), &[(OURS, Verdict::Valid)]),
        ];
        let expected: Vec<UnsupportedReason> = vec![
            UnsupportedReason {
                reason: "keyword \"if\"".to_string(),
                tests: 3,
                cases: 2,
                by_group: BTreeMap::from([
                    ("draft2020-12".to_string(), 1),
                    ("draft7".to_string(), 2),
                ]),
            },
            UnsupportedReason {
                reason: "schema: a".to_string(),
                tests: 1,
                cases: 1,
                by_group: BTreeMap::from([("draft7".to_string(), 1)]),
            },
            UnsupportedReason {
                reason: "schema: boolean schema".to_string(),
                tests: 1,
                cases: 1,
                by_group: BTreeMap::from([("draft7".to_string(), 1)]),
            },
        ];
        let actual: Vec<UnsupportedReason> = unsupported_breakdown(&records, OURS);
        assert_eq!(expected, actual);
    }

    #[test]
    fn consecutive_tests_of_a_case_are_grouped() {
        let records: Vec<TestRecord> = vec![
            record(
                "draft7",
                "1",
                Some(false),
                &[(OURS, Verdict::Valid), ("a", Verdict::Invalid)],
            ),
            record(
                "draft7",
                "1",
                Some(true),
                &[(OURS, Verdict::Invalid), ("a", Verdict::Valid)],
            ),
            record(
                "draft7",
                "1",
                Some(true),
                &[(OURS, Verdict::Valid), ("a", Verdict::Invalid)],
            ),
        ];
        let test = |expected: bool, ours: bool, majority: bool| -> DisagreementTest {
            DisagreementTest {
                test_description: "test".to_string(),
                expected: Some(expected),
                ours,
                majority: Some(majority),
                agreeing: 1,
                voters: 1,
            }
        };
        let case = |triage: Triage, tests: Vec<DisagreementTest>| -> DisagreementCase {
            DisagreementCase {
                group: "draft7".to_string(),
                file: Some("f.json".to_string()),
                optional: false,
                case_description: "case 1".to_string(),
                triage,
                tests,
            }
        };
        let expected: Vec<DisagreementCase> = vec![
            case(
                Triage::LikelyOurBug,
                vec![test(false, true, false), test(true, false, true)],
            ),
            case(
                Triage::CompetitorsDifferFromExpected,
                vec![test(true, true, false)],
            ),
        ];
        let actual: Vec<DisagreementCase> =
            our_findings(&records, OURS).disagreements_with_majority;
        assert_eq!(expected, actual);
    }

    #[test]
    fn triage_covers_every_combination() {
        let expected: Vec<Triage> = vec![
            Triage::NoExpectation,
            Triage::NoConsensus,
            Triage::LikelyOurBug,
            Triage::Ambiguous,
            Triage::CompetitorsDifferFromExpected,
            Triage::Agreement,
        ];
        let actual: Vec<Triage> = vec![
            Triage::classify(None, true, Some(false)),
            Triage::classify(Some(true), false, None),
            Triage::classify(Some(true), false, Some(true)),
            Triage::classify(Some(true), false, Some(false)),
            Triage::classify(Some(true), true, Some(false)),
            Triage::classify(Some(true), true, Some(true)),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn triage_labels() {
        let expected: Vec<&str> = vec![
            "likely our bug",
            "competitors differ from expected",
            "ambiguous",
            "no consensus",
            "no expectation",
            "agreement",
        ];
        let actual: Vec<&str> = [
            Triage::LikelyOurBug,
            Triage::CompetitorsDifferFromExpected,
            Triage::Ambiguous,
            Triage::NoConsensus,
            Triage::NoExpectation,
            Triage::Agreement,
        ]
        .iter()
        .map(|t: &Triage| t.label())
        .collect();
        assert_eq!(expected, actual);
    }
}
