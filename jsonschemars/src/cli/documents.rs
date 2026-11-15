//! Reading the schema documents a subcommand works on: the schema and its `--resource`s, each
//! with the `file:` URI it was read from, and reporting the problems that leave no result.

use super::CliFailure;
use super::utils::{display_schema_path, print_dialect_warnings, read_schema_from_path};
use json_schema_rs::{JsonSchemaSettings, Schema, file_uri};
use std::fmt;
use std::path::{Path, PathBuf};

/// Exit code when there is no result (unreadable input, unusable schema).
const EXIT_NO_VERDICT: i32 = 2;

/// A failure that leaves no result: no verdict about a payload, no bundle.
pub(crate) fn no_verdict(message: String) -> CliFailure {
    CliFailure {
        message,
        exit_code: EXIT_NO_VERDICT,
    }
}

/// A schema document read from the command line, with the `file:` URI it was read from (`None`
/// for stdin).
pub(crate) struct Document {
    pub(crate) schema: Schema,
    pub(crate) uri: Option<String>,
}

/// Reads one schema document, which is retrieved from the `file:` URI of its canonical path.
fn read_document(path: &PathBuf, settings: &JsonSchemaSettings) -> Result<Document, String> {
    let schema: Schema = read_schema_from_path(path, settings)?;
    let uri: Option<String> = if path.as_os_str() == "-" {
        None
    } else {
        let canonical: PathBuf = path
            .canonicalize()
            .map_err(|e| format!("failed to resolve the schema file path: {e}"))?;
        Some(file_uri(&canonical).ok_or_else(|| {
            format!(
                "{} has no file: URI (its path is not UTF-8)",
                canonical.display()
            )
        })?)
    };
    Ok(Document { schema, uri })
}

/// Reads the schema and every resource, reporting every document that cannot be read or parsed
/// (one `error: <path>: <problem>` line each) before failing.
pub(crate) fn read_documents(
    schema_path: &PathBuf,
    resource_paths: &[PathBuf],
    settings: &JsonSchemaSettings,
) -> Result<(Document, Vec<Document>), CliFailure> {
    let mut failed: usize = 0;
    let mut report = |path: &Path, error: &str| {
        for line in error.lines() {
            eprintln!("error: {}: {line}", display_schema_path(path));
        }
        failed += 1;
    };
    let schema: Option<Document> = read_document(schema_path, settings)
        .map_err(|e| report(schema_path, &e))
        .ok();
    let mut resources: Vec<Document> = Vec::with_capacity(resource_paths.len());
    for path in resource_paths {
        let document: Result<Document, String> = if path.as_os_str() == "-" {
            Err("a resource must be a file; stdin (\"-\") is not accepted".to_string())
        } else {
            read_document(path, settings)
        };
        match document {
            Ok(document) => resources.push(document),
            Err(e) => report(path, &e),
        }
    }
    if let Some(schema) = &schema {
        let documents: Vec<(&Path, &Schema, Option<&str>)> =
            std::iter::once((schema_path.as_path(), schema))
                .chain(
                    resource_paths
                        .iter()
                        .map(PathBuf::as_path)
                        .zip(resources.iter()),
                )
                .map(|(path, document)| (path, &document.schema, document.uri.as_deref()))
                .collect();
        print_dialect_warnings(&documents, settings);
    }
    match schema {
        Some(schema) if failed == 0 => Ok((schema, resources)),
        _ => Err(no_verdict(format!(
            "schema ingestion failed with {failed} error(s)"
        ))),
    }
}

/// Prints one `schema error:` line per problem and fails with no verdict.
pub(crate) fn schema_problems(problems: impl Iterator<Item = impl fmt::Display>) -> CliFailure {
    let mut count: usize = 0;
    for problem in problems {
        eprintln!("schema error: {problem}");
        count += 1;
    }
    no_verdict(format!("the schema cannot be used: {count} problem(s)"))
}
