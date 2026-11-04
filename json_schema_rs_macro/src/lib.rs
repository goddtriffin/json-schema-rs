//! Procedural macro `json_schema_to_rust!` and derive `ToJsonSchema` for json-schema-rs.
//!
//! Use with the `json-schema-rs` crate: add `json-schema-rs` with the `macro` feature
//! (or add `json-schema-rs-macro` directly), then invoke
//! `json_schema_to_rust!("path/to/schema.json")` or
//! `json_schema_to_rust!(r#"{"type":"object", ...}"#)`. A literal that starts with `{`, or is
//! `true` or `false` (a boolean schema), is inline JSON; any other literal is a path relative to
//! `CARGO_MANIFEST_DIR`.
//!
//! Cargo rebuilds the invoking crate when any schema file the macro read changes: every path
//! input and every file a schema `$ref`s.
//!
//! For reverse codegen (Rust → JSON Schema), use `#[derive(ToJsonSchema)]` with optional
//! `#[json_schema(title = "...")]` on the struct and `#[serde(rename = "...")]` on fields.

mod derive;

use derive::expand_to_json_schema;
use json_schema_rs::sanitizers::module_name_from_path;
use json_schema_rs::{
    CodeGenBackend, CodeGenInput, CodeGenSettings, FileRetriever, Retrieve, RetrieveError, Schema,
    file_path, file_uri,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, LitStr, Result as SynResult, Token};

/// Parse one or more string literals (paths or inline JSON).
struct SchemaInputs {
    literals: Vec<LitStr>,
}

impl Parse for SchemaInputs {
    fn parse(input: ParseStream<'_>) -> SynResult<Self> {
        let mut literals = Vec::new();
        literals.push(input.parse()?);
        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            literals.push(input.parse()?);
        }
        Ok(SchemaInputs { literals })
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
    let schema_inputs: SchemaInputs = syn::parse2(input)?;
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;

    // Cargo reruns a proc macro only when the invoking crate changes, so every file read here is
    // also `include_bytes!`ed into an unused constant: the compiler then lists it in the crate's
    // dep-info and Cargo rebuilds when it changes.
    let mut tracked: Vec<proc_macro2::TokenStream> = Vec::new();

    let mut schemas: Vec<(Schema, Option<String>)> =
        Vec::with_capacity(schema_inputs.literals.len());
    let mut mod_names: Vec<(String, proc_macro2::Span)> =
        Vec::with_capacity(schema_inputs.literals.len());

    for (index, lit) in schema_inputs.literals.iter().enumerate() {
        let s = lit.value();
        let s = s.trim();
        let inline: bool = s.starts_with('{') || s == "true" || s == "false";
        let (json_str, mod_name, uri): (String, String, Option<String>) = if inline {
            (s.to_string(), format!("schema_{index}"), None)
        } else {
            let path = std::path::Path::new(&manifest_dir).join(s);
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                syn::Error::new(lit.span(), format!("failed to read schema file {s}: {e}"))
            })?;
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

        let schema: Schema = Schema::try_from(json_str.as_str())
            .map_err(|e| syn::Error::new(lit.span(), format!("invalid JSON Schema: {e}")))?;
        schemas.push((schema, uri));
        mod_names.push((mod_name, lit.span()));
    }

    // `$ref`s to files that are not macro inputs are read from inside the crate's directory.
    let retriever: TrackingRetriever = TrackingRetriever {
        inner: FileRetriever::new(&manifest_dir),
        retrieved: RefCell::new(Vec::new()),
    };
    let mut input: CodeGenInput<'_> = CodeGenInput::new().retriever(&retriever);
    for (schema, uri) in &schemas {
        input = match uri {
            Some(uri) => input.schema_at(uri.clone(), schema),
            None => input.schema(schema),
        };
    }
    let code_gen_settings: CodeGenSettings = CodeGenSettings::builder().build();
    let backend = json_schema_rs::RustBackend;
    let output: json_schema_rs::GenerateRustOutput =
        backend
            .generate(&input, &code_gen_settings)
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
    use super::{manifest_relative, retrieved_path};
    use std::path::Path;

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
