//! `jsonschemars generate` subcommand: generate Rust from JSON Schema files.

use super::utils::{collect_schema_entries, read_schema_from_path, write_mod_rs_files};
use json_schema_rs::{
    CodeGenSettings, DedupeMode, JsonSchema, JsonSchemaSettings, ModelNameSource, generate_rust,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[expect(clippy::too_many_lines)]
pub(crate) fn run_generate(
    lang: &str,
    inputs: &[String],
    output_dir: &Path,
    jss_disallow_unknown_fields: bool,
    cgs_model_name_source: Option<&str>,
    cgs_dedupe_mode: Option<&str>,
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

    let schema_settings: JsonSchemaSettings = JsonSchemaSettings::builder()
        .disallow_unknown_fields(jss_disallow_unknown_fields)
        .build();
    let code_gen_settings: CodeGenSettings = {
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
        b.build()
    };

    // Standalone ingestion step: try every file, log each failure to stderr, do not short-circuit.
    let mut successful: Vec<(JsonSchema, PathBuf)> = Vec::with_capacity(entries.len());
    let mut had_errors = false;
    for (input_path, output_relative) in &entries {
        match read_schema_from_path(input_path, &schema_settings) {
            Ok(schema) => successful.push((schema, output_relative.clone())),
            Err(e) => {
                let path_display = if input_path.as_os_str() == "-" {
                    "stdin".to_string()
                } else {
                    input_path.display().to_string()
                };
                eprintln!("error: {path_display}: {e}");
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

    let (schemas, output_relatives): (Vec<JsonSchema>, Vec<PathBuf>) =
        successful.into_iter().unzip();
    let output = generate_rust(&schemas, &code_gen_settings).map_err(|e| e.to_string())?;
    assert_eq!(
        output.per_schema.len(),
        output_relatives.len(),
        "codegen output count"
    );

    let per_schema_bytes: Vec<Vec<u8>> = if output.shared.is_some() {
        output
            .per_schema
            .iter()
            .map(|bytes| {
                let s = String::from_utf8_lossy(bytes);
                let replaced = s.replace("pub use crate::", "pub use crate::shared::");
                replaced.into_bytes()
            })
            .collect()
    } else {
        output.per_schema.clone()
    };

    for (output_relative, bytes) in output_relatives.iter().zip(per_schema_bytes.iter()) {
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
    write_mod_rs_files(output_dir, &mod_relatives)?;
    Ok(())
}
