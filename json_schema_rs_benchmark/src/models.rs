//! Names shared by the reverse-codegen models' generator (`isolated/models/gen`), the
//! build-failure triage (`src/bin/model_build_failures.rs`) and the driver.
//!
//! The `reverse-codegen` build of `isolated/` compiles one model per (tool, fixture): the Rust
//! types our codegen generates for the fixture's schema, carrying that tool's derive. A derive
//! that cannot handle a model fails the whole build, so the driver checks it with
//! `--message-format=json`, and on failure [`build_failures`] attributes each compiler error to
//! the model whose file it is in. The driver appends those to the exclusion file named by
//! [`EXCLUDE_ENV`] and builds again; the generator leaves an excluded model out and records the
//! reason as that cell's compatibility failure.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Component, Path};

/// Environment variable naming the exclusion file: one `tool\tfixture\treason` row per model
/// left out of the build.
pub const EXCLUDE_ENV: &str = "BENCH_MODEL_EXCLUDE";

/// Directory (under the build script's `OUT_DIR`) holding every model, as
/// `<tool>/<model dir name>/model.rs`.
pub const MODELS_DIR_NAME: &str = "models";

/// File name of one model's source.
pub const MODEL_FILE_NAME: &str = "model.rs";

/// The fixture id of the CLI tier's startup baseline, whose model is built in.
pub const STARTUP_FIXTURE_ID: &str = "startup";

/// The startup baseline's schema (see `STARTUP_FIXTURE` in the report).
pub const STARTUP_SCHEMA: &str = r#"{"$schema": "http://json-schema.org/draft-07/schema#", "type": "object", "properties": {"name": {"type": "string"}}}"#;

/// Fixtures whose models are not built, and why: our codegen's types for them take too long to
/// build in an optimized profile. No copy of their model is written, so every reverse-codegen and
/// generated-types cell of theirs is a compatibility failure with the reason as its detail
/// (prefixed `no model: `). A fix removes its fixture from the list.
pub const MODELS_NOT_BUILT: &[(&str, &str)] = &[
    (
        "generated/wide-object",
        "its types take minutes to build (#102)",
    ),
    (
        "medium/package-json",
        "its types do not finish an optimized build (#111)",
    ),
];

/// Why `fixture_id`'s models are not built (see [`MODELS_NOT_BUILT`]).
#[must_use]
pub fn model_not_built(fixture_id: &str) -> Option<&'static str> {
    MODELS_NOT_BUILT
        .iter()
        .find(|(id, _reason): &&(&str, &str)| *id == fixture_id)
        .map(|(_id, reason): &(&str, &str)| *reason)
}

/// The directory name of `fixture_id`'s model (`/` becomes `__`).
#[must_use]
pub fn model_dir_name(fixture_id: &str) -> String {
    fixture_id.replace('/', "__")
}

/// The fixture id a model directory name stands for (the inverse of [`model_dir_name`]).
#[must_use]
pub fn fixture_id_of(dir_name: &str) -> String {
    dir_name.replace("__", "/")
}

/// A model left out of the build, and why.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Exclusion {
    /// Id of the tool whose derive the model carries.
    pub tool: String,
    /// Id of the fixture the model was generated from.
    pub fixture: String,
    /// One-line reason, reported as the cell's compatibility failure.
    pub reason: String,
}

/// Parse an exclusion file (`tool\tfixture\treason` rows; blank lines ignored).
///
/// # Errors
///
/// Returns the offending line when a row does not have three fields.
pub fn parse_exclusions(text: &str) -> Result<Vec<Exclusion>, String> {
    text.lines()
        .filter(|line: &&str| !line.trim().is_empty())
        .map(|line: &str| {
            let mut fields = line.splitn(3, '\t');
            match (fields.next(), fields.next(), fields.next()) {
                (Some(tool), Some(fixture), Some(reason)) => Ok(Exclusion {
                    tool: tool.to_owned(),
                    fixture: fixture.to_owned(),
                    reason: reason.to_owned(),
                }),
                _ => Err(format!("malformed exclusion row: {line:?}")),
            }
        })
        .collect()
}

/// The models a failed `cargo check --message-format=json` cannot compile: one [`Exclusion`]
/// per (tool, fixture) with a compiler error in its model file, its reason the first such error.
///
/// # Errors
///
/// Returns a description when an error lies outside every model (the build is broken for
/// another reason, which excluding models cannot fix), or when no error names a model at all.
pub fn build_failures(messages: &str) -> Result<Vec<Exclusion>, String> {
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut failures: Vec<Exclusion> = Vec::new();
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-message" || message["message"]["level"] != "error" {
            continue;
        }
        let diagnostic: &Value = &message["message"];
        let text: String = one_line(diagnostic["message"].as_str().unwrap_or("compiler error"));
        let Some(file) = primary_file(diagnostic) else {
            // A summary without a location ("aborting due to N previous errors").
            continue;
        };
        let Some((tool, fixture)) = model_of(Path::new(file)) else {
            return Err(format!(
                "build error outside every model, in {file}: {text}"
            ));
        };
        if seen.insert((tool.clone(), fixture.clone())) {
            failures.push(Exclusion {
                tool,
                fixture,
                reason: format!("model does not compile: {text}"),
            });
        }
    }
    if failures.is_empty() {
        return Err("the build failed without a compiler error in any model".to_owned());
    }
    Ok(failures)
}

/// The file of a diagnostic's primary span.
fn primary_file(diagnostic: &Value) -> Option<&str> {
    diagnostic["spans"]
        .as_array()?
        .iter()
        .find(|span: &&Value| span["is_primary"] == true)
        .and_then(|span: &Value| span["file_name"].as_str())
}

/// The (tool, fixture) of a model file: `.../models/<tool>/<model dir name>/model.rs`.
fn model_of(file: &Path) -> Option<(String, String)> {
    let names: Vec<&str> = file
        .components()
        .filter_map(|component: Component<'_>| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    let at: usize = names
        .iter()
        .rposition(|name: &&str| *name == MODELS_DIR_NAME)?;
    match names.get(at + 1..) {
        Some([tool, dir, _file]) => Some(((*tool).to_owned(), fixture_id_of(dir))),
        _ => None,
    }
}

/// Collapse whitespace runs to single spaces so a reason fits a TSV cell.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{
        Exclusion, build_failures, fixture_id_of, model_dir_name, model_not_built, parse_exclusions,
    };

    #[test]
    fn model_not_built_gives_a_listed_fixtures_reason() {
        let expected: Option<&str> = Some("its types do not finish an optimized build (#111)");
        let actual: Option<&str> = model_not_built("medium/package-json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn model_not_built_is_none_for_other_fixtures() {
        let expected: Option<&str> = None;
        let actual: Option<&str> = model_not_built("small/config");
        assert_eq!(expected, actual);
    }

    /// One cargo `compiler-message` line: an error with a primary span in `file`.
    fn error_in(file: &str, message: &str) -> String {
        serde_json::json!({
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "message": message,
                "spans": [{"file_name": file, "is_primary": true}],
            },
        })
        .to_string()
    }

    fn exclusion(tool: &str, fixture: &str, reason: &str) -> Exclusion {
        Exclusion {
            tool: tool.to_owned(),
            fixture: fixture.to_owned(),
            reason: reason.to_owned(),
        }
    }

    #[test]
    fn model_dir_name_round_trips_a_fixture_id() {
        let expected: String = "small/config".to_owned();
        let actual: String = fixture_id_of(&model_dir_name("small/config"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_exclusions_reads_rows_and_skips_blank_lines() {
        let expected: Result<Vec<Exclusion>, String> = Ok(vec![
            exclusion("schemars", "small/config", "model does not compile: a\tb"),
            exclusion("jsonschemars", "large/x", "r"),
        ]);
        let actual: Result<Vec<Exclusion>, String> = parse_exclusions(
            "schemars\tsmall/config\tmodel does not compile: a\tb\n\njsonschemars\tlarge/x\tr\n",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_exclusions_rejects_a_row_without_a_reason() {
        let expected: Result<Vec<Exclusion>, String> =
            Err("malformed exclusion row: \"schemars\\tsmall/config\"".to_owned());
        let actual: Result<Vec<Exclusion>, String> = parse_exclusions("schemars\tsmall/config\n");
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_failures_attributes_the_first_error_of_each_model() {
        let messages: String = [
            r#"{"reason":"compiler-artifact"}"#.to_owned(),
            error_in(
                "/t/out/models/schemars/small__config/model.rs",
                "the trait bound `X: JsonSchema` is not satisfied",
            ),
            error_in(
                "/t/out/models/schemars/small__config/model.rs",
                "second error",
            ),
            error_in(
                "/t/out/models/jsonschemars/large__api-catalog/model.rs",
                "cannot find\n  type `Y`",
            ),
            r#"{"reason":"compiler-message","message":{"level":"error","message":"aborting due to 3 previous errors","spans":[]}}"#.to_owned(),
            r#"{"reason":"build-finished","success":false}"#.to_owned(),
        ]
        .join("\n");
        let expected: Result<Vec<Exclusion>, String> = Ok(vec![
            exclusion(
                "schemars",
                "small/config",
                "model does not compile: the trait bound `X: JsonSchema` is not satisfied",
            ),
            exclusion(
                "jsonschemars",
                "large/api-catalog",
                "model does not compile: cannot find type `Y`",
            ),
        ]);
        let actual: Result<Vec<Exclusion>, String> = build_failures(&messages);
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_failures_ignores_warnings() {
        let warning: String = serde_json::json!({
            "reason": "compiler-message",
            "message": {
                "level": "warning",
                "message": "unused",
                "spans": [{"file_name": "/t/src/lib.rs", "is_primary": true}],
            },
        })
        .to_string();
        let messages: String = [
            warning,
            error_in("/t/out/models/schemars/small__config/model.rs", "e"),
        ]
        .join("\n");
        let expected: Result<Vec<Exclusion>, String> = Ok(vec![exclusion(
            "schemars",
            "small/config",
            "model does not compile: e",
        )]);
        let actual: Result<Vec<Exclusion>, String> = build_failures(&messages);
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_failures_rejects_an_error_outside_every_model() {
        let expected: Result<Vec<Exclusion>, String> =
            Err("build error outside every model, in /t/src/lib.rs: mismatched types".to_owned());
        let actual: Result<Vec<Exclusion>, String> =
            build_failures(&error_in("/t/src/lib.rs", "mismatched types"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_failures_rejects_a_failed_build_without_model_errors() {
        let expected: Result<Vec<Exclusion>, String> =
            Err("the build failed without a compiler error in any model".to_owned());
        let actual: Result<Vec<Exclusion>, String> =
            build_failures(r#"{"reason":"build-finished","success":false}"#);
        assert_eq!(expected, actual);
    }
}
