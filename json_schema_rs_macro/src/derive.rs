//! Derive macro for `ToJsonSchema`: builds JSON Schema from struct type and attributes.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{
    Attribute, DeriveInput, Error, Expr, Field, Fields, Ident, Lit, LitStr, Meta,
    Result as SynResult, Token, Type, Variant,
};

/// Collects the values of `#[doc = ...]` attributes (what `///` desugars to), in order.
///
/// A value is a string literal or, as rustc allows, a macro that expands to one
/// (`include_str!`, `concat!`); both are kept as expressions and evaluated in the generated
/// code, so no doc line is dropped. List-form attributes (`#[doc(hidden)]`) carry no text and
/// are skipped.
fn doc_exprs(attrs: &[Attribute]) -> Vec<&Expr> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(nv) => Some(&nv.value),
            Meta::Path(_) | Meta::List(_) => None,
        })
        .collect()
}

/// Builds an `Option<String>` expression from doc values: each value trimmed, joined with `\n`,
/// leading/trailing blank lines removed; `None` when no doc text remains.
fn doc_description_expr(exprs: &[&Expr]) -> TokenStream2 {
    if exprs.is_empty() {
        return quote! { None };
    }
    quote! {
        {
            let lines: ::std::vec::Vec<&str> = [#(#exprs),*].iter().map(|line: &&str| line.trim()).collect();
            let joined: ::std::string::String = lines.join("\n").trim().to_string();
            if joined.is_empty() { None } else { Some(joined) }
        }
    }
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

/// Builds the container's `Option<String>` description expression: `description = "..."` from
/// the `#[json_schema(...)]` attribute if present, else the `///` doc comments (see
/// [`doc_description_expr`]). The attribute takes precedence.
fn container_description(attrs: &[Attribute]) -> SynResult<TokenStream2> {
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
            if !nv.path.is_ident("description") {
                continue;
            }
            let syn::Expr::Lit(expr_lit) = nv.value else {
                continue;
            };
            let syn::Lit::Str(s) = expr_lit.lit else {
                continue;
            };
            return Ok(quote! { Some(#s.to_string()) });
        }
    }
    Ok(doc_description_expr(&doc_exprs(attrs)))
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

/// A numeric attribute value, kept exactly as written: integers stay integers.
#[derive(Debug, Clone, Copy)]
enum NumericLiteral {
    Unsigned(u64),
    Signed(i64),
    Float(f64),
}

impl NumericLiteral {
    /// An expression of type `Option<serde_json::Number>` for this value.
    fn to_number_expr(self) -> TokenStream2 {
        match self {
            NumericLiteral::Unsigned(n) => {
                let lit = proc_macro2::Literal::u64_suffixed(n);
                quote! { Some(::serde_json::Number::from(#lit)) }
            }
            NumericLiteral::Signed(i64::MIN) => {
                quote! { Some(::serde_json::Number::from(i64::MIN)) }
            }
            NumericLiteral::Signed(n) => {
                let lit = proc_macro2::Literal::i64_suffixed(n.abs());
                if n < 0 {
                    quote! { Some(::serde_json::Number::from(-#lit)) }
                } else {
                    quote! { Some(::serde_json::Number::from(#lit)) }
                }
            }
            NumericLiteral::Float(f) => {
                let lit = proc_macro2::Literal::f64_suffixed(f.abs());
                if f.is_sign_negative() {
                    quote! { ::serde_json::Number::from_f64(-#lit) }
                } else {
                    quote! { ::serde_json::Number::from_f64(#lit) }
                }
            }
        }
    }
}

/// Reads an integer or float literal, optionally negated (`-5`, `1.5`), as written.
fn numeric_literal(expr: &Expr, key: &str) -> SynResult<NumericLiteral> {
    let error = || {
        Error::new_spanned(
            expr,
            format!("json_schema({key} = ...) requires an integer or float literal"),
        )
    };
    let (negative, lit): (bool, &Lit) = match expr {
        Expr::Lit(expr_lit) => (false, &expr_lit.lit),
        Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr: inner,
            ..
        }) => match inner.as_ref() {
            Expr::Lit(expr_lit) => (true, &expr_lit.lit),
            _ => return Err(error()),
        },
        _ => return Err(error()),
    };
    match lit {
        Lit::Int(lit_int) => {
            let magnitude: u64 = lit_int.base10_parse()?;
            if !negative {
                return Ok(NumericLiteral::Unsigned(magnitude));
            }
            let value: i64 = 0_i64.checked_sub_unsigned(magnitude).ok_or_else(|| {
                Error::new_spanned(expr, format!("json_schema({key} = ...) is below i64::MIN"))
            })?;
            Ok(NumericLiteral::Signed(value))
        }
        Lit::Float(lit_float) => {
            let magnitude: f64 = lit_float.base10_parse()?;
            Ok(NumericLiteral::Float(if negative {
                -magnitude
            } else {
                magnitude
            }))
        }
        _ => Err(error()),
    }
}

/// Extracts a numeric value (integer or float literal, optionally negated) from
/// `#[json_schema(key = N)]` on a field, exactly as written.
fn field_numeric_attr(field: &Field, key: &str) -> SynResult<Option<NumericLiteral>> {
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
            return numeric_literal(&nv.value, key).map(Some);
        }
    }
    Ok(None)
}

/// An `Option<serde_json::Number>` expression for the numeric attribute `key`, `None` when absent.
fn field_number_expr(field: &Field, key: &str) -> SynResult<TokenStream2> {
    Ok(field_numeric_attr(field, key)?
        .map_or_else(|| quote! { None }, NumericLiteral::to_number_expr))
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

/// Extracts `deprecated = true` from a field's `#[json_schema(...)]` attribute.
fn field_deprecated(field: &Field) -> SynResult<Option<bool>> {
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
            if !nv.path.is_ident("deprecated") {
                continue;
            }
            let syn::Expr::Lit(expr_lit) = &nv.value else {
                return Err(Error::new_spanned(
                    &nv.value,
                    "json_schema(deprecated = ...) requires a boolean literal (true or false)",
                ));
            };
            let syn::Lit::Bool(b) = &expr_lit.lit else {
                return Err(Error::new_spanned(
                    &expr_lit.lit,
                    "json_schema(deprecated = ...) requires a boolean literal (true or false)",
                ));
            };
            return Ok(Some(b.value()));
        }
    }
    Ok(None)
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

    let description_expr: TokenStream2 = container_description(&input.attrs)?;

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
        let field_desc_expr: TokenStream2 = doc_description_expr(&doc_exprs(&field.attrs));
        let min_expr: TokenStream2 = field_number_expr(field, "minimum")?;
        let max_expr: TokenStream2 = field_number_expr(field, "maximum")?;
        let exclusive_min_expr: TokenStream2 = field_number_expr(field, "exclusive_minimum")?;
        let exclusive_max_expr: TokenStream2 = field_number_expr(field, "exclusive_maximum")?;
        let multiple_of_expr: TokenStream2 = field_number_expr(field, "multiple_of")?;
        let field_min_items: Option<u64> = field_min_items(field)?;
        let field_max_items: Option<u64> = field_max_items(field)?;
        let field_min_length: Option<u64> = field_min_length(field)?;
        let field_max_length: Option<u64> = field_max_length(field)?;
        let field_pattern_val: Option<String> = field_pattern(field)?;
        let field_deprecated_val: Option<bool> = field_deprecated(field)?;
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

        let deprecated_expr: TokenStream2 = if field_deprecated_val == Some(true) {
            quote! { Some(true) }
        } else {
            quote! { None }
        };

        property_inserts.push(quote! {
            {
                let mut schema = <#schema_ty as ::json_schema_rs::ToJsonSchema>::json_schema_with(generator);
                schema.description = #field_desc_expr.or(schema.description);
                schema.minimum = #min_expr.or(schema.minimum);
                schema.maximum = #max_expr.or(schema.maximum);
                schema.exclusive_minimum = #exclusive_min_expr.or(schema.exclusive_minimum);
                schema.exclusive_maximum = #exclusive_max_expr.or(schema.exclusive_maximum);
                schema.multiple_of = #multiple_of_expr.or(schema.multiple_of);
                schema.min_items = #min_items_expr.or(schema.min_items);
                schema.max_items = #max_items_expr.or(schema.max_items);
                schema.min_length = #min_length_expr.or(schema.min_length);
                schema.max_length = #max_length_expr.or(schema.max_length);
                schema.pattern = #pattern_expr.or(schema.pattern);
                schema.deprecated = #deprecated_expr.or(schema.deprecated);
                #set_default_value
                properties.insert(#key_lit.to_string(), ::json_schema_rs::Schema::from(schema));
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

    // A struct with no fields never touches the generator or mutates `properties`.
    let (generator_ident, properties_mut): (TokenStream2, TokenStream2) =
        if property_inserts.is_empty() {
            (quote! { _generator }, quote! {})
        } else {
            (quote! { generator }, quote! { mut })
        };
    let body: TokenStream2 = quote! {
        let #properties_mut properties = ::std::collections::BTreeMap::new();
        #(#property_inserts)*
        ::json_schema_rs::JsonSchema {
            id: #id_expr,
            type_: Some(::json_schema_rs::TypeSet::from(::json_schema_rs::JsonType::Object)),
            properties,
            additional_properties: Some(::json_schema_rs::Schema::Bool(false)),
            required: #required_expr,
            title: #title_expr,
            description: #description_expr,
            comment: #comment_expr,
            ..::json_schema_rs::JsonSchema::default()
        }
    };
    Ok(named_type_impl(
        &name,
        id.as_deref(),
        &generator_ident,
        &body,
    ))
}

/// Emits the `ToJsonSchema` impl for a named type (derived struct or enum) whose schema body
/// is `body`, an expression that may use the generator bound to `generator_ident`.
///
/// The body lives in a hidden inherent fn so it is written once: `json_schema_with` registers
/// it in the generator's `$defs` and returns a `$ref`; `json_schema` builds it as the root
/// with a fresh generator and adds `$schema`. The type's identity in the generator is its full
/// path (`module_path!()` expands where the type is defined), so same-name types from
/// different modules get separate `$defs` entries.
///
/// A type with `id` is its own schema resource, built through the generator's resource
/// methods so it carries the `$defs` its refs need and refers to itself as `#`.
fn named_type_impl(
    name: &Ident,
    id: Option<&str>,
    generator_ident: &TokenStream2,
    body: &TokenStream2,
) -> TokenStream2 {
    let name_lit: LitStr = LitStr::new(&name.to_string(), name.span());
    let (root_call, subschema_call): (TokenStream2, TokenStream2) = if let Some(id) = id {
        let id_lit: LitStr = LitStr::new(id, name.span());
        (
            quote! {
                .into_resource_root_schema(
                    <Self as ::json_schema_rs::ToJsonSchema>::schema_id(),
                    #id_lit,
                    Self::__json_schema_rs_body,
                )
            },
            quote! {
                generator.resource_subschema(
                    <Self as ::json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    #id_lit,
                    Self::__json_schema_rs_body,
                )
            },
        )
    } else {
        (
            quote! {
                .into_named_root_schema(
                    <Self as ::json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    Self::__json_schema_rs_body,
                )
            },
            quote! {
                generator.named_subschema(
                    <Self as ::json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    Self::__json_schema_rs_body,
                )
            },
        )
    };
    quote! {
        impl #name {
            #[doc(hidden)]
            fn __json_schema_rs_body(
                #generator_ident: &mut ::json_schema_rs::SchemaGenerator,
            ) -> ::json_schema_rs::JsonSchema {
                #body
            }
        }

        impl ::json_schema_rs::ToJsonSchema for #name {
            fn json_schema() -> ::json_schema_rs::JsonSchema {
                let mut schema: ::json_schema_rs::JsonSchema = ::json_schema_rs::SchemaGenerator::new()
                    #root_call;
                schema.schema = Some(::json_schema_rs::SpecVersion::Draft202012.schema_uri().to_string());
                schema
            }

            fn json_schema_with(
                generator: &mut ::json_schema_rs::SchemaGenerator,
            ) -> ::json_schema_rs::JsonSchema {
                #subschema_call
            }

            fn schema_id() -> &'static str {
                ::core::concat!(::core::module_path!(), "::", #name_lit)
            }
        }
    }
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

    let description_expr: TokenStream2 = container_description(attrs)?;

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

    let body: TokenStream2 = quote! {
        ::json_schema_rs::JsonSchema {
            id: #id_expr,
            type_: Some(::json_schema_rs::TypeSet::from(::json_schema_rs::JsonType::String)),
            title: #title_expr,
            description: #description_expr,
            comment: #comment_expr,
            enum_values: #enum_values_expr,
            const_value: #const_value_expr,
            ..::json_schema_rs::JsonSchema::default()
        }
    };
    Ok(named_type_impl(
        name,
        id.as_deref(),
        &quote! { _generator },
        &body,
    ))
}
