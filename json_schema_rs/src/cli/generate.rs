//! `jsonschemars generate` subcommand: generate Rust from JSON Schema files.

use super::utils::{
    collect_schema_entries, display_schema_path, read_schema_from_path, write_mod_rs_files,
};
use json_schema_rs::{
    CodeGenInput, CodeGenSettings, DedupeMode, FileRetriever, JsonSchemaSettings, ModelNameSource,
    Retrieve, RetrieveError, Schema, UnknownProperties, file_uri, generate_rust,
};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where `$ref`s to documents that are not inputs may be read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefRetrieval {
    /// `--cgs-file-root`: files are read only from inside it (default: the current directory).
    pub(crate) file_root: Option<PathBuf>,
    /// `--cgs-allow-remote-refs`: retrieve `http:` / `https:` documents.
    pub(crate) allow_remote: bool,
}

/// The CLI's retriever: `file:` URIs inside the file root, and `http:` / `https:` URLs when
/// remote retrieval is enabled.
struct CliRetriever {
    file: FileRetriever,
    #[cfg(feature = "http")]
    http: Option<json_schema_rs::HttpRetriever>,
}

impl CliRetriever {
    fn new(retrieval: &RefRetrieval) -> Result<Self, String> {
        let file_root: PathBuf = match &retrieval.file_root {
            Some(root) => root.clone(),
            None => std::env::current_dir()
                .map_err(|e| format!("cannot read the current directory: {e}"))?,
        };
        #[cfg(feature = "http")]
        let http: Option<json_schema_rs::HttpRetriever> = if retrieval.allow_remote {
            Some(json_schema_rs::HttpRetriever::new().map_err(|e| e.reason)?)
        } else {
            None
        };
        #[cfg(not(feature = "http"))]
        if retrieval.allow_remote {
            return Err(
                "--cgs-allow-remote-refs needs jsonschemars built with the `http` feature"
                    .to_string(),
            );
        }
        Ok(CliRetriever {
            file: FileRetriever::new(file_root),
            #[cfg(feature = "http")]
            http,
        })
    }
}

impl Retrieve for CliRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let scheme: String = uri
            .split_once(':')
            .map(|(scheme, _)| scheme.to_ascii_lowercase())
            .unwrap_or_default();
        match scheme.as_str() {
            "file" => self.file.retrieve(uri).map_err(|e| {
                // Point at the flag only when the root is what refused the file.
                if e.reason.contains(" is outside the file root ") {
                    RetrieveError {
                        reason: format!("{} (set it with --cgs-file-root)", e.reason),
                    }
                } else {
                    e
                }
            }),
            "http" | "https" => {
                #[cfg(feature = "http")]
                if let Some(http) = &self.http {
                    return http.retrieve(uri);
                }
                Err(RetrieveError {
                    reason: "remote retrieval is off (enable it with --cgs-allow-remote-refs)"
                        .to_string(),
                })
            }
            _ => Err(RetrieveError {
                reason: format!("no retriever for the URI scheme \"{scheme}\""),
            }),
        }
    }
}

#[expect(clippy::too_many_arguments)]
pub(crate) fn run_generate(
    lang: &str,
    inputs: &[String],
    output_dir: &Path,
    schema_settings: &JsonSchemaSettings,
    cgs_model_name_source: Option<&str>,
    cgs_dedupe_mode: Option<&str>,
    cgs_unknown_properties: Option<&str>,
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
    );

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

/// Codegen settings from the `--cgs-*` flags.
fn code_gen_settings(
    cgs_model_name_source: Option<&str>,
    cgs_dedupe_mode: Option<&str>,
    cgs_unknown_properties: Option<&str>,
) -> CodeGenSettings {
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
    b.build()
}
