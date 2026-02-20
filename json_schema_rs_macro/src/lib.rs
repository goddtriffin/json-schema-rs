//! Procedural macro `generate_rust_schema!` for compile-time codegen from JSON Schema.
//!
//! Use with the `json-schema-rs` crate: add both `json-schema-rs` and
//! `json-schema-rs-macro` to your dependencies, then invoke
//! `generate_rust_schema!("path/to/schema.json")` or
//! `generate_rust_schema!(r#"{"type":"object", ...}"#)`.

use json_schema_rs::sanitize::module_name_from_path;
use json_schema_rs::{CodeGenBackend, CodeGenSettings, JsonSchemaSettings, parse_schema};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{LitStr, Result as SynResult, Token};

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

#[proc_macro]
pub fn generate_rust_schema(input: TokenStream) -> TokenStream {
    let result = generate_rust_schema_impl(input.into());
    match result {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err(e) => syn::Error::to_compile_error(&e).into(),
    }
}

fn generate_rust_schema_impl(
    input: proc_macro2::TokenStream,
) -> SynResult<proc_macro2::TokenStream> {
    let schema_inputs: SchemaInputs = syn::parse2(input)?;
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| syn::Error::new(Span::call_site(), "CARGO_MANIFEST_DIR not set"))?;

    let mut schemas: Vec<json_schema_rs::JsonSchema> =
        Vec::with_capacity(schema_inputs.literals.len());
    let mut mod_names: Vec<(String, proc_macro2::Span)> =
        Vec::with_capacity(schema_inputs.literals.len());

    for (index, lit) in schema_inputs.literals.iter().enumerate() {
        let s = lit.value();
        let s = s.trim();
        let (json_str, mod_name): (String, String) = if s.starts_with('{') {
            (s.to_string(), format!("schema_{index}"))
        } else {
            let path = std::path::Path::new(&manifest_dir).join(s);
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                syn::Error::new(lit.span(), format!("failed to read schema file {s}: {e}"))
            })?;
            let name = module_name_from_path(s);
            (contents, name)
        };

        let schema_settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let schema: json_schema_rs::JsonSchema = parse_schema(&json_str, &schema_settings)
            .map_err(|e| syn::Error::new(lit.span(), format!("invalid JSON Schema: {e}")))?;
        schemas.push(schema);
        mod_names.push((mod_name, lit.span()));
    }

    let code_gen_settings: CodeGenSettings = CodeGenSettings::builder().build();
    let backend = json_schema_rs::RustBackend;
    let bytes_list: Vec<Vec<u8>> = backend
        .generate(&schemas, &code_gen_settings)
        .map_err(|e| syn::Error::new(Span::call_site(), format!("codegen failed: {e}")))?;

    let mut modules = Vec::new();
    for ((mod_name, span), bytes) in mod_names.into_iter().zip(bytes_list) {
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
