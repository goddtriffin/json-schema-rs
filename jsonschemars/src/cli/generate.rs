//! `jsonschemars generate` subcommand: generate Rust from JSON Schema files.

use super::retrieval::{CliRetriever, RefRetrieval};
use super::utils::{
    collect_schema_entries, display_schema_path, print_dialect_warnings, read_schema_from_path,
    write_mod_rs_files,
};
use clap::ArgMatches;
use json_schema_rs::{
    CodeGenInput, CodeGenSettings, CrateSpec, DedupeMode, FormatTypes, JsonSchemaSettings, MapType,
    ModelNameSource, Schema, TypePatch, UniqueItemsType, UnknownCrates, UnknownProperties,
    file_uri, generate_rust,
};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn run_generate(
    lang: &str,
    inputs: &[String],
    output_dir: &Path,
    schema_settings: &JsonSchemaSettings,
    code_gen_settings: &CodeGenSettings,
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
    let uris: Vec<Option<String>> = entries.iter().map(|(path, _)| file_uri(path)).collect();
    let documents: Vec<(&Path, &Schema, Option<&str>)> = entries
        .iter()
        .zip(&schemas)
        .zip(&uris)
        .map(|(((path, _), schema), uri)| (path.as_path(), schema, uri.as_deref()))
        .collect();
    print_dialect_warnings(&documents, schema_settings);
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
    let output = generate_rust(input, code_gen_settings).map_err(|e| e.to_string())?;
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

/// The patches of a `--cgs-patch` file: a JSON object of type names to objects with
/// `rename` (a string), `derives` and `attrs` (arrays of strings).
fn read_patches(path: &Path) -> Result<BTreeMap<String, TypePatch>, String> {
    let fail = |reason: String| format!("--cgs-patch {}: {reason}", path.display());
    let text: String = fs::read_to_string(path).map_err(|e| fail(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
    let serde_json::Value::Object(entries) = value else {
        return Err(fail(
            "expected an object of type names to patches".to_string(),
        ));
    };
    let strings =
        |name: &str, field: &str, value: &serde_json::Value| -> Result<Vec<String>, String> {
            value
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .map(|item| item.as_str().map(str::to_string))
                        .collect::<Option<Vec<String>>>()
                })
                .ok_or_else(|| fail(format!("`{name}.{field}` must be an array of strings")))
        };
    let mut patches: BTreeMap<String, TypePatch> = BTreeMap::new();
    for (name, fields) in entries {
        let serde_json::Value::Object(fields) = fields else {
            return Err(fail(format!("the patch for `{name}` must be an object")));
        };
        let mut type_patch: TypePatch = TypePatch::default();
        for (field, value) in &fields {
            match field.as_str() {
                "rename" => {
                    type_patch.rename = Some(
                        value
                            .as_str()
                            .ok_or_else(|| fail(format!("`{name}.rename` must be a string")))?
                            .to_string(),
                    );
                }
                "derives" => type_patch.derives = strings(&name, field, value)?,
                "attrs" => type_patch.attrs = strings(&name, field, value)?,
                _ => {
                    return Err(fail(format!(
                        "unknown field `{name}.{field}`; expected `rename`, `derives` or `attrs`"
                    )));
                }
            }
        }
        patches.insert(name, type_patch);
    }
    Ok(patches)
}

/// The `--cgs-map-type` value's [`MapType`]; `index-map` fails when `jsonschemars` is built
/// without the `indexmap` feature.
#[cfg_attr(
    feature = "indexmap",
    expect(
        clippy::unnecessary_wraps,
        reason = "fails only without the `indexmap` feature"
    )
)]
fn map_type(kind: &str) -> Result<MapType, String> {
    match kind {
        "hash-map" => Ok(MapType::HashMap),
        #[cfg(feature = "indexmap")]
        "index-map" => Ok(MapType::IndexMap),
        #[cfg(not(feature = "indexmap"))]
        "index-map" => Err(
            "--cgs-map-type index-map needs jsonschemars built with the `indexmap` feature"
                .to_string(),
        ),
        _ => Ok(MapType::BTreeMap),
    }
}

/// Codegen settings from the `--cgs-*` flags. Fails when the `--cgs-patch` file cannot be read
/// or is malformed, when a `--cgs-convert` or `--cgs-replace` is not `X=Y` (or its schema is not
/// JSON), when a `--cgs-crate` is not `[rename=]crate@version`, for `--cgs-unique-items index-set` when
/// `jsonschemars` is built without the `indexmap` feature, and as [`map_type`] does.
pub(crate) fn code_gen_settings(matches: &ArgMatches) -> Result<CodeGenSettings, String> {
    let flag = |name: &str| matches.get_one::<String>(name).map(String::as_str);
    let mut b = CodeGenSettings::builder()
        .flatten_single_entry_combinators(matches.get_flag("cgs-flatten-single-entry-combinators"))
        .builders(matches.get_flag("cgs-builders"));
    if let Some(&on) = matches.get_one::<bool>("cgs-self-validating") {
        b = b.self_validating(on);
    }
    if let Some(&on) = matches.get_one::<bool>("cgs-constrained-newtypes") {
        b = b.constrained_newtypes(on);
    }
    if let Some(src) = flag("cgs-model-name-source") {
        b = b.model_name_source(match src {
            "property-key" => ModelNameSource::PropertyKeyFirst,
            _ => ModelNameSource::TitleFirst,
        });
    }
    if let Some(mode) = flag("cgs-dedupe-mode") {
        b = b.dedupe_mode(match mode {
            "disabled" => DedupeMode::Disabled,
            "functional" => DedupeMode::Functional,
            _ => DedupeMode::Full,
        });
    }
    if let Some(mode) = flag("cgs-unknown-properties") {
        b = b.unknown_properties(match mode {
            "capture" => UnknownProperties::Capture,
            _ => UnknownProperties::Drop,
        });
    }
    if let Some(mode) = flag("cgs-format-types") {
        b = b.format_types(match mode {
            "follow-dialect" => FormatTypes::FollowDialect,
            _ => FormatTypes::Typed,
        });
    }
    let many = |name: &str| -> Vec<String> {
        matches
            .get_many::<String>(name)
            .map_or_else(Vec::new, |values| values.cloned().collect())
    };
    b = b
        .extra_derives(many("cgs-derive"))
        .extra_attrs(many("cgs-attr"));
    if let Some(path) = flag("cgs-patch") {
        for (name, patch) in read_patches(Path::new(path))? {
            b = b.patch(name, patch);
        }
    }
    for value in many("cgs-convert") {
        let (rust_type, schema) = value
            .split_once('=')
            .ok_or_else(|| format!("--cgs-convert {value}: expected TYPE=SCHEMA"))?;
        let schema: serde_json::Value = serde_json::from_str(schema)
            .map_err(|e| format!("--cgs-convert {value}: the schema is not JSON: {e}"))?;
        b = b.convert(schema, rust_type);
    }
    for value in many("cgs-replace") {
        let (name, rust_type) = value
            .split_once('=')
            .ok_or_else(|| format!("--cgs-replace {value}: expected NAME=TYPE"))?;
        b = b.replace(name, rust_type);
    }
    for value in many("cgs-crate") {
        let (name, spec): (String, CrateSpec) =
            CrateSpec::parse_named(&value).map_err(|e| format!("--cgs-crate: {e}"))?;
        b = b.external_crate(name, spec);
    }
    if let Some(mode) = flag("cgs-unknown-crates") {
        b = b.unknown_crates(match mode {
            "allow" => UnknownCrates::Allow,
            "deny" => UnknownCrates::Deny,
            _ => UnknownCrates::Generate,
        });
    }
    if let Some(kind) = flag("cgs-unique-items") {
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
    if let Some(kind) = flag("cgs-map-type") {
        b = b.map_type(map_type(kind)?);
    }
    Ok(b.build())
}
