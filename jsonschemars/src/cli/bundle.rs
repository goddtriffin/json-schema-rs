//! `jsonschemars bundle` subcommand: one schema document holding every document the schema
//! references (2020-12 core §9.3, see `Registry::bundle`).
//!
//! The schema and its `--resource`s are read as `validate` reads them; any other document a
//! `$ref` names is retrieved from inside `--file-root` (or over HTTP with
//! `--allow-remote-refs`). The bundle is written as pretty-printed JSON to `--output`, or to
//! stdout.
//!
//! Exit codes: `0` the bundle was written; `1` it could not be written; `2` there is no
//! bundle: a document could not be read or parsed (one `error:` line per problem), or the
//! schema cannot be used (one `schema error:` line per problem).

use super::CliFailure;
use super::documents::{Document, no_verdict, read_documents, schema_problems};
use super::retrieval::{CliRetriever, RefRetrieval};
use json_schema_rs::{JsonSchemaSettings, Registry, RegistryBuilder};
use std::path::{Path, PathBuf};

pub(crate) fn run_bundle(
    schema_path: &PathBuf,
    resource_paths: &[PathBuf],
    output: Option<&Path>,
    schema_settings: &JsonSchemaSettings,
    retrieval: &RefRetrieval,
) -> Result<(), CliFailure> {
    let (schema, resources): (Document, Vec<Document>) =
        read_documents(schema_path, resource_paths, schema_settings)?;
    let retriever: CliRetriever = CliRetriever::new(retrieval).map_err(no_verdict)?;
    let mut builder: RegistryBuilder<'_> = Registry::builder()
        .retriever(retriever)
        .settings(schema_settings.clone());
    if let Some(uri) = &schema.uri {
        builder = builder.base_uri(uri.clone());
    }
    for resource in &resources {
        if let Some(uri) = &resource.uri {
            builder = builder.parsed_document(uri.clone(), &resource.schema);
        }
    }
    let registry: Registry<'_> = builder
        .build(&schema.schema)
        .map_err(|errors| schema_problems(errors.iter()))?;
    let bundle: serde_json::Value = registry
        .bundle()
        .map_err(|e| no_verdict(format!("failed to build the bundle: {e}")))?;
    let mut json: String = serde_json::to_string_pretty(&bundle)
        .map_err(|e| no_verdict(format!("failed to serialize the bundle: {e}")))?;
    json.push('\n');
    if let Some(path) = output {
        std::fs::write(path, json)
            .map_err(|e| CliFailure::from(format!("failed to write {}: {e}", path.display())))
    } else {
        print!("{json}");
        Ok(())
    }
}
