//! `jsonschemars bundle`, `jsonschemars dereference` and `jsonschemars canonicalize`: one
//! schema document made from the schema. `bundle` embeds every document it references
//! (2020-12 core §9.3, see `Registry::bundle`); `dereference` replaces every reference by its
//! target (see `Registry::dereference`); `canonicalize` rewrites the schema into one normal
//! form (see `Registry::canonicalize`).
//!
//! The schema and its `--resource`s are read as `validate` reads them; any other document a
//! `$ref` names is retrieved from inside `--file-root` (or over HTTP with
//! `--allow-remote-refs`). The result is written as pretty-printed JSON to `--output`, or to
//! stdout.
//!
//! Exit codes: `0` the result was written; `1` it could not be written; `2` there is no
//! result: a document could not be read or parsed (one `error:` line per problem), the schema
//! cannot be used (one `schema error:` line per problem), or it cannot be transformed.

use super::CliFailure;
use super::documents::{Document, no_verdict, read_documents, registry_builder, schema_problems};
use super::retrieval::{CliRetriever, RefRetrieval};
use json_schema_rs::{JsonSchemaSettings, Registry};
use std::path::{Path, PathBuf};

/// What to make of the schema.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Transform {
    /// [`Registry::bundle`].
    Bundle,
    /// [`Registry::dereference`].
    Dereference,
    /// [`Registry::canonicalize`].
    Canonicalize,
}

impl Transform {
    fn noun(self) -> &'static str {
        match self {
            Transform::Bundle => "bundle",
            Transform::Dereference => "dereferenced schema",
            Transform::Canonicalize => "canonical schema",
        }
    }
}

pub(crate) fn run_transform(
    transform: Transform,
    schema_path: &Path,
    resource_paths: &[PathBuf],
    output: Option<&Path>,
    schema_settings: &JsonSchemaSettings,
    retrieval: &RefRetrieval,
) -> Result<(), CliFailure> {
    let (schema, resources): (Option<Document>, Vec<Document>) =
        read_documents(Some(schema_path), resource_paths, schema_settings)?;
    let schema: Document = schema.expect("a schema path reads a schema or fails");
    let retriever: CliRetriever = CliRetriever::new(retrieval).map_err(no_verdict)?;
    let registry: Registry<'_> = registry_builder(
        schema.uri.as_deref(),
        &resources,
        retriever,
        schema_settings,
    )
    .build(&schema.schema)
    .map_err(|errors| schema_problems(errors.iter()))?;
    let noun: &str = transform.noun();
    let result: serde_json::Value = match transform {
        Transform::Bundle => registry.bundle().map_err(|e| e.to_string()),
        Transform::Dereference => registry.dereference().map_err(|e| e.to_string()),
        Transform::Canonicalize => registry.canonicalize().map_err(|e| e.to_string()),
    }
    .map_err(|e| no_verdict(format!("failed to build the {noun}: {e}")))?;
    let mut json: String = serde_json::to_string_pretty(&result)
        .map_err(|e| no_verdict(format!("failed to serialize the {noun}: {e}")))?;
    json.push('\n');
    if let Some(path) = output {
        std::fs::write(path, json)
            .map_err(|e| CliFailure::from(format!("failed to write {}: {e}", path.display())))
    } else {
        print!("{json}");
        Ok(())
    }
}
