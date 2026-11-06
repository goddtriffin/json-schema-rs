//! Procedural macro `json_schema_to_rust!` and derive `ToJsonSchema` for json-schema-rs.
//!
//! Add `json-schema-rs` and `json-schema-rs-macro`, then invoke
//! `json_schema_to_rust!("path/to/schema.json")` or
//! `json_schema_to_rust!(r#"{"type":"object", ...}"#)`. A literal that starts with `{`, or is
//! `true` or `false` (a boolean schema), is inline JSON; any other literal is a path relative to
//! `CARGO_MANIFEST_DIR`.
//!
//! Settings follow the schema literals as `name = value`, one per `CodeGenSettings` and
//! `JsonSchemaSettings` field, named like the builder methods; a flag takes `true` / `false` and
//! an enum its variant name:
//!
//! ```ignore
//! json_schema_to_rust!(
//!     "schemas/a.json",
//!     dedupe_mode = Disabled,
//!     unique_items = BTreeSet,
//!     forced_spec_version = Draft07,
//! );
//! ```
//!
//! A setting left out keeps the builder's default. An unknown setting, an unknown value, a
//! repeated setting, and `unique_items = IndexSet` without this crate's `indexmap` feature are
//! compile errors at their span.
//!
//! Cargo rebuilds the invoking crate when any schema file the macro read changes: every path
//! input and every file a schema `$ref`s.
//!
//! For reverse codegen (Rust → JSON Schema), use `#[derive(ToJsonSchema)]` with optional
//! `#[json_schema(title = "...")]` on the struct and `#[serde(rename = "...")]` on fields.

mod derive;
mod settings;

use derive::expand_to_json_schema;
use json_schema_rs::sanitizers::module_name_from_path;
use json_schema_rs::{
    CodeGenBackend, CodeGenInput, FileRetriever, JsonSchemaSettings, Retrieve, RetrieveError,
    Schema, file_path, file_uri,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde_json::Value;
use settings::{MacroSettings, SettingArg};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, LitStr, Result as SynResult, Token};

/// The macro's arguments: one or more string literals (paths or inline JSON), then any settings
/// (`name = value`, see [`settings`]), separated by commas, with an optional trailing comma.
struct MacroInput {
    literals: Vec<LitStr>,
    settings: Vec<SettingArg>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream<'_>) -> SynResult<Self> {
        let mut literals: Vec<LitStr> = vec![input.parse()?];
        let mut settings: Vec<SettingArg> = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            if input.peek(LitStr) {
                let literal: LitStr = input.parse()?;
                if !settings.is_empty() {
                    return Err(syn::Error::new(
                        literal.span(),
                        "schema literals come before the settings",
                    ));
                }
                literals.push(literal);
            } else {
                settings.push(input.parse()?);
            }
        }
        Ok(MacroInput { literals, settings })
    }
}

/// A [`FileRetriever`] that records the `file:` URI of every document it returns, so the macro
/// can tell Cargo which files the expansion depends on.
struct TrackingRetriever {
    inner: FileRetriever,
    retrieved: RefCell<Vec<String>>,
}

impl Retrieve for TrackingRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let document: Value = self.inner.retrieve(uri)?;
        self.retrieved.borrow_mut().push(uri.to_string());
        Ok(document)
    }
}

/// The argument of `include_bytes!` for a file at `relative` (`/`-separated) inside the crate's
/// directory: `concat!(env!("CARGO_MANIFEST_DIR"), "/relative")`, so the expansion names no
/// absolute path of this machine.
fn manifest_relative(relative: &str) -> proc_macro2::TokenStream {
    let suffix: String = format!("/{relative}");
    quote! { concat!(env!("CARGO_MANIFEST_DIR"), #suffix) }
}

/// The argument of `include_bytes!` for a retrieved file (canonical `path`): relative to the
/// crate's canonical directory `manifest_root`, which the [`FileRetriever`] keeps it inside; the
/// absolute path only if it is somehow outside or not UTF-8.
fn retrieved_path(manifest_root: &Path, path: &Path) -> proc_macro2::TokenStream {
    let relative: Option<String> = path.strip_prefix(manifest_root).ok().and_then(|relative| {
        relative
            .components()
            .map(|component| component.as_os_str().to_str())
            .collect::<Option<Vec<&str>>>()
            .map(|parts| parts.join("/"))
    });
    if let Some(relative) = relative {
        manifest_relative(&relative)
    } else {
        let absolute: String = path.to_string_lossy().into_owned();
        quote! { #absolute }
    }
}

/// Add `error` to `errors`, the errors found so far (each becomes its own compile error).
fn push_error(errors: &mut Option<syn::Error>, error: syn::Error) {
    match errors {
        Some(all) => all.combine(error),
        None => *errors = Some(error),
    }
}

/// The parsed input schemas, each with its retrieval URI, the module name and span for each,
/// and the `include_bytes!` argument for each path input.
struct LoadedSchemas {
    schemas: Vec<(Schema, Option<String>)>,
    mod_names: Vec<(String, Span)>,
    tracked: Vec<proc_macro2::TokenStream>,
}

/// Read and parse every schema literal (inline JSON or a path relative to `manifest_dir`).
fn load_schemas(
    literals: &[LitStr],
    manifest_dir: &str,
    json_schema_settings: &JsonSchemaSettings,
) -> SynResult<LoadedSchemas> {
    let mut tracked: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut schemas: Vec<(Schema, Option<String>)> = Vec::with_capacity(literals.len());
    let mut mod_names: Vec<(String, proc_macro2::Span)> = Vec::with_capacity(literals.len());

    // Every literal is read and parsed, so each problem in each schema is reported at once.
    let mut errors: Option<syn::Error> = None;
    for (index, lit) in literals.iter().enumerate() {
        let s = lit.value();
        let s = s.trim();
        let inline: bool = s.starts_with('{') || s == "true" || s == "false";
        let (json_str, mod_name, uri): (String, String, Option<String>) = if inline {
            (s.to_string(), format!("schema_{index}"), None)
        } else {
            let path = std::path::Path::new(manifest_dir).join(s);
            let contents: String = match std::fs::read_to_string(&path) {
                Ok(contents) => contents,
                Err(e) => {
                    push_error(
                        &mut errors,
                        syn::Error::new(lit.span(), format!("failed to read schema file {s}: {e}")),
                    );
                    continue;
                }
            };
            let name = module_name_from_path(s);
            // The file's own URI is the base for its relative `$ref`s to sibling files.
            let uri: Option<String> = path.canonicalize().ok().as_deref().and_then(file_uri);
            tracked.push(if Path::new(s).is_absolute() {
                quote! { #s }
            } else {
                manifest_relative(s)
            });
            (contents, name, uri)
        };

        // One error per problem, at the literal: the span says which schema it is about, as the
        // CLI's `<file>: ` prefix does, so the library's message needs no prefix of its own.
        match Schema::new_from_str(&json_str, json_schema_settings) {
            Ok(schema) => {
                schemas.push((schema, uri));
                mod_names.push((mod_name, lit.span()));
            }
            Err(parse_errors) => {
                for parse_error in parse_errors {
                    push_error(
                        &mut errors,
                        syn::Error::new(lit.span(), parse_error.to_string()),
                    );
                }
            }
        }
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(LoadedSchemas {
            schemas,
            mod_names,
            tracked,
        }),
    }
}

#[proc_macro_derive(ToJsonSchema, attributes(json_schema))]
pub fn derive_to_json_schema(input: TokenStream) -> TokenStream {
    let input: DeriveInput = syn::parse_macro_input!(input as DeriveInput);
    match expand_to_json_schema(input) {
        Ok(stream) => stream.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

#[proc_macro]
pub fn json_schema_to_rust(input: TokenStream) -> TokenStream {
    let result = json_schema_to_rust_impl(input.into());
    match result {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err(e) => syn::Error::to_compile_error(&e).into(),
    }
}

fn json_schema_to_rust_impl(
    input: proc_macro2::TokenStream,
) -> SynResult<proc_macro2::TokenStream> {
    let schema_inputs: MacroInput = syn::parse2(input)?;
    let settings: MacroSettings = settings::resolve(&schema_inputs.settings)?;
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;

    // Cargo reruns a proc macro only when the invoking crate changes, so every file read here is
    // also `include_bytes!`ed into an unused constant: the compiler then lists it in the crate's
    // dep-info and Cargo rebuilds when it changes.
    let mut tracked: Vec<proc_macro2::TokenStream> = Vec::new();

    let LoadedSchemas {
        schemas,
        mod_names,
        tracked: input_files,
    } = load_schemas(
        &schema_inputs.literals,
        &manifest_dir,
        &settings.json_schema,
    )?;
    tracked.extend(input_files);

    // `$ref`s to files that are not macro inputs are read from inside the crate's directory.
    let retriever: TrackingRetriever = TrackingRetriever {
        inner: FileRetriever::new(&manifest_dir),
        retrieved: RefCell::new(Vec::new()),
    };
    let mut input: CodeGenInput<'_> = CodeGenInput::new()
        .retriever(&retriever)
        .settings(settings.json_schema.clone());
    for (schema, uri) in &schemas {
        input = match uri {
            Some(uri) => input.schema_at(uri.clone(), schema),
            None => input.schema(schema),
        };
    }
    let backend = json_schema_rs::RustBackend;
    let output: json_schema_rs::GenerateRustOutput =
        backend
            .generate(&input, &settings.code_gen)
            .map_err(|e| syn::Error::new(Span::call_site(), format!("codegen failed: {e}")))?;

    drop(input);
    let manifest_root: PathBuf = Path::new(&manifest_dir)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&manifest_dir));
    let retrieved_files: BTreeSet<PathBuf> = retriever
        .retrieved
        .into_inner()
        .iter()
        .filter_map(|uri| file_path(uri))
        .map(|path| path.canonicalize().unwrap_or(path))
        .collect();
    tracked.extend(
        retrieved_files
            .iter()
            .map(|path| retrieved_path(&manifest_root, path)),
    );

    let mut modules = Vec::new();
    if let Some(shared_bytes) = &output.shared {
        let rust_str = String::from_utf8(shared_bytes.clone()).map_err(|e| {
            syn::Error::new(Span::call_site(), format!("shared code was not UTF-8: {e}"))
        })?;
        let file: syn::File = syn::parse_str(&rust_str).map_err(|e| {
            syn::Error::new(Span::call_site(), format!("shared Rust did not parse: {e}"))
        })?;
        let items = &file.items;
        modules.push(quote! {
            pub mod shared {
                #(#items)*
            }
        });
    }
    for ((mod_name, span), bytes) in mod_names.into_iter().zip(output.per_schema) {
        let rust_str = String::from_utf8(bytes)
            .map_err(|e| syn::Error::new(span, format!("generated code was not UTF-8: {e}")))?;

        let file: syn::File = syn::parse_str(&rust_str)
            .map_err(|e| syn::Error::new(span, format!("generated Rust did not parse: {e}")))?;

        let items = &file.items;
        let mod_ident = syn::Ident::new(&mod_name, span);
        modules.push(quote! {
            pub mod #mod_ident {
                #(#items)*
            }
        });
    }

    Ok(quote! {
        #(const _: &[u8] = include_bytes!(#tracked);)*
        #(#modules)*
    })
}

#[cfg(test)]
mod tests {
    use super::{json_schema_to_rust_impl, manifest_relative, retrieved_path};
    use proc_macro2::TokenStream;
    use std::path::Path;
    use std::str::FromStr;

    /// The messages of the errors the macro reports for arguments `source`, each with its line
    /// (1-based) and column (0-based); empty when it expands.
    fn expansion_errors(source: &str) -> Vec<(String, usize, usize)> {
        let tokens: TokenStream = TokenStream::from_str(source).expect("tokenize");
        match json_schema_to_rust_impl(tokens) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|error| {
                    let start = error.span().start();
                    (error.to_string(), start.line, start.column)
                })
                .collect(),
        }
    }

    #[test]
    fn settings_after_literals_with_trailing_comma_expand() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"type":"string"}"#, r#"{"type":"integer"}"#, dedupe_mode = Disabled, self_validating = false,"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn literal_after_setting_is_an_error_at_the_literal() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "schema literals come before the settings".to_string(),
            1,
            48,
        )];
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"type":"string"}"#, dedupe_mode = Disabled, r#"{"type":"integer"}"#"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn setting_without_value_is_an_error() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "expected a setting value: `true`, `false` or a variant name".to_string(),
            1,
            38,
        )];
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"type":"string"}"#, dedupe_mode = "Full""##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn disallow_unknown_fields_rejects_unknown_key() {
        let expected: Vec<(String, usize, usize)> =
            vec![(r#"unknown keyword "colour" at root"#.to_string(), 1, 0)];
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"type":"string","colour":"blue"}"#, disallow_unknown_fields = true"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_unknown_key_is_its_own_error() {
        let expected: Vec<(String, usize, usize)> = vec![
            (r#"unknown keyword "colour" at root"#.to_string(), 1, 0),
            (
                r#"unknown keyword "size" at /properties/a"#.to_string(),
                1,
                0,
            ),
        ];
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"colour":"blue","properties":{"a":{"size":1}}}"#, disallow_unknown_fields = true"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_failing_literal_is_reported() {
        let expected: Vec<(String, usize, usize)> = vec![
            (
                "invalid JSON Schema: invalid type: string \"x\", expected a JSON number at line 1 \
                 column 14"
                    .to_string(),
                1,
                0,
            ),
            (
                "failed to read schema file missing.json: No such file or directory (os error 2)"
                    .to_string(),
                1,
                22,
            ),
            (
                "invalid JSON Schema: invalid type: string \"y\", expected a JSON number at line 1 \
                 column 14"
                    .to_string(),
                1,
                38,
            ),
        ];
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"minimum":"x"}"#, "missing.json", r#"{"maximum":"y"}"#"##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_key_is_ignored_by_default() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"type":"string","colour":"blue"}"#"##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_schema_uri_reject_rejects_unrecognised_schema() {
        let expected: Vec<(String, usize, usize)> = vec![(
            r#"unknown $schema "https://example.com/meta""#.to_string(),
            1,
            0,
        )];
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"$schema":"https://example.com/meta","type":"string"}"#, unknown_schema_uri = Reject"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_bounds_boolean_modifier_accepts_boolean_exclusive_minimum() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> = expansion_errors(
            r##"r#"{"type":"integer","minimum":5,"exclusiveMinimum":true}"#, exclusive_bounds = BooleanModifier"##,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_exclusive_minimum_is_an_error_by_default() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "invalid JSON Schema: invalid type: boolean `true`, expected a \
             JSON number at line 1 column 53"
                .to_string(),
            1,
            0,
        )];
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"type":"integer","minimum":5,"exclusiveMinimum":true}"#"##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unrecognised_schema_falls_back_by_default() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"$schema":"https://example.com/meta","type":"string"}"#"##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn settings_errors_are_reported_together() {
        let expected: Vec<(String, usize, usize)> = vec![
            (
                "unknown setting `dedupe`; expected `model_name_source`, `dedupe_mode`, \
                 `unknown_properties`, `unique_items`, `flatten_single_entry_combinators`, \
                 `self_validating`, `constrained_newtypes`, `format_types`, `disallow_unknown_fields`, \
                 `forced_spec_version`, `default_spec_version`, `format_mode`, \
                 `integer_semantics`, `exclusive_bounds`, `ref_siblings` or `unknown_schema_uri`"
                    .to_string(),
                1,
                24,
            ),
            (
                "unknown value `List` for `unique_items`; expected `Vec`, `HashSet`, `BTreeSet` \
                 or `IndexSet`"
                    .to_string(),
                1,
                54,
            ),
        ];
        let actual: Vec<(String, usize, usize)> =
            expansion_errors(r##"r#"{"type":"string"}"#, dedupe = Full, unique_items = List"##);
        assert_eq!(expected, actual);
    }

    #[test]
    fn manifest_relative_joins_manifest_dir_env() {
        let expected: String =
            r#"concat ! (env ! ("CARGO_MANIFEST_DIR") , "/schemas/a.json")"#.to_string();
        let actual: String = manifest_relative("schemas/a.json").to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn retrieved_path_inside_crate_is_manifest_relative() {
        let expected: String =
            r#"concat ! (env ! ("CARGO_MANIFEST_DIR") , "/schemas/sub/b.json")"#.to_string();
        let actual: String = retrieved_path(
            Path::new("/work/crate"),
            Path::new("/work/crate/schemas/sub/b.json"),
        )
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(windows))]
    fn retrieved_path_outside_crate_is_absolute() {
        let expected: String = r#""/elsewhere/b.json""#.to_string();
        let actual: String =
            retrieved_path(Path::new("/work/crate"), Path::new("/elsewhere/b.json")).to_string();
        assert_eq!(expected, actual);
    }
}
