//! Procedural macro `json_schema_to_rust!` and derive `ToJsonSchema` for json-schema-rs.
//!
//! Use with the `json-schema-rs` crate: add `json-schema-rs` with the `macro` feature
//! (or add `json-schema-rs-macro` directly), then invoke
//! `json_schema_to_rust!("path/to/schema.json")` or
//! `json_schema_to_rust!(r#"{"type":"object", ...}"#)`.
//!
//! For reverse codegen (Rust → JSON Schema), use `#[derive(ToJsonSchema)]` with optional
//! `#[json_schema(title = "...")]` on the struct and `#[serde(rename = "...")]` on fields.

mod derive;

use derive::expand_to_json_schema;
use json_schema_rs::sanitizers::module_name_from_path;
use json_schema_rs::{
    CodeGenBackend, CodeGenInput, CodeGenSettings, FileRetriever, JsonSchema, file_uri,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
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

    let mut schemas: Vec<(json_schema_rs::JsonSchema, Option<String>)> =
        Vec::with_capacity(schema_inputs.literals.len());
    let mut mod_names: Vec<(String, proc_macro2::Span)> =
        Vec::with_capacity(schema_inputs.literals.len());

    for (index, lit) in schema_inputs.literals.iter().enumerate() {
        let s = lit.value();
        let s = s.trim();
        let (json_str, mod_name, uri): (String, String, Option<String>) = if s.starts_with('{') {
            (s.to_string(), format!("schema_{index}"), None)
        } else {
            let path = std::path::Path::new(&manifest_dir).join(s);
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                syn::Error::new(lit.span(), format!("failed to read schema file {s}: {e}"))
            })?;
            let name = module_name_from_path(s);
            // The file's own URI is the base for its relative `$ref`s to sibling files.
            let uri: Option<String> = path.canonicalize().ok().as_deref().and_then(file_uri);
            (contents, name, uri)
        };

        let schema: json_schema_rs::JsonSchema = JsonSchema::try_from(json_str.as_str())
            .map_err(|e| syn::Error::new(lit.span(), format!("invalid JSON Schema: {e}")))?;
        schemas.push((schema, uri));
        mod_names.push((mod_name, lit.span()));
    }

    // `$ref`s to files that are not macro inputs are read from inside the crate's directory.
    let mut input: CodeGenInput<'_> =
        CodeGenInput::new().retriever(FileRetriever::new(&manifest_dir));
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
        #(#modules)*
    })
}
