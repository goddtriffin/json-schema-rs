//! `jsonschemars generate` subcommand: generate Rust from JSON Schema files.

use super::retrieval::{CliRetriever, RefRetrieval};
use super::utils::{
    collect_schema_entries, display_schema_path, read_schema_from_path, write_mod_rs_files,
};
use json_schema_rs::{
    CodeGenInput, CodeGenSettings, DedupeMode, JsonSchemaSettings, ModelNameSource, Schema,
    UniqueItemsType, UnknownProperties, file_uri, generate_rust,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[expect(clippy::too_many_arguments)]
pub(crate) fn run_generate(
    lang: &str,
    inputs: &[String],
    output_dir: &Path,
    schema_settings: &JsonSchemaSettings,
    cgs_model_name_source: Option<&str>,
    cgs_dedupe_mode: Option<&str>,
    cgs_unknown_properties: Option<&str>,
    cgs_unique_items: Option<&str>,
    retrieval: &RefRetrieval,
) -> Result<(), String> {
    if !lang.eq_ignore_ascii_case("rust") {
        return Err(format!("unsupported language: {lang}; supported: rust"));
    }
    if inputs.is_empty() {
        return Err(
            "at least one INPUT (file, directory, or \"-\" for stdin) is required".to_string(),
        );
    }
    let entries = collect_schema_entries(inputs)?;
    if entries.is_empty() {
        return Err("no JSON Schema files found (look for .json in directories)".to_string());
    }

    let code_gen_settings: CodeGenSettings = code_gen_settings(
        cgs_model_name_source,
        cgs_dedupe_mode,
        cgs_unknown_properties,
        cgs_unique_items,
    )?;

    // Standalone ingestion step: try every file, log each failure to stderr, do not short-circuit.
    let retriever: CliRetriever = CliRetriever::new(retrieval)?;
    let mut successful: Vec<(Schema, PathBuf)> = Vec::with_capacity(entries.len());
    let mut had_errors = false;
    for (input_path, output_relative) in &entries {
        match read_schema_from_path(input_path, schema_settings) {
            Ok(schema) => successful.push((schema, output_relative.clone())),
            Err(e) => {
                // One line per problem the schema has, each naming the file.
                for line in e.lines() {
                    eprintln!("error: {}: {line}", display_schema_path(input_path));
                }
                had_errors = true;
            }
        }
    }
    if had_errors {
        let count = entries.len() - successful.len();
        return Err(format!(
            "schema ingestion failed with {count} error(s); no output written"
        ));
    }

    let (schemas, output_relatives): (Vec<Schema>, Vec<PathBuf>) = successful.into_iter().unzip();
    // Each file is retrieved from its own `file:` URI, so its relative `$ref`s reach its
    // siblings, and the other inputs reach it by that URI or its `$id`.
    let mut input: CodeGenInput<'_> = CodeGenInput::new()
        .retriever(retriever)
        .settings(schema_settings.clone());
    for (schema, (input_path, _)) in schemas.iter().zip(&entries) {
        input = match file_uri(input_path) {
            Some(uri) => input.schema_at(uri, schema),
            None => input.schema(schema),
        };
    }
    let output = generate_rust(input, &code_gen_settings).map_err(|e| e.to_string())?;
    assert_eq!(
        output.per_schema.len(),
        output_relatives.len(),
        "codegen output count"
    );

    for (output_relative, bytes) in output_relatives.iter().zip(output.per_schema.iter()) {
        let out_path = output_dir.join(output_relative);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create output dir {}: {e}", parent.display()))?;
        }
        let mut f = fs::File::create(&out_path)
            .map_err(|e| format!("failed to create {}: {e}", out_path.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", out_path.display()))?;
    }

    let mod_relatives: Vec<PathBuf> = if let Some(shared_bytes) = &output.shared {
        let shared_path = output_dir.join("shared.rs");
        let mut f = fs::File::create(&shared_path)
            .map_err(|e| format!("failed to create {}: {e}", shared_path.display()))?;
        f.write_all(shared_bytes)
            .map_err(|e| format!("failed to write {}: {e}", shared_path.display()))?;
        f.flush()
            .map_err(|e| format!("failed to flush {}: {e}", shared_path.display()))?;
        let mut all_files = vec![PathBuf::from("shared.rs")];
        all_files.extend(output_relatives.iter().cloned());
        all_files
    } else {
        output_relatives.clone()
    };
    write_mod_rs_files(output_dir, &mod_relatives, output.shared.is_some())?;
    Ok(())
}

/// Codegen settings from the `--cgs-*` flags. Fails for `--cgs-unique-items index-set` when
/// `jsonschemars` is built without the `indexmap` feature.
#[cfg_attr(
    feature = "indexmap",
    expect(
        clippy::unnecessary_wraps,
        reason = "fails only without the `indexmap` feature"
    )
)]
fn code_gen_settings(
    cgs_model_name_source: Option<&str>,
    cgs_dedupe_mode: Option<&str>,
    cgs_unknown_properties: Option<&str>,
    cgs_unique_items: Option<&str>,
) -> Result<CodeGenSettings, String> {
    let mut b = CodeGenSettings::builder();
    if let Some(src) = cgs_model_name_source {
        b = b.model_name_source(match src {
            "property-key" => ModelNameSource::PropertyKeyFirst,
            _ => ModelNameSource::TitleFirst,
        });
    }
    if let Some(mode) = cgs_dedupe_mode {
        b = b.dedupe_mode(match mode {
            "disabled" => DedupeMode::Disabled,
            "functional" => DedupeMode::Functional,
            _ => DedupeMode::Full,
        });
    }
    if let Some(mode) = cgs_unknown_properties {
        b = b.unknown_properties(match mode {
            "capture" => UnknownProperties::Capture,
            _ => UnknownProperties::Drop,
        });
    }
    if let Some(kind) = cgs_unique_items {
        b = b.unique_items(match kind {
            "hash-set" => UniqueItemsType::HashSet,
            "btree-set" => UniqueItemsType::BTreeSet,
            #[cfg(feature = "indexmap")]
            "index-set" => UniqueItemsType::IndexSet,
            #[cfg(not(feature = "indexmap"))]
            "index-set" => {
                return Err(
                    "--cgs-unique-items index-set needs jsonschemars built with the `indexmap` feature"
                        .to_string(),
                );
            }
            _ => UniqueItemsType::Vec,
        });
    }
    Ok(b.build())
}
