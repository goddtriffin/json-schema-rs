//! The official JSON Schema Test Suite as files on disk, and mapping Bowtie cases back to them.
//!
//! Bowtie reports carry no file paths, so every reported case is matched back to the suite
//! file it came from by its description and schema. This is what lets the report flag
//! optional-suite tests (files under `tests/<draft>/optional/`) and name the file of every
//! disagreement.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

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

/// One suite file: its path relative to `tests/<draft>/` (with `/` separators) and its cases.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteFile {
    /// Path relative to the draft directory, e.g. `minimum.json` or `optional/bignum.json`.
    pub path: String,
    /// The cases of the file, in file order.
    pub cases: Vec<SuiteFileCase>,
}

/// Whether a draft-relative suite path belongs to the optional suite.
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
    use super::{SuiteFile, SuiteFileCase, SuiteFileTest, SuiteIndex, is_optional_path};
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
}
