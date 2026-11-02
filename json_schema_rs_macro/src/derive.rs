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
    // syn reads `-5` in an attribute value as one negative literal (digits `-5`), and `-(5)`
    // style input as a negation of a literal: both are negative.
    match lit {
        Lit::Int(lit_int) => {
            let digits: &str = lit_int.base10_digits();
            let (negative, magnitude): (bool, &str) = match digits.strip_prefix('-') {
                Some(rest) => (!negative, rest),
                None => (negative, digits),
            };
            let magnitude: u64 = magnitude
                .parse()
                .map_err(|e| Error::new_spanned(expr, format!("json_schema({key} = ...): {e}")))?;
            if !negative {
                return Ok(NumericLiteral::Unsigned(magnitude));
            }
            let value: i64 = 0_i64.checked_sub_unsigned(magnitude).ok_or_else(|| {
                Error::new_spanned(expr, format!("json_schema({key} = ...) is below i64::MIN"))
            })?;
            Ok(NumericLiteral::Signed(value))
        }
        Lit::Float(lit_float) => {
            let value: f64 = lit_float.base10_parse()?;
            Ok(NumericLiteral::Float(if negative { -value } else { value }))
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

/// Extracts `default = <literal>` from a field's `#[json_schema(...)]` attribute as an
/// expression of type `Option<serde_json::Value>`: a string, a boolean, or an integer or float
/// literal (optionally negated), kept exact (`18446744073709551615`, `-5`, `2.5`).
fn field_default(field: &Field) -> SynResult<Option<TokenStream2>> {
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
            let expr: &Expr = &nv.value;
            if let Expr::Lit(expr_lit) = expr {
                match &expr_lit.lit {
                    Lit::Str(lit) => {
                        return Ok(Some(
                            quote! { Some(::serde_json::Value::String(#lit.to_string())) },
                        ));
                    }
                    Lit::Bool(lit) => {
                        return Ok(Some(quote! { Some(::serde_json::Value::Bool(#lit)) }));
                    }
                    _ => {}
                }
            }
            let number: TokenStream2 = numeric_literal(expr, "default")
                .map_err(|_| {
                    Error::new_spanned(
                        expr,
                        "json_schema(default = ...) requires a string, integer, float, or bool literal",
                    )
                })?
                .to_number_expr();
            return Ok(Some(quote! { (#number).map(::serde_json::Value::Number) }));
        }
    }
    Ok(None)
}

/// One argument of a `#[serde(...)]` attribute: its name and, for `name = "..."`, the string.
struct SerdeArg {
    name: String,
    value: Option<String>,
}

/// The arguments of every `#[serde(...)]` attribute in `attrs`, in order.
///
/// Serde attributes combine several arguments in one attribute
/// (`#[serde(rename = "k", default = "f", skip_serializing_if = "Option::is_none")]`), and an
/// argument may be a bare flag (`flatten`), take a value of any expression form, or take a
/// parenthesized list (`rename(serialize = "a")`). Every form is read so the arguments this
/// derive does not use are skipped without error; serde itself validates them.
fn serde_args(attrs: &[Attribute]) -> SynResult<Vec<SerdeArg>> {
    let mut args: Vec<SerdeArg> = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            let name: String = meta
                .path
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default();
            let mut value: Option<String> = None;
            if meta.input.peek(Token![=]) {
                let expr: Expr = meta.value()?.parse()?;
                if let Expr::Lit(syn::ExprLit {
                    lit: Lit::Str(lit_str),
                    ..
                }) = expr
                {
                    value = Some(lit_str.value());
                }
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                content.parse::<TokenStream2>()?;
            }
            args.push(SerdeArg { name, value });
            Ok(())
        })?;
    }
    Ok(args)
}

/// The string value of `#[serde(key = "...")]` in `attrs`, if present.
fn serde_str(attrs: &[Attribute], key: &str) -> SynResult<Option<String>> {
    Ok(serde_args(attrs)?
        .into_iter()
        .find(|arg| arg.name == key)
        .and_then(|arg| arg.value))
}

/// Whether `attrs` carry the bare serde flag `#[serde(flag)]` (e.g. `flatten`, `untagged`).
fn serde_flag(attrs: &[Attribute], flag: &str) -> SynResult<bool> {
    Ok(serde_args(attrs)?
        .iter()
        .any(|arg| arg.name == flag && arg.value.is_none()))
}

/// Whether `attrs` carry the bare flag `#[json_schema(flag)]` (e.g. `required`, `one_of`).
fn json_schema_flag(attrs: &[Attribute], flag: &str) -> SynResult<bool> {
    for attr in attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
        let metas: Punctuated<Meta, Token![,]> = attr.parse_args_with(parser)?;
        if metas
            .iter()
            .any(|meta| matches!(meta, Meta::Path(path) if path.is_ident(flag)))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Returns the JSON property key for a field: serde rename or field name.
fn field_property_key(field: &Field) -> SynResult<String> {
    if let Some(renamed) = serde_str(&field.attrs, "rename")? {
        return Ok(renamed);
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

/// The value type `V` of `BTreeMap<String, V>`, or `None` for any other type.
fn btree_map_value_type(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let seg = type_path.path.segments.last()?;
    if seg.ident != "BTreeMap" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    let mut types = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    });
    let (Some(key), Some(value), None) = (types.next(), types.next(), types.next()) else {
        return None;
    };
    let Type::Path(key_path) = key else {
        return None;
    };
    key_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "String")
        .then_some(value)
}

/// Returns the serde name of an enum variant: serde rename or variant name.
fn variant_external_name(variant: &Variant) -> SynResult<String> {
    Ok(serde_str(&variant.attrs, "rename")?.unwrap_or_else(|| variant.ident.to_string()))
}

/// The tokens of `#[json_schema(value = <JSON literal>)]` on an enum variant, if present.
///
/// The value is any JSON literal in `serde_json::json!` syntax (`"x"`, `1`, `-3`, `1.5`,
/// `true`, `null`, `[1, 2]`, `{"a": 1}`), so the tokens up to the next top-level comma are
/// kept as written and expanded through `json!` in the generated code. `value` is the only
/// argument a variant takes.
fn variant_value_tokens(variant: &Variant) -> SynResult<Option<TokenStream2>> {
    let mut found: Option<TokenStream2> = None;
    for attr in &variant.attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        attr.parse_args_with(|input: syn::parse::ParseStream| {
            while !input.is_empty() {
                let key: Ident = input.parse()?;
                if key != "value" {
                    return Err(Error::new_spanned(
                        &key,
                        "unsupported json_schema attribute on an enum variant (expected `value = <JSON literal>`)",
                    ));
                }
                input.parse::<Token![=]>()?;
                let mut tokens: TokenStream2 = TokenStream2::new();
                while !input.is_empty() && !input.peek(Token![,]) {
                    tokens.extend(std::iter::once(input.parse::<proc_macro2::TokenTree>()?));
                }
                if tokens.is_empty() {
                    return Err(Error::new_spanned(
                        &key,
                        "json_schema(value = ...) requires a JSON literal",
                    ));
                }
                found = Some(tokens);
                if !input.is_empty() {
                    input.parse::<Token![,]>()?;
                }
            }
            Ok(())
        })?;
    }
    Ok(found)
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

/// The container-level `#[json_schema(...)]` values every derived type carries, as
/// expressions for the generated `JsonSchema` fields.
struct ContainerExprs {
    title: TokenStream2,
    description: TokenStream2,
    comment: TokenStream2,
    id_expr: TokenStream2,
    id: Option<String>,
}

/// Reads `title`, `description` (or doc comments), `comment` and `id` from a container.
fn container_exprs(attrs: &[Attribute]) -> SynResult<ContainerExprs> {
    let optional_string = |value: Option<&String>| -> TokenStream2 {
        value.map_or_else(
            || quote! { None },
            |v| {
                let lit = LitStr::new(v, proc_macro2::Span::call_site());
                quote! { Some(#lit.to_string()) }
            },
        )
    };
    let id: Option<String> = container_id(attrs)?;
    Ok(ContainerExprs {
        title: optional_string(container_title(attrs)?.as_ref()),
        description: container_description(attrs)?,
        comment: optional_string(container_comment(attrs)?.as_ref()),
        id_expr: optional_string(id.as_ref()),
        id,
    })
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
                "ToJsonSchema derive only supports structs and enums",
            ));
        }
    };

    let container: ContainerExprs = container_exprs(&input.attrs)?;
    let ContainerExprs {
        title: title_expr,
        description: description_expr,
        comment: comment_expr,
        id_expr,
        id,
    } = container;

    let mut property_inserts: Vec<TokenStream2> = Vec::new();
    let mut required_keys: Vec<String> = Vec::new();
    // `additionalProperties`: closed (`false`) unless a `#[serde(flatten)]` map collects the
    // keys that are not properties.
    let mut additional_properties_expr: Option<TokenStream2> = None;

    for field in fields {
        let ty: &Type = &field.ty;
        if serde_flag(&field.attrs, "flatten")? {
            let Some(value_ty) = btree_map_value_type(ty) else {
                return Err(Error::new_spanned(
                    ty,
                    "ToJsonSchema derive supports #[serde(flatten)] only on a BTreeMap<String, V> field (it becomes additionalProperties)",
                ));
            };
            if additional_properties_expr.is_some() {
                return Err(Error::new_spanned(
                    field,
                    "ToJsonSchema derive supports at most one #[serde(flatten)] field",
                ));
            }
            // A map of any value (`{}`) is the same as no `additionalProperties`: leave it out.
            additional_properties_expr = Some(quote! {
                {
                    let value_schema: ::json_schema_rs::JsonSchema =
                        <#value_ty as ::json_schema_rs::ToJsonSchema>::json_schema_with(generator);
                    if value_schema == ::json_schema_rs::JsonSchema::default() {
                        None
                    } else {
                        Some(::json_schema_rs::Schema::from(value_schema))
                    }
                }
            });
            continue;
        }
        let key: String = field_property_key(field)?;
        let span = field
            .ident
            .as_ref()
            .map_or_else(proc_macro2::Span::call_site, syn::spanned::Spanned::span);
        let key_lit: LitStr = LitStr::new(&key, span);
        // An `Option` field is optional (one `Option` stripped) unless marked
        // `#[json_schema(required)]`: then it is required and keeps `Option<T>`'s nullable
        // schema.
        let is_required: bool = json_schema_flag(&field.attrs, "required")?;
        let is_opt: bool = is_option_type(ty) && !is_required;
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
        let unique_items_expr: TokenStream2 = if json_schema_flag(&field.attrs, "unique_items")? {
            quote! { Some(true) }
        } else {
            quote! { None }
        };
        let default_value_override: Option<TokenStream2> = field_default(field)?;
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
                schema.unique_items = #unique_items_expr.or(schema.unique_items);
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
    let properties_mut: TokenStream2 = if property_inserts.is_empty() {
        quote! {}
    } else {
        quote! { mut }
    };
    let generator_ident: TokenStream2 =
        if property_inserts.is_empty() && additional_properties_expr.is_none() {
            quote! { _generator }
        } else {
            quote! { generator }
        };
    let additional_properties_expr: TokenStream2 = additional_properties_expr
        .unwrap_or_else(|| quote! { Some(::json_schema_rs::Schema::Bool(false)) });
    let body: TokenStream2 = quote! {
        let #properties_mut properties = ::std::collections::BTreeMap::new();
        #(#property_inserts)*
        let additional_properties: ::core::option::Option<::json_schema_rs::Schema> =
            #additional_properties_expr;
        ::json_schema_rs::JsonSchema {
            id: #id_expr,
            type_: Some(::json_schema_rs::TypeSet::from(::json_schema_rs::JsonType::Object)),
            properties,
            additional_properties,
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
                    #name_lit,
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
            fn json_schema() -> ::json_schema_rs::ReverseCodeGenResult {
                let mut schema: ::json_schema_rs::JsonSchema = ::json_schema_rs::SchemaGenerator::new()
                    #root_call?;
                schema.schema = Some(::json_schema_rs::SpecVersion::Draft202012.schema_uri().to_string());
                ::core::result::Result::Ok(schema)
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

/// Expand `ToJsonSchema` for an enum.
///
/// An enum of unit variants is a value enum (see [`value_enum_body`]). An enum of single-field
/// tuple variants with `#[serde(untagged)]` is a union (see [`union_enum_body`]). Any other
/// shape is an error.
fn expand_enum_to_json_schema(
    name: &Ident,
    attrs: &[Attribute],
    data_enum: &syn::DataEnum,
) -> SynResult<TokenStream2> {
    let container: ContainerExprs = container_exprs(attrs)?;
    let all_unit: bool = data_enum
        .variants
        .iter()
        .all(|variant| matches!(variant.fields, Fields::Unit));
    let (body, generator_ident): (TokenStream2, TokenStream2) = if all_unit {
        (
            value_enum_body(&container, data_enum)?,
            quote! { _generator },
        )
    } else {
        (
            union_enum_body(name, attrs, &container, data_enum)?,
            quote! { generator },
        )
    };
    Ok(named_type_impl(
        name,
        container.id.as_deref(),
        &generator_ident,
        &body,
    ))
}

/// The schema of an enum of unit variants: its variants' values as `enum` (`const` for one
/// variant).
///
/// A variant's value is its `#[json_schema(value = <JSON literal>)]`, else its serde name as a
/// string. When every value is a serde name the schema also has `"type": "string"`; when any
/// variant has `value`, the values may be of any JSON type and no `type` is emitted. An enum
/// with no variants is `{"enum": []}`, which no instance matches.
fn value_enum_body(
    container: &ContainerExprs,
    data_enum: &syn::DataEnum,
) -> SynResult<TokenStream2> {
    let mut values: Vec<TokenStream2> = Vec::new();
    let mut any_explicit_value: bool = false;
    for variant in &data_enum.variants {
        if let Some(tokens) = variant_value_tokens(variant)? {
            any_explicit_value = true;
            values.push(quote! { ::serde_json::json!(#tokens) });
        } else {
            let external: String = variant_external_name(variant)?;
            let lit: LitStr = LitStr::new(&external, variant.ident.span());
            values.push(quote! { ::serde_json::Value::String(#lit.to_string()) });
        }
    }

    let type_expr: TokenStream2 = if any_explicit_value || values.is_empty() {
        quote! { None }
    } else {
        quote! { Some(::json_schema_rs::TypeSet::from(::json_schema_rs::JsonType::String)) }
    };
    let (enum_values_expr, const_value_expr): (TokenStream2, TokenStream2) =
        if let [value] = values.as_slice() {
            (quote! { None }, quote! { Some(#value) })
        } else {
            (
                quote! {
                    Some(::std::vec::Vec::<::serde_json::Value>::from([#(#values),*]))
                },
                quote! { None },
            )
        };

    let ContainerExprs {
        title,
        description,
        comment,
        id_expr,
        ..
    } = container;
    Ok(quote! {
        ::json_schema_rs::JsonSchema {
            id: #id_expr,
            type_: #type_expr,
            title: #title,
            description: #description,
            comment: #comment,
            enum_values: #enum_values_expr,
            const_value: #const_value_expr,
            ..::json_schema_rs::JsonSchema::default()
        }
    })
}

/// The schema of an untagged enum whose variants each wrap one value: `anyOf` (or `oneOf` with
/// `#[json_schema(one_of)]`) of the variants' payload schemas, in variant order.
///
/// Each payload schema comes from `json_schema_with`, so a named payload type lands in `$defs`
/// behind a `$ref` exactly as a struct field does (a recursive payload included).
///
/// With `#[json_schema(flatten_single_entry)]`, an enum of exactly one variant is its payload's
/// schema with the container's `title`, `description`, `comment` and `id` on top (each one
/// set replaces the payload's), instead of a one-entry `anyOf` / `oneOf`: the two are
/// equivalent for validation. A payload that is itself a schema resource (`$id`, from a
/// hand-written impl) keeps the combinator, since moving it to the enum's position would change
/// what its `$id` and the `$ref`s inside it resolve against.
fn union_enum_body(
    name: &Ident,
    attrs: &[Attribute],
    container: &ContainerExprs,
    data_enum: &syn::DataEnum,
) -> SynResult<TokenStream2> {
    let shape_error = |span: &dyn quote::ToTokens| {
        Error::new_spanned(
            span,
            "ToJsonSchema derive for enum supports unit variants, or single-field tuple variants under #[serde(untagged)]",
        )
    };
    if !serde_flag(attrs, "untagged")? {
        return Err(shape_error(name));
    }
    let mut payload_types: Vec<&Type> = Vec::new();
    for variant in &data_enum.variants {
        let Fields::Unnamed(unnamed) = &variant.fields else {
            return Err(shape_error(variant));
        };
        let (Some(field), 1) = (unnamed.unnamed.first(), unnamed.unnamed.len()) else {
            return Err(shape_error(variant));
        };
        if variant_value_tokens(variant)?.is_some() {
            return Err(Error::new_spanned(
                variant,
                "json_schema(value = ...) applies to unit variants only",
            ));
        }
        payload_types.push(&field.ty);
    }
    let combinator_field: Ident = if json_schema_flag(attrs, "one_of")? {
        Ident::new("one_of", proc_macro2::Span::call_site())
    } else {
        Ident::new("any_of", proc_macro2::Span::call_site())
    };
    let ContainerExprs {
        title,
        description,
        comment,
        id_expr,
        ..
    } = container;
    if let [payload] = payload_types.as_slice()
        && json_schema_flag(attrs, "flatten_single_entry")?
    {
        return Ok(quote! {
            let payload: ::json_schema_rs::JsonSchema =
                <#payload as ::json_schema_rs::ToJsonSchema>::json_schema_with(generator);
            if payload.id.is_some() {
                ::json_schema_rs::JsonSchema {
                    id: #id_expr,
                    title: #title,
                    description: #description,
                    comment: #comment,
                    #combinator_field: Some(::std::vec::Vec::from([
                        ::json_schema_rs::Schema::from(payload),
                    ])),
                    ..::json_schema_rs::JsonSchema::default()
                }
            } else {
                ::json_schema_rs::JsonSchema {
                    id: #id_expr,
                    title: #title.or(payload.title),
                    description: #description.or(payload.description),
                    comment: #comment.or(payload.comment),
                    ..payload
                }
            }
        });
    }
    Ok(quote! {
        ::json_schema_rs::JsonSchema {
            id: #id_expr,
            title: #title,
            description: #description,
            comment: #comment,
            #combinator_field: Some(::std::vec::Vec::<::json_schema_rs::Schema>::from([
                #(::json_schema_rs::Schema::from(
                    <#payload_types as ::json_schema_rs::ToJsonSchema>::json_schema_with(generator),
                )),*
            ])),
            ..::json_schema_rs::JsonSchema::default()
        }
    })
}
