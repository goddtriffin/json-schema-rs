//! Derive macro for `ToJsonSchema`: builds JSON Schema from struct type and attributes.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    Attribute, DeriveInput, Error, Field, Fields, Ident, LitStr, Meta, Result as SynResult, Token,
    Type,
};

/// Extracts `title = "..."` from `#[to_json_schema(...)]` container attribute.
fn container_title(attrs: &[Attribute]) -> SynResult<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("to_json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("title") {
                continue;
            }
            let syn::Expr::Lit(expr_lit) = nv.value else {
                continue;
            };
            let syn::Lit::Str(s) = expr_lit.lit else {
                continue;
            };
            return Ok(Some(s.value()));
        }
    }
    Ok(None)
}

/// Returns the JSON property key for a field: serde rename or field name.
fn field_property_key(field: &Field) -> SynResult<String> {
    for attr in &field.attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if nv.path.is_ident("rename") {
                let syn::Expr::Lit(expr_lit) = nv.value else {
                    continue;
                };
                let syn::Lit::Str(s) = expr_lit.lit else {
                    continue;
                };
                return Ok(s.value());
            }
        }
    }
    let ident = field.ident.as_ref().ok_or_else(|| {
        Error::new_spanned(
            field,
            "ToJsonSchema derive only supports named struct fields",
        )
    })?;
    Ok(ident.to_string())
}

/// Returns true if the type is `Option<T>` (path is `Option` or `std::option::Option`).
fn is_option_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    let path = &type_path.path;
    path.segments
        .last()
        .is_some_and(|seg| seg.ident == "Option")
}

/// Inner type of `Option<T>` if this is Option, otherwise the type itself.
fn optional_inner_type(ty: &Type) -> &Type {
    let Type::Path(type_path) = ty else {
        return ty;
    };
    let path = &type_path.path;
    let Some(seg) = path.segments.last() else {
        return ty;
    };
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return ty;
    };
    let Some(first) = args.args.first() else {
        return ty;
    };
    let syn::GenericArgument::Type(t) = first else {
        return ty;
    };
    t
}

pub fn expand_to_json_schema(input: DeriveInput) -> SynResult<TokenStream2> {
    let fields = match &input.data {
        syn::Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(Error::new_spanned(
                    &input,
                    "ToJsonSchema derive only supports structs with named fields",
                ));
            }
        },
        syn::Data::Enum(_) | syn::Data::Union(_) => {
            return Err(Error::new_spanned(
                &input,
                "ToJsonSchema derive only supports structs",
            ));
        }
    };
    let name: Ident = input.ident;

    let title: Option<String> = container_title(&input.attrs)?;
    let title_expr = title
        .as_ref()
        .map(|t| {
            let lit = LitStr::new(t, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let mut property_inserts: Vec<TokenStream2> = Vec::new();
    let mut required_keys: Vec<String> = Vec::new();

    for field in fields {
        let key: String = field_property_key(field)?;
        let span = field
            .ident
            .as_ref()
            .map_or_else(proc_macro2::Span::call_site, syn::spanned::Spanned::span);
        let key_lit: LitStr = LitStr::new(&key, span);
        let ty: &Type = &field.ty;
        let is_opt: bool = is_option_type(ty);
        let schema_ty: &Type = if is_opt { optional_inner_type(ty) } else { ty };
        if !is_opt {
            required_keys.push(key);
        }
        property_inserts.push(quote! {
            properties.insert(#key_lit.to_string(), <#schema_ty as ::json_schema_rs::ToJsonSchema>::json_schema());
        });
    }

    let required_expr = if required_keys.is_empty() {
        quote! { None }
    } else {
        let keys: Vec<TokenStream2> = required_keys
            .iter()
            .map(|k| {
                let lit = LitStr::new(k, name.span());
                quote! { #lit.to_string() }
            })
            .collect();
        quote! { Some(vec![#(#keys),*]) }
    };

    Ok(quote! {
        impl ::json_schema_rs::ToJsonSchema for #name {
            fn json_schema() -> ::json_schema_rs::JsonSchema {
                let mut properties = ::std::collections::BTreeMap::new();
                #(#property_inserts)*
                ::json_schema_rs::JsonSchema {
                    type_: Some("object".to_string()),
                    properties,
                    required: #required_expr,
                    title: #title_expr,
                }
            }
        }
    })
}
