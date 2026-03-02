//! Derive macro for `ToJsonSchema`: builds JSON Schema from struct type and attributes.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    Attribute, DeriveInput, Error, Expr, Field, Fields, Ident, Lit, LitStr, Meta,
    Result as SynResult, Token, Type, Variant,
};

/// Parser for doc attribute value: `= "string"` or just `"string"`.
fn parse_doc_value(input: syn::parse::ParseStream) -> SynResult<String> {
    if input.peek(Token![=]) {
        input.parse::<Token![=]>()?;
    }
    let lit: LitStr = input.parse()?;
    Ok(lit.value())
}

/// Extracts `title = "..."` from `#[json_schema(...)]` container attribute.
fn container_title(attrs: &[Attribute]) -> SynResult<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("json_schema") {
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

/// Extracts `id = "..."` from `#[json_schema(...)]` container attribute.
fn container_id(attrs: &[Attribute]) -> SynResult<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("id") {
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

/// Extracts `description = "..."` from `#[json_schema(...)]` container attribute, or from `///` doc comments (joined with newline). Attribute takes precedence.
fn container_description(attrs: &[Attribute]) -> SynResult<Option<String>> {
    let mut from_attr: Option<String> = None;
    let mut doc_lines: Vec<String> = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("json_schema") {
            let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
            let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
            for meta in metas {
                let Meta::NameValue(nv) = meta else {
                    continue;
                };
                if !nv.path.is_ident("description") {
                    continue;
                }
                let syn::Expr::Lit(expr_lit) = nv.value else {
                    continue;
                };
                let syn::Lit::Str(s) = expr_lit.lit else {
                    continue;
                };
                from_attr = Some(s.value());
                break;
            }
        } else if attr.path().is_ident("doc")
            && let Ok(s) = attr.parse_args_with(parse_doc_value)
        {
            doc_lines.push(s.trim().to_string());
        }
    }
    if let Some(s) = from_attr {
        return Ok(Some(s));
    }
    if doc_lines.is_empty() {
        Ok(None)
    } else {
        Ok(Some(doc_lines.join("\n")))
    }
}

/// Extracts `comment = "..."` from `#[json_schema(...)]` container attribute.
fn container_comment(attrs: &[Attribute]) -> SynResult<Option<String>> {
    for attr in attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("comment") {
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

/// Extracts description from a field's `///` doc comments (joined with newline).
#[expect(clippy::unnecessary_wraps)]
fn field_description(field: &Field) -> SynResult<Option<String>> {
    let mut doc_lines: Vec<String> = Vec::new();
    for attr in &field.attrs {
        if !attr.path().is_ident("doc") {
            continue;
        }
        if let Ok(s) = attr.parse_args_with(parse_doc_value) {
            doc_lines.push(s.trim().to_string());
        }
    }
    if doc_lines.is_empty() {
        Ok(None)
    } else {
        Ok(Some(doc_lines.join("\n")))
    }
}

/// Extracts a numeric value (integer or float literal) from `#[json_schema(key = N)]` on a field.
fn field_numeric_attr(field: &Field, key: &str) -> SynResult<Option<f64>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident(key) {
                continue;
            }
            let value: f64 = match &nv.value {
                Expr::Lit(expr_lit) => match &expr_lit.lit {
                    Lit::Int(lit_int) => {
                        let n: i64 = lit_int.base10_parse()?;
                        #[expect(clippy::cast_precision_loss)]
                        let f: f64 = n as f64;
                        f
                    }
                    Lit::Float(lit_float) => lit_float.base10_parse()?,
                    _ => {
                        return Err(Error::new_spanned(
                            &nv.value,
                            format!(
                                "json_schema({key} = ...) requires an integer or float literal"
                            ),
                        ));
                    }
                },
                _ => {
                    return Err(Error::new_spanned(
                        &nv.value,
                        format!("json_schema({key} = ...) requires an integer or float literal"),
                    ));
                }
            };
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Extracts `minimum = N` from a field's `#[json_schema(...)]` attribute.
fn field_minimum(field: &Field) -> SynResult<Option<f64>> {
    field_numeric_attr(field, "minimum")
}

/// Extracts `maximum = N` from a field's `#[json_schema(...)]` attribute.
fn field_maximum(field: &Field) -> SynResult<Option<f64>> {
    field_numeric_attr(field, "maximum")
}

/// Extracts an integer value (u64) from `#[json_schema(key = N)]` on a field.
fn field_u64_attr(field: &Field, key: &str) -> SynResult<Option<u64>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident(key) {
                continue;
            }
            let value: u64 = match &nv.value {
                Expr::Lit(expr_lit) => match &expr_lit.lit {
                    Lit::Int(lit_int) => lit_int.base10_parse()?,
                    _ => {
                        return Err(Error::new_spanned(
                            &nv.value,
                            format!(
                                "json_schema({key} = ...) requires a non-negative integer literal"
                            ),
                        ));
                    }
                },
                _ => {
                    return Err(Error::new_spanned(
                        &nv.value,
                        format!("json_schema({key} = ...) requires a non-negative integer literal"),
                    ));
                }
            };
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Extracts `min_items = N` from a field's `#[json_schema(...)]` attribute.
fn field_min_items(field: &Field) -> SynResult<Option<u64>> {
    field_u64_attr(field, "min_items")
}

/// Extracts `max_items = N` from a field's `#[json_schema(...)]` attribute.
fn field_max_items(field: &Field) -> SynResult<Option<u64>> {
    field_u64_attr(field, "max_items")
}

/// Extracts `min_length = N` from a field's `#[json_schema(...)]` attribute.
fn field_min_length(field: &Field) -> SynResult<Option<u64>> {
    field_u64_attr(field, "min_length")
}

/// Extracts `max_length = N` from a field's `#[json_schema(...)]` attribute.
fn field_max_length(field: &Field) -> SynResult<Option<u64>> {
    field_u64_attr(field, "max_length")
}

/// Extracts a string value from `#[json_schema(key = "value")]` on a field.
fn field_str_attr(field: &Field, key: &str) -> SynResult<Option<String>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident(key) {
                continue;
            }
            let value: String = match &nv.value {
                Expr::Lit(expr_lit) => match &expr_lit.lit {
                    Lit::Str(lit_str) => lit_str.value(),
                    _ => {
                        return Err(Error::new_spanned(
                            &nv.value,
                            format!("json_schema({key} = ...) requires a string literal"),
                        ));
                    }
                },
                _ => {
                    return Err(Error::new_spanned(
                        &nv.value,
                        format!("json_schema({key} = ...) requires a string literal"),
                    ));
                }
            };
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Extracts `pattern = "..."` from a field's `#[json_schema(...)]` attribute.
fn field_pattern(field: &Field) -> SynResult<Option<String>> {
    field_str_attr(field, "pattern")
}

/// Extracts `default = <literal>` from a field's `#[json_schema(...)]` attribute.
/// Supports string, integer, float, and bool literals (maps to JSON default value).
/// Returns the expression to use for `default_value` (e.g. `Some(serde_json::json!(42))`).
fn field_default(field: &Field) -> SynResult<Option<Expr>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        for meta in metas {
            let Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("default") {
                continue;
            }
            let expr = &nv.value;
            let valid = matches!(
                expr,
                Expr::Lit(expr_lit) if matches!(
                    &expr_lit.lit,
                    Lit::Str(_) | Lit::Int(_) | Lit::Float(_) | Lit::Bool(_)
                )
            );
            if valid {
                return Ok(Some(expr.clone()));
            }
            return Err(Error::new_spanned(
                expr,
                "json_schema(default = ...) requires a string, integer, float, or bool literal",
            ));
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

/// Returns the JSON string value for an enum unit variant: serde rename or variant name.
fn variant_external_name(variant: &Variant) -> SynResult<String> {
    for attr in &variant.attrs {
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
    Ok(variant.ident.to_string())
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

#[expect(clippy::too_many_lines)]
pub fn expand_to_json_schema(input: DeriveInput) -> SynResult<TokenStream2> {
    let name: Ident = input.ident;

    if let syn::Data::Enum(data_enum) = &input.data {
        return expand_enum_to_json_schema(&name, &input.attrs, data_enum);
    }

    let fields = match &input.data {
        syn::Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            Fields::Unnamed(_) | Fields::Unit => {
                return Err(Error::new_spanned(
                    name,
                    "ToJsonSchema derive only supports structs with named fields",
                ));
            }
        },
        syn::Data::Enum(_) | syn::Data::Union(_) => {
            return Err(Error::new_spanned(
                name,
                "ToJsonSchema derive only supports structs and unit enums",
            ));
        }
    };

    let title: Option<String> = container_title(&input.attrs)?;
    let title_expr = title
        .as_ref()
        .map(|t| {
            let lit = LitStr::new(t, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let description: Option<String> = container_description(&input.attrs)?;
    let description_expr = description
        .as_ref()
        .map(|d| {
            let lit = LitStr::new(d, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let comment: Option<String> = container_comment(&input.attrs)?;
    let comment_expr = comment
        .as_ref()
        .map(|c| {
            let lit = LitStr::new(c, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let id: Option<String> = container_id(&input.attrs)?;
    let id_expr = id
        .as_ref()
        .map(|i| {
            let lit = LitStr::new(i, proc_macro2::Span::call_site());
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
        let field_desc: Option<String> = field_description(field)?;
        let field_desc_expr = field_desc
            .as_ref()
            .map(|d| {
                let lit = LitStr::new(d, span);
                quote! { Some(#lit.to_string()) }
            })
            .unwrap_or(quote! { None });
        let field_min: Option<f64> = field_minimum(field)?;
        let field_max: Option<f64> = field_maximum(field)?;
        let field_min_items: Option<u64> = field_min_items(field)?;
        let field_max_items: Option<u64> = field_max_items(field)?;
        let field_min_length: Option<u64> = field_min_length(field)?;
        let field_max_length: Option<u64> = field_max_length(field)?;
        let field_pattern_val: Option<String> = field_pattern(field)?;
        let field_default_expr: Option<Expr> = field_default(field)?;
        let default_value_override: Option<TokenStream2> =
            field_default_expr.as_ref().map(|expr| {
                quote! { Some(::serde_json::json!(#expr)) }
            });
        let set_default_value: TokenStream2 = if let Some(ref ov) = default_value_override {
            quote! { schema.default_value = #ov; }
        } else {
            quote! {}
        };
        let min_expr: TokenStream2 = if let Some(m) = field_min {
            let lit = proc_macro2::Literal::f64_unsuffixed(m);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let max_expr: TokenStream2 = if let Some(m) = field_max {
            let lit = proc_macro2::Literal::f64_unsuffixed(m);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let min_items_expr: TokenStream2 = if let Some(n) = field_min_items {
            let lit = proc_macro2::Literal::u64_unsuffixed(n);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let max_items_expr: TokenStream2 = if let Some(n) = field_max_items {
            let lit = proc_macro2::Literal::u64_unsuffixed(n);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let min_length_expr: TokenStream2 = if let Some(n) = field_min_length {
            let lit = proc_macro2::Literal::u64_unsuffixed(n);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let max_length_expr: TokenStream2 = if let Some(n) = field_max_length {
            let lit = proc_macro2::Literal::u64_unsuffixed(n);
            quote! { Some(#lit) }
        } else {
            quote! { None }
        };
        let pattern_expr: TokenStream2 = if let Some(ref p) = field_pattern_val {
            let lit = LitStr::new(p, span);
            quote! { Some(#lit.to_string()) }
        } else {
            quote! { None }
        };
        property_inserts.push(quote! {
            {
                let base = <#schema_ty as ::json_schema_rs::ToJsonSchema>::json_schema();
                let mut schema = base.clone();
                schema.description = #field_desc_expr.or(schema.description);
                schema.minimum = #min_expr.or(schema.minimum);
                schema.maximum = #max_expr.or(schema.maximum);
                schema.min_items = #min_items_expr.or(schema.min_items);
                schema.max_items = #max_items_expr.or(schema.max_items);
                schema.min_length = #min_length_expr.or(schema.min_length);
                schema.max_length = #max_length_expr.or(schema.max_length);
                schema.pattern = #pattern_expr.or(schema.pattern);
                #set_default_value
                properties.insert(#key_lit.to_string(), schema);
            }
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
                    schema: Some(::json_schema_rs::SpecVersion::Draft202012.schema_uri().to_string()),
                    id: #id_expr,
                    type_: Some("object".to_string()),
                    properties,
                    additional_properties: Some(::json_schema_rs::json_schema::json_schema::AdditionalProperties::Forbid),
                    required: #required_expr,
                    title: #title_expr,
                    description: #description_expr,
                    comment: #comment_expr,
                    enum_values: None,
                    const_value: None,
                    items: None,
                    unique_items: None,
                    min_items: None,
                    max_items: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    format: None,
                    default_value: None,
                    all_of: None,
                    any_of: None,
                    one_of: None,
                }
            }
        }
    })
}

/// Expand `ToJsonSchema` for a unit enum: emit schema with type "string" and `enum_values`.
fn expand_enum_to_json_schema(
    name: &Ident,
    attrs: &[Attribute],
    data_enum: &syn::DataEnum,
) -> SynResult<TokenStream2> {
    let title: Option<String> = container_title(attrs)?;
    let title_expr = title
        .as_ref()
        .map(|t| {
            let lit = LitStr::new(t, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let description: Option<String> = container_description(attrs)?;
    let description_expr = description
        .as_ref()
        .map(|d| {
            let lit = LitStr::new(d, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let comment: Option<String> = container_comment(attrs)?;
    let comment_expr = comment
        .as_ref()
        .map(|c| {
            let lit = LitStr::new(c, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let id: Option<String> = container_id(attrs)?;
    let id_expr = id
        .as_ref()
        .map(|i| {
            let lit = LitStr::new(i, proc_macro2::Span::call_site());
            quote! { Some(#lit.to_string()) }
        })
        .unwrap_or(quote! { None });

    let mut enum_value_lits: Vec<LitStr> = Vec::new();
    for variant in &data_enum.variants {
        match &variant.fields {
            Fields::Unit => {}
            Fields::Unnamed(_) | Fields::Named(_) => {
                return Err(Error::new_spanned(
                    variant,
                    "ToJsonSchema derive for enum only supports unit variants",
                ));
            }
        }
        let external: String = variant_external_name(variant)?;
        enum_value_lits.push(LitStr::new(&external, variant.ident.span()));
    }

    let is_single_variant: bool = enum_value_lits.len() == 1;

    let (enum_values_expr, const_value_expr): (TokenStream2, TokenStream2) = if is_single_variant {
        let lit = &enum_value_lits[0];
        (
            quote! { None },
            quote! { Some(::serde_json::Value::String(#lit.to_string())) },
        )
    } else {
        (
            quote! {
                Some(vec![
                    #(::serde_json::Value::String(#enum_value_lits.to_string())),*
                ])
            },
            quote! { None },
        )
    };

    Ok(quote! {
        impl ::json_schema_rs::ToJsonSchema for #name {
            fn json_schema() -> ::json_schema_rs::JsonSchema {
                ::json_schema_rs::JsonSchema {
                    schema: Some(::json_schema_rs::SpecVersion::Draft202012.schema_uri().to_string()),
                    id: #id_expr,
                    type_: Some("string".to_string()),
                    properties: ::std::collections::BTreeMap::new(),
                    additional_properties: None,
                    required: None,
                    title: #title_expr,
                    description: #description_expr,
                    comment: #comment_expr,
                    enum_values: #enum_values_expr,
                    const_value: #const_value_expr,
                    items: None,
                    unique_items: None,
                    min_items: None,
                    max_items: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                    pattern: None,
                    format: None,
                    default_value: None,
                    all_of: None,
                    any_of: None,
                    one_of: None,
                }
            }
        }
    })
}
