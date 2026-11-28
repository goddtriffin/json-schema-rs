//! The official JSON Schema Test Suite as files on disk, and mapping Bowtie cases back to them.
//!
//! Bowtie reports carry no file paths, so every reported case is matched back to the suite
//! file it came from by its description and schema. This is what lets the report flag
//! optional-suite tests (files under `validation/tests/optional/`) and name the file of every
//! disagreement.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The suite's `compatibility` matching, shared with the library's suite gates.
#[path = "../../json_schema_rs/tests/suite_compatibility/mod.rs"]
mod compatibility;
pub use compatibility::compatible;

/// The release number the suite's `compatibility` property uses for one of [`crate::DRAFTS`].
#[must_use]
pub fn release(draft: &str) -> Option<u32> {
    match draft {
        "draft4" => Some(4),
        "draft6" => Some(6),
        "draft7" => Some(7),
        "draft2019-09" => Some(2019),
        "draft2020-12" => Some(2020),
        _ => None,
    }
}

/// Whether `bowtie suite` runs a case with these `tags` for `draft`: only when every tag is one
/// it expects for that dialect (`format-annotation` for 2019-09 and 2020-12, none before).
#[must_use]
pub fn bowtie_runs_tags(draft: &str, tags: &[String]) -> bool {
    let expected: &[&str] = match draft {
        "draft2019-09" | "draft2020-12" => &["format-annotation"],
        _ => &[],
    };
    tags.iter()
        .all(|tag: &String| expected.contains(&tag.as_str()))
}

/// One test of a suite file case (the suite's own on-disk shape).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SuiteFileTest {
    /// The test description.
    pub description: String,
    /// The instance being validated.
    pub data: Value,
    /// Whether the instance is valid under the case schema.
    pub valid: bool,
}

/// One case of a suite file (the suite's own on-disk shape).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SuiteFileCase {
    /// The case description.
    pub description: String,
    /// The case schema.
    pub schema: Value,
    /// The tests of the case.
    pub tests: Vec<SuiteFileTest>,
}

/// One suite file: its path relative to `validation/tests/` (with `/` separators) and the cases
/// of it that Bowtie runs for one draft.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteFile {
    /// Path relative to `validation/tests/`, e.g. `minimum.json` or `optional/bignum.json`.
    pub path: String,
    /// The cases of the file, in file order.
    pub cases: Vec<SuiteFileCase>,
}

/// Whether a suite path (relative to `validation/tests/`) belongs to the optional suite.
#[must_use]
pub fn is_optional_path(path: &str) -> bool {
    path.starts_with("optional/")
}

/// Lookup from (case description, schema) to the suite file the case lives in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SuiteIndex {
    entries: BTreeMap<String, Vec<(Value, String)>>,
}

impl SuiteIndex {
    /// Build an index over `files`. Files are indexed in path order so a case that appears
    /// verbatim in several files resolves to the lexicographically first path.
    #[must_use]
    pub fn new(files: &[SuiteFile]) -> Self {
        let mut sorted: Vec<&SuiteFile> = files.iter().collect();
        sorted.sort_by(|a: &&SuiteFile, b: &&SuiteFile| a.path.cmp(&b.path));
        let mut entries: BTreeMap<String, Vec<(Value, String)>> = BTreeMap::new();
        for file in sorted {
            for case in &file.cases {
                entries
                    .entry(case.description.clone())
                    .or_default()
                    .push((case.schema.clone(), file.path.clone()));
            }
        }
        SuiteIndex { entries }
    }

    /// The suite file containing a case with this description and schema, if any.
    ///
    /// A `$schema` keyword present in the reported schema but absent from the suite schema is
    /// ignored, so reports produced with `bowtie suite --set-schema` still resolve.
    #[must_use]
    pub fn locate(&self, description: &str, schema: &Value) -> Option<&str> {
        let candidates: &Vec<(Value, String)> = self.entries.get(description)?;
        if let Some((_, path)) = candidates
            .iter()
            .find(|(candidate, _): &&(Value, String)| candidate == schema)
        {
            return Some(path.as_str());
        }
        let Value::Object(object) = schema else {
            return None;
        };
        if !object.contains_key("$schema") {
            return None;
        }
        let mut stripped: serde_json::Map<String, Value> = object.clone();
        stripped.remove("$schema");
        let stripped: Value = Value::Object(stripped);
        candidates
            .iter()
            .find(|(candidate, _): &&(Value, String)| *candidate == stripped)
            .map(|(_, path): &(Value, String)| path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SuiteFile, SuiteFileCase, SuiteFileTest, SuiteIndex, bowtie_runs_tags, compatible,
        is_optional_path, release,
    };
    use serde_json::{Value, json};

    fn file(path: &str, description: &str, schema: Value) -> SuiteFile {
        SuiteFile {
            path: path.to_string(),
            cases: vec![SuiteFileCase {
                description: description.to_string(),
                schema,
                tests: vec![SuiteFileTest {
                    description: "t".to_string(),
                    data: json!(1),
                    valid: true,
                }],
            }],
        }
    }

    fn index() -> SuiteIndex {
        SuiteIndex::new(&[
            file(
                "optional/bignum.json",
                "integer",
                json!({"type": "integer"}),
            ),
            file("type.json", "integer", json!({"type": "integer", "x": 1})),
            file("b.json", "dup", json!(true)),
            file("a.json", "dup", json!(true)),
        ])
    }

    #[test]
    fn locates_by_description_and_schema() {
        let expected: Option<&str> = Some("type.json");
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("integer", &json!({"x": 1, "type": "integer"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn locates_optional_file() {
        let expected: Option<&str> = Some("optional/bignum.json");
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("integer", &json!({"type": "integer"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_cases_resolve_to_first_path() {
        let expected: Option<&str> = Some("a.json");
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("dup", &json!(true));
        assert_eq!(expected, actual);
    }

    #[test]
    fn ignores_added_dollar_schema() {
        let expected: Option<&str> = Some("optional/bignum.json");
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate(
            "integer",
            &json!({"$schema": "http://json-schema.org/draft-07/schema#", "type": "integer"}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_description_is_none() {
        let expected: Option<&str> = None;
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("nope", &json!(true));
        assert_eq!(expected, actual);
    }

    #[test]
    fn different_schema_is_none() {
        let expected: Option<&str> = None;
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("integer", &json!({"type": "string"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn different_schema_with_dollar_schema_is_none() {
        let expected: Option<&str> = None;
        let index: SuiteIndex = index();
        let actual: Option<&str> =
            index.locate("integer", &json!({"$schema": "x", "type": "string"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_object_schema_mismatch_is_none() {
        let expected: Option<&str> = None;
        let index: SuiteIndex = index();
        let actual: Option<&str> = index.locate("dup", &json!(false));
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_path_detection() {
        let expected: Vec<bool> = vec![true, true, false];
        let actual: Vec<bool> = [
            "optional/bignum.json",
            "optional/format/email.json",
            "type.json",
        ]
        .iter()
        .map(|p: &&str| is_optional_path(p))
        .collect();
        assert_eq!(expected, actual);
    }

    const RELEASES: [u32; 5] = [4, 6, 7, 2019, 2020];

    // --- compatibility ---

    fn admitted(compatibility: Option<&str>) -> Vec<u32> {
        RELEASES
            .into_iter()
            .filter(|&release| compatible(compatibility, release).expect("valid compatibility"))
            .collect()
    }

    #[test]
    fn absent_admits_every_release() {
        let expected: Vec<u32> = vec![4, 6, 7, 2019, 2020];
        let actual: Vec<u32> = admitted(None);
        assert_eq!(expected, actual);
    }

    #[test]
    fn bare_number_admits_it_and_later() {
        let expected: Vec<u32> = vec![7, 2019, 2020];
        let actual: Vec<u32> = admitted(Some("7"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn at_most_admits_it_and_earlier() {
        let expected: Vec<u32> = vec![4, 6, 7, 2019];
        let actual: Vec<u32> = admitted(Some("<=2019"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn equals_admits_only_it() {
        let expected: Vec<u32> = vec![2020];
        let actual: Vec<u32> = admitted(Some("=2020"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn range_admits_between_bounds() {
        let expected: Vec<u32> = vec![6, 7];
        let actual: Vec<u32> = admitted(Some("6,<=7"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn range_from_an_older_release_admits_up_to_its_end() {
        let expected: Vec<u32> = vec![4];
        let actual: Vec<u32> = admitted(Some("3,<=4"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn disjoint_ranges_admit_either() {
        let expected: Vec<u32> = vec![7, 2019, 2020];
        let actual: Vec<u32> = admitted(Some("<=3,7"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_release_plus_open_range_admits_both() {
        let expected: Vec<u32> = vec![4, 6, 2020];
        let actual: Vec<u32> = admitted(Some("4,<=6,2020"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unreleased_placeholder_admits_none() {
        let expected: Vec<u32> = vec![];
        let actual: Vec<u32> = admitted(Some("9999"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_numeric_bound_is_an_error() {
        let expected: bool = true;
        let actual: bool = compatible(Some("<=next"), 2020).is_err();
        assert_eq!(expected, actual);
    }

    // --- drafts and tags ---

    #[test]
    fn every_draft_has_a_release() {
        let expected: Vec<Option<u32>> = vec![Some(4), Some(6), Some(7), Some(2019), Some(2020)];
        let actual: Vec<Option<u32>> = crate::DRAFTS.iter().map(|d: &&str| release(d)).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bowtie_runs_format_annotation_only_from_2019_09() {
        let tags: Vec<String> = vec!["format-annotation".to_string()];
        let expected: Vec<bool> = vec![false, false, false, true, true];
        let actual: Vec<bool> = crate::DRAFTS
            .iter()
            .map(|d: &&str| bowtie_runs_tags(d, &tags))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bowtie_never_runs_format_assertion() {
        let tags: Vec<String> = vec!["format-assertion".to_string()];
        let expected: Vec<bool> = vec![false, false, false, false, false];
        let actual: Vec<bool> = crate::DRAFTS
            .iter()
            .map(|d: &&str| bowtie_runs_tags(d, &tags))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bowtie_runs_untagged_everywhere() {
        let expected: Vec<bool> = vec![true, true, true, true, true];
        let actual: Vec<bool> = crate::DRAFTS
            .iter()
            .map(|d: &&str| bowtie_runs_tags(d, &[]))
            .collect();
        assert_eq!(expected, actual);
    }
}
