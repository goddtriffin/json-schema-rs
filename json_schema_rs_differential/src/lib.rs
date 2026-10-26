//! Differential testing of json-schema-rs against competitor JSON Schema validators.
//!
//! Bowtie runs our harness (`bowtie_harness`) alongside competitor implementations against the
//! official JSON Schema Test Suite and the benchmark fixtures, writing JSONL reports. This
//! library turns those reports into a comparison: per-draft compliance for every
//! implementation, our coverage (tests run vs. skipped as unsupported) separated from our
//! correctness (tests run that match the expectation), the unsupported-feature breakdown,
//! every test where we disagree with the competitor majority or the expectation, the fixture
//! verdict grid, and an in-process side-by-side of error messages.
//!
//! Module layout: [`report`] parses Bowtie reports, [`suite`] maps cases back to suite files,
//! [`analysis`] holds the pure aggregation logic, [`messages`] runs the in-process message
//! comparison, [`markdown`] renders `results.md`, and [`load`] performs all file I/O.

pub mod analysis;
pub mod error;
pub mod load;
pub mod markdown;
pub mod messages;
pub mod report;
pub mod suite;

pub use error::DifferentialError;

use analysis::{
    ImplementationStats, OurFindings, OurStats, SectionReport, Tally, TestRecord,
    UnsupportedReason, fixture_records, our_findings, section_report, suite_records,
    unsupported_breakdown,
};
use messages::MessageCase;
use report::{BowtieReport, ImplementationInfo};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use suite::SuiteIndex;

/// The drafts the suite comparison covers, in report order (directory names under `tests/`).
pub const DRAFTS: [&str; 5] = ["draft4", "draft6", "draft7", "draft2019-09", "draft2020-12"];

/// The suite inputs for one draft.
#[derive(Debug, Clone)]
pub struct SuiteInput {
    /// Draft directory name, e.g. `draft2020-12`.
    pub draft: String,
    /// The Bowtie report for the draft's (non-optional) suite.
    pub report: BowtieReport,
    /// The Bowtie report for the draft's optional suite, when the driver produced one.
    pub optional_report: Option<BowtieReport>,
    /// Index of the draft's suite files, for mapping cases back to files.
    pub index: SuiteIndex,
}

/// The fixture inputs for one benchmark fixture.
#[derive(Debug, Clone)]
pub struct FixtureInput {
    /// Fixture id, e.g. `small/config`.
    pub id: String,
    /// The `bowtie validate --expect valid` report and the manifest's valid instance paths.
    pub valid: (BowtieReport, Vec<String>),
    /// The `bowtie validate --expect invalid` report and the manifest's invalid instance paths.
    pub invalid: (BowtieReport, Vec<String>),
}

/// The whole differential report (serialized as `results.json`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DifferentialReport {
    /// Our implementation id in the Bowtie reports.
    pub ours: String,
    /// Metadata for every implementation seen in any report.
    pub implementations: BTreeMap<String, ImplementationInfo>,
    /// Headline numbers.
    pub summary: Summary,
    /// Per-draft details, in [`DRAFTS`] order.
    pub drafts: Vec<DraftReport>,
    /// Why we skipped suite tests, most impactful first.
    pub unsupported: Vec<UnsupportedReason>,
    /// Per-fixture details, in manifest order.
    pub fixtures: Vec<FixtureReport>,
    /// Why we skipped fixture instances, most impactful first.
    pub fixture_unsupported: Vec<UnsupportedReason>,
    /// The error-message side-by-side.
    pub messages: Vec<MessageCase>,
}

/// Headline numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Our numbers per draft (non-optional suite).
    pub drafts: Vec<DraftSummary>,
    /// Every implementation's compliance over the non-optional suite of all drafts.
    pub suite: BTreeMap<String, ImplementationStats>,
    /// Every implementation's compliance over the optional suite of all drafts, when present.
    pub optional_suite: Option<BTreeMap<String, ImplementationStats>>,
    /// Every implementation's agreement with the manifest over all fixtures.
    pub fixtures: BTreeMap<String, ImplementationStats>,
}

/// Our numbers for one draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftSummary {
    /// Draft directory name.
    pub draft: String,
    /// Our stats over the draft's non-optional suite.
    pub ours: OurStats,
}

/// Details for one draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftReport {
    /// Draft directory name.
    pub draft: String,
    /// The dialect URI Bowtie ran.
    pub dialect: String,
    /// Whether any of the draft's Bowtie runs stopped early.
    pub did_fail_fast: bool,
    /// Cases that could not be mapped back to a suite file.
    pub unmapped_cases: u64,
    /// Non-optional suite numbers.
    pub required: SectionReport,
    /// Optional suite numbers, when any optional tests were reported.
    pub optional: Option<SectionReport>,
    /// Our disagreements and errors (optional tests included, flagged per entry).
    pub findings: OurFindings,
}

/// Details for one benchmark fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FixtureReport {
    /// Fixture id.
    pub id: String,
    /// Agreement with the manifest per implementation.
    pub section: SectionReport,
    /// Every instance with every implementation's verdict.
    pub instances: Vec<TestRecord>,
    /// Our disagreements and errors.
    pub findings: OurFindings,
}

/// Build the differential report from loaded inputs.
///
/// # Errors
///
/// Returns [`DifferentialError::Fixture`] when a fixture report does not match the manifest's
/// instance lists.
pub fn build_report(
    ours: &str,
    suites: &[SuiteInput],
    fixtures: &[FixtureInput],
    messages: Vec<MessageCase>,
) -> Result<DifferentialReport, DifferentialError> {
    let mut implementations: BTreeMap<String, ImplementationInfo> = BTreeMap::new();
    let mut drafts: Vec<DraftReport> = Vec::with_capacity(suites.len());
    let mut all_suite_records: Vec<TestRecord> = Vec::new();
    for input in suites {
        collect_implementations(&mut implementations, &input.report);
        if let Some(optional) = &input.optional_report {
            collect_implementations(&mut implementations, optional);
        }
        let (draft, records): (DraftReport, Vec<TestRecord>) = draft_report(ours, input);
        drafts.push(draft);
        all_suite_records.extend(records);
    }

    let mut fixture_reports: Vec<FixtureReport> = Vec::with_capacity(fixtures.len());
    let mut all_fixture_records: Vec<TestRecord> = Vec::new();
    for input in fixtures {
        collect_implementations(&mut implementations, &input.valid.0);
        collect_implementations(&mut implementations, &input.invalid.0);
        let (report, records): (FixtureReport, Vec<TestRecord>) = fixture_report(ours, input)?;
        fixture_reports.push(report);
        all_fixture_records.extend(records);
    }

    let summary: Summary = Summary {
        drafts: drafts
            .iter()
            .map(|d: &DraftReport| DraftSummary {
                draft: d.draft.clone(),
                ours: d.required.ours.clone(),
            })
            .collect(),
        suite: merge_sections(drafts.iter().map(|d: &DraftReport| &d.required)),
        optional_suite: if drafts.iter().any(|d: &DraftReport| d.optional.is_some()) {
            Some(merge_sections(
                drafts
                    .iter()
                    .filter_map(|d: &DraftReport| d.optional.as_ref()),
            ))
        } else {
            None
        },
        fixtures: merge_sections(fixture_reports.iter().map(|f: &FixtureReport| &f.section)),
    };

    Ok(DifferentialReport {
        ours: ours.to_string(),
        implementations,
        summary,
        drafts,
        unsupported: unsupported_breakdown(&all_suite_records, ours),
        fixtures: fixture_reports,
        fixture_unsupported: unsupported_breakdown(&all_fixture_records, ours),
        messages,
    })
}

fn collect_implementations(into: &mut BTreeMap<String, ImplementationInfo>, report: &BowtieReport) {
    for (id, info) in &report.implementations {
        into.entry(id.clone()).or_insert_with(|| info.clone());
    }
}

fn draft_report(ours: &str, input: &SuiteInput) -> (DraftReport, Vec<TestRecord>) {
    let mut records: Vec<TestRecord> =
        suite_records(&input.draft, &input.report, &input.index, false);
    let mut did_fail_fast: bool = input.report.did_fail_fast;
    if let Some(optional) = &input.optional_report {
        records.extend(suite_records(&input.draft, optional, &input.index, true));
        did_fail_fast |= optional.did_fail_fast;
    }
    // Seqs are unique per report only, so the optional flag (one report each) is part of the key.
    let mut unmapped: BTreeSet<(bool, &str)> = BTreeSet::new();
    for record in &records {
        if record.file.is_none() {
            unmapped.insert((record.optional, record.case_seq.as_str()));
        }
    }
    let required: Vec<&TestRecord> = records
        .iter()
        .filter(|r: &&TestRecord| !r.optional)
        .collect();
    let optional: Vec<&TestRecord> = records
        .iter()
        .filter(|r: &&TestRecord| r.optional)
        .collect();
    let report: DraftReport = DraftReport {
        draft: input.draft.clone(),
        dialect: input.report.dialect.clone(),
        did_fail_fast,
        unmapped_cases: u64::try_from(unmapped.len()).unwrap_or(u64::MAX),
        required: section_report(&required, ours),
        optional: if optional.is_empty() {
            None
        } else {
            Some(section_report(&optional, ours))
        },
        findings: our_findings(&records, ours),
    };
    (report, records)
}

fn fixture_report(
    ours: &str,
    input: &FixtureInput,
) -> Result<(FixtureReport, Vec<TestRecord>), DifferentialError> {
    let to_error = |message: String| DifferentialError::Fixture {
        id: input.id.clone(),
        message,
    };
    let mut records: Vec<TestRecord> =
        fixture_records(&input.id, &input.valid.0, true, &input.valid.1).map_err(to_error)?;
    records.extend(
        fixture_records(&input.id, &input.invalid.0, false, &input.invalid.1).map_err(to_error)?,
    );
    let refs: Vec<&TestRecord> = records.iter().collect();
    let report: FixtureReport = FixtureReport {
        id: input.id.clone(),
        section: section_report(&refs, ours),
        findings: our_findings(&records, ours),
        instances: records.clone(),
    };
    Ok((report, records))
}

fn merge_sections<'s, I>(sections: I) -> BTreeMap<String, ImplementationStats>
where
    I: Iterator<Item = &'s SectionReport>,
{
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for section in sections {
        for (id, stats) in &section.implementations {
            tallies.entry(id.clone()).or_default().merge(&stats.tally);
        }
    }
    tallies
        .into_iter()
        .map(|(id, tally): (String, Tally)| (id, ImplementationStats::from_tally(tally)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DifferentialError, DifferentialReport, FixtureInput, SuiteInput, build_report};
    use crate::analysis::{ImplementationStats, Tally};
    use crate::report::{BowtieReport, parse_report};
    use crate::suite::SuiteIndex;
    use std::collections::BTreeMap;

    const SUITE: &str = r#"{"implementations": {"ours": {"name": "json-schema-rs", "language": "rust"}, "a": {"name": "a", "language": "rust", "version": "1"}}, "dialect": "D", "bowtie_version": "x", "started": "t"}
{"seq": 1, "case": {"description": "c", "schema": {}, "tests": [{"description": "t1", "instance": 1, "valid": true}, {"description": "t2", "instance": 2, "valid": false}]}}
{"seq": 1, "implementation": "ours", "expected": [true, false], "results": [{"valid": true}, {"valid": true}]}
{"seq": 1, "implementation": "a", "expected": [true, false], "results": [{"valid": true}, {"valid": false}]}
{"did_fail_fast": false}"#;

    const OPTIONAL: &str = r#"{"implementations": {"ours": {"name": "json-schema-rs", "language": "rust"}, "a": {"name": "a", "language": "rust"}}, "dialect": "D", "bowtie_version": "x", "started": "t"}
{"seq": 1, "case": {"description": "o", "schema": {}, "tests": [{"description": "t", "instance": 1, "valid": true}]}}
{"seq": 1, "implementation": "ours", "expected": [true], "skipped": true, "message": "unsupported: bignum"}
{"seq": 1, "implementation": "a", "expected": [true], "results": [{"valid": true}]}
{"did_fail_fast": true}"#;

    const FIXTURE_VALID: &str = r#"{"implementations": {"ours": {"name": "json-schema-rs", "language": "rust"}, "a": {"name": "a", "language": "rust"}}, "dialect": "D", "bowtie_version": "x", "started": "t"}
{"seq": 1, "case": {"description": "bowtie validate", "schema": {}, "tests": [{"description": "", "instance": 1, "valid": true}]}}
{"seq": 1, "implementation": "ours", "expected": [true], "results": [{"valid": true}]}
{"seq": 1, "implementation": "a", "expected": [true], "results": [{"valid": true}]}
{"did_fail_fast": false}"#;

    fn parse(text: &str) -> BowtieReport {
        parse_report(text).expect("valid report")
    }

    fn suite_input(optional: bool) -> SuiteInput {
        SuiteInput {
            draft: "draft7".to_string(),
            report: parse(SUITE),
            optional_report: if optional {
                Some(parse(OPTIONAL))
            } else {
                None
            },
            index: SuiteIndex::default(),
        }
    }

    fn fixture_input(invalid_paths: &[&str]) -> FixtureInput {
        FixtureInput {
            id: "small/x".to_string(),
            valid: (parse(FIXTURE_VALID), vec!["v.json".to_string()]),
            invalid: (
                parse(&FIXTURE_VALID.replace("\"valid\": true}]}}", "\"valid\": false}]}}")),
                invalid_paths
                    .iter()
                    .map(|p: &&str| (*p).to_string())
                    .collect(),
            ),
        }
    }

    fn stats(passed: u64, failed: u64, skipped: u64, total: u64) -> ImplementationStats {
        ImplementationStats::from_tally(Tally {
            total,
            passed,
            failed,
            skipped,
            errored: 0,
            unexpected: 0,
        })
    }

    #[test]
    fn summary_suite_merges_drafts() {
        let report: DifferentialReport = build_report(
            "ours",
            &[suite_input(false), suite_input(false)],
            &[],
            Vec::new(),
        )
        .expect("builds");
        let expected: BTreeMap<String, ImplementationStats> = BTreeMap::from([
            ("a".to_string(), stats(4, 0, 0, 4)),
            ("ours".to_string(), stats(2, 2, 0, 4)),
        ]);
        let actual: BTreeMap<String, ImplementationStats> = report.summary.suite;
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_report_is_separate_section() {
        let report: DifferentialReport =
            build_report("ours", &[suite_input(true)], &[], Vec::new()).expect("builds");
        let expected: Option<BTreeMap<String, ImplementationStats>> = Some(BTreeMap::from([
            ("a".to_string(), stats(1, 0, 0, 1)),
            ("ours".to_string(), stats(0, 0, 1, 1)),
        ]));
        let actual: Option<BTreeMap<String, ImplementationStats>> = report.summary.optional_suite;
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_fail_fast_propagates_to_draft() {
        let report: DifferentialReport =
            build_report("ours", &[suite_input(true)], &[], Vec::new()).expect("builds");
        let expected: (bool, u64) = (true, 2);
        let actual: (bool, u64) = (
            report.drafts[0].did_fail_fast,
            report.drafts[0].unmapped_cases,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_optional_report_means_no_optional_summary() {
        let report: DifferentialReport =
            build_report("ours", &[suite_input(false)], &[], Vec::new()).expect("builds");
        let expected: Option<BTreeMap<String, ImplementationStats>> = None;
        let actual: Option<BTreeMap<String, ImplementationStats>> = report.summary.optional_suite;
        assert_eq!(expected, actual);
    }

    #[test]
    fn fixtures_summary_counts_both_reports() {
        let report: DifferentialReport =
            build_report("ours", &[], &[fixture_input(&["i.json"])], Vec::new()).expect("builds");
        let expected: BTreeMap<String, ImplementationStats> = BTreeMap::from([
            ("a".to_string(), stats(1, 1, 0, 2)),
            ("ours".to_string(), stats(1, 1, 0, 2)),
        ]);
        let actual: BTreeMap<String, ImplementationStats> = report.summary.fixtures;
        assert_eq!(expected, actual);
    }

    #[test]
    fn fixture_mismatch_is_an_error() {
        let actual: Result<DifferentialReport, DifferentialError> = build_report(
            "ours",
            &[],
            &[fixture_input(&["i.json", "j.json"])],
            Vec::new(),
        );
        let expected: String =
            "fixture \"small/x\": manifest lists 2 invalid instances but the report has 1 tests"
                .to_string();
        let actual: String = actual.expect_err("mismatch").to_string();
        assert_eq!(expected, actual);
    }
}
