//! `jsonschemars validate` subcommand: validate a JSON instance against a JSON Schema.
//!
//! The schema file is read from its `file:` URI, so its relative `$ref`s reach the files beside
//! it. Each `--resource` document is registered under its own `file:` URI and its root `$id`;
//! any other document a `$ref` names is retrieved from inside `--file-root` (or over HTTP with
//! `--allow-remote-refs`).
//!
//! Exit codes: `0` the payload conforms; `1` the payload does not conform (one line per error);
//! `2` no verdict could be reached: a schema document or the payload could not be read or
//! parsed (one `error:` line per problem), or the schema cannot be used (one `schema error:`
//! line per problem).

use super::CliFailure;
use super::retrieval::{CliRetriever, RefRetrieval};
use super::utils::{
    display_schema_path, read_payload_from_path, read_payload_from_reader, read_schema_from_path,
};
use json_schema_rs::{
    CompiledSchema, JsonSchemaSettings, Registry, RegistryBuilder, Schema, file_uri,
};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

/// Exit code when the payload does not conform to the schema.
const EXIT_INVALID: i32 = 1;
/// Exit code when no verdict could be reached (unreadable input, unusable schema).
const EXIT_NO_VERDICT: i32 = 2;

/// A failure that reached no verdict about the payload.
fn no_verdict(message: String) -> CliFailure {
    CliFailure {
        message,
        exit_code: EXIT_NO_VERDICT,
    }
}

/// A schema document read from the command line, with the `file:` URI it was read from (`None`
/// for stdin).
struct Document {
    schema: Schema,
    uri: Option<String>,
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
fn read_documents(
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
    match schema {
        Some(schema) if failed == 0 => Ok((schema, resources)),
        _ => Err(no_verdict(format!(
            "schema ingestion failed with {failed} error(s)"
        ))),
    }
}

pub(crate) fn run_validate(
    schema_path: &PathBuf,
    payload_path: Option<PathBuf>,
    resource_paths: &[PathBuf],
    schema_settings: &JsonSchemaSettings,
    retrieval: &RefRetrieval,
) -> Result<(), CliFailure> {
    let (schema, resources): (Document, Vec<Document>) =
        read_documents(schema_path, resource_paths, schema_settings)?;
    let instance: serde_json::Value = match payload_path {
        Some(p) => read_payload_from_path(&p),
        None => read_payload_from_reader(io::stdin()),
    }
    .map_err(no_verdict)?;
    let retriever: CliRetriever = CliRetriever::new(retrieval).map_err(no_verdict)?;

    let mut builder: RegistryBuilder<'_> = Registry::builder()
        .retriever(retriever)
        .settings(schema_settings.clone());
    if let Some(uri) = &schema.uri {
        builder = builder.base_uri(uri.clone());
    }
    // Resources are files (stdin is refused), so each has a `file:` URI.
    for resource in &resources {
        if let Some(uri) = &resource.uri {
            builder = builder.parsed_document(uri.clone(), &resource.schema);
        }
    }
    let registry: Registry<'_> = builder
        .build(&schema.schema)
        .map_err(|errors| schema_problems(errors.iter()))?;
    let compiled: CompiledSchema =
        CompiledSchema::new(&registry).map_err(|errors| schema_problems(errors.iter()))?;
    match compiled.validate(&instance) {
        Ok(()) => Ok(()),
        Err(errors) => {
            for e in &errors {
                eprintln!("{e}");
            }
            Err(CliFailure {
                message: format!("validation failed with {} error(s)", errors.len()),
                exit_code: EXIT_INVALID,
            })
        }
    }
}

/// Prints one `schema error:` line per problem and fails with no verdict.
fn schema_problems(problems: impl Iterator<Item = impl fmt::Display>) -> CliFailure {
    let mut count: usize = 0;
    for problem in problems {
        eprintln!("schema error: {problem}");
        count += 1;
    }
    no_verdict(format!("the schema cannot be used: {count} problem(s)"))
}
