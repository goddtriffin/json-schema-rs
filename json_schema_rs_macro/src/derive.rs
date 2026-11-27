//! Derive macro for `ToJsonSchema`: builds JSON Schema from struct type and attributes.

use crate::rename_rule::rename_rule_from_attrs;
use json_schema_rs::sanitizers::RenameRule;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::visit_mut::VisitMut;
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

/// Path of the library's run-time support for this derive (`FieldSpec`, the builders).
fn support() -> TokenStream2 {
    quote! { __json_schema_rs::reverse_code_gen::derive_support }
}

/// Builds an `Option<String>` expression from doc values through the library's non-generic
/// `doc_description` (each value trimmed, joined with `\n`, leading/trailing blank lines
/// removed; `None` when no doc text remains).
fn doc_description_expr(exprs: &[&Expr]) -> TokenStream2 {
    if exprs.is_empty() {
        return quote! { None };
    }
    let support: TokenStream2 = support();
    quote! { #support::doc_description(&[#(#exprs),*]) }
}

/// `ty` with every `Self` path replaced by `name`, so the type can be named inside a `static`
/// in the generated body fn (a nested item cannot use the outer impl's `Self`).
fn replace_self(ty: &Type, name: &Ident) -> Type {
    struct ReplaceSelf<'a>(&'a Ident);
    impl VisitMut for ReplaceSelf<'_> {
        fn visit_path_mut(&mut self, path: &mut syn::Path) {
            if let Some(first) = path.segments.first_mut()
                && first.ident == "Self"
            {
                first.ident = self.0.clone();
            }
            syn::visit_mut::visit_path_mut(self, path);
        }
    }
    let mut ty: Type = ty.clone();
    ReplaceSelf(name).visit_type_mut(&mut ty);
    ty
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
/// [`doc_description_expr`]). The attribute takes precedence; a blank one is no description.
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
            let support: TokenStream2 = support();
            return Ok(quote! { #support::doc_description(&[#s]) });
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
    /// A constant expression of the library's `NumberLiteral` for this value, which converts
    /// to a `serde_json::Number` at run time (`None` for a float that is not finite).
    fn to_literal_tokens(self) -> TokenStream2 {
        let support: TokenStream2 = support();
        match self {
            NumericLiteral::Unsigned(n) => {
                let lit = proc_macro2::Literal::u64_suffixed(n);
                quote! { #support::NumberLiteral::Unsigned(#lit) }
            }
            NumericLiteral::Signed(i64::MIN) => {
                quote! { #support::NumberLiteral::Signed(i64::MIN) }
            }
            NumericLiteral::Signed(n) => {
                let lit = proc_macro2::Literal::i64_suffixed(n.abs());
                if n < 0 {
                    quote! { #support::NumberLiteral::Signed(-#lit) }
                } else {
                    quote! { #support::NumberLiteral::Signed(#lit) }
                }
            }
            NumericLiteral::Float(f) => {
                let lit = proc_macro2::Literal::f64_suffixed(f.abs());
                if f.is_sign_negative() {
                    quote! { #support::NumberLiteral::Float(-#lit) }
                } else {
                    quote! { #support::NumberLiteral::Float(#lit) }
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

/// A constant `Option<NumberLiteral>` expression for the numeric attribute `key`, `None` when
/// absent.
fn field_number_literal(field: &Field, key: &str) -> SynResult<TokenStream2> {
    Ok(field_numeric_attr(field, key)?.map_or_else(
        || quote! { None },
        |literal: NumericLiteral| {
            let tokens: TokenStream2 = literal.to_literal_tokens();
            quote! { Some(#tokens) }
        },
    ))
}

/// An `Option<serde_json::Number>` expression for the numeric attribute `key`, `None` when
/// absent.
fn field_number_expr(field: &Field, key: &str) -> SynResult<TokenStream2> {
    let literal: TokenStream2 = field_number_literal(field, key)?;
    let support: TokenStream2 = support();
    Ok(quote! {
        ::core::option::Option::<#support::NumberLiteral>::and_then(
            #literal,
            #support::NumberLiteral::to_number,
        )
    })
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

/// Extracts `default = <literal>` from a field's `#[json_schema(...)]` attribute as a constant
/// expression of the library's `DefaultLiteral`: a string, a boolean, or an integer or float
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
            let support: TokenStream2 = support();
            if let Expr::Lit(expr_lit) = expr {
                match &expr_lit.lit {
                    Lit::Str(lit) => {
                        return Ok(Some(quote! { #support::DefaultLiteral::String(#lit) }));
                    }
                    Lit::Bool(lit) => {
                        return Ok(Some(quote! { #support::DefaultLiteral::Bool(#lit) }));
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
                .to_literal_tokens();
            return Ok(Some(quote! { #support::DefaultLiteral::Number(#number) }));
        }
    }
    Ok(None)
}

/// One argument of a `#[serde(...)]` attribute: its name and, for `name = "..."` (or the
/// `deserialize = "..."` of `name(...)`), the string.
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
            let name: String = meta.path.get_ident().map_or_default(ToString::to_string);
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
                // `rename(serialize = "a", deserialize = "b")`: the deserialize value names
                // what serde reads; other list forms (`bound(...)`) carry no value used here.
                let mut deserialize: Option<String> = None;
                let parsed: SynResult<()> = meta.parse_nested_meta(|inner| {
                    if inner.input.peek(Token![=]) {
                        let expr: Expr = inner.value()?.parse()?;
                        if inner.path.is_ident("deserialize")
                            && let Expr::Lit(syn::ExprLit {
                                lit: Lit::Str(lit_str),
                                ..
                            }) = expr
                        {
                            deserialize = Some(lit_str.value());
                        }
                    }
                    Ok(())
                });
                if parsed.is_err() {
                    deserialize = None;
                }
                value = deserialize;
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

/// Every `#[serde(alias = "...")]` in `attrs`, in order, as string literals.
fn serde_aliases(attrs: &[Attribute], span: proc_macro2::Span) -> SynResult<Vec<LitStr>> {
    Ok(serde_args(attrs)?
        .into_iter()
        .filter(|arg| arg.name == "alias")
        .filter_map(|arg| arg.value)
        .map(|alias: String| LitStr::new(&alias, span))
        .collect())
}

/// Whether `attrs` carry the bare serde flag `#[serde(flag)]` (e.g. `flatten`, `untagged`).
fn serde_flag(attrs: &[Attribute], flag: &str) -> SynResult<bool> {
    Ok(serde_args(attrs)?
        .iter()
        .any(|arg| arg.name == flag && arg.value.is_none()))
}

/// Whether `attrs` carry the serde argument `name` in any form (`default`, `default = "f"`).
fn serde_has(attrs: &[Attribute], name: &str) -> SynResult<bool> {
    Ok(serde_args(attrs)?.iter().any(|arg| arg.name == name))
}

/// Whether serde never reads `attrs`' field or variant (`skip`, `skip_deserializing`).
fn serde_skipped(attrs: &[Attribute]) -> SynResult<bool> {
    Ok(serde_has(attrs, "skip")? || serde_has(attrs, "skip_deserializing")?)
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

/// Returns the JSON property key for a field, as serde names it: its `rename`, else its name
/// (without `r#`) under the container's `rename_all` rule.
fn field_property_key(field: &Field, rule: Option<RenameRule>) -> SynResult<String> {
    if let Some(renamed) = serde_str(&field.attrs, "rename")? {
        return Ok(renamed);
    }
    let ident = field.ident.as_ref().ok_or_else(|| {
        Error::new_spanned(
            field,
            "ToJsonSchema derive only supports named struct fields",
        )
    })?;
    let name: String = ident.unraw().to_string();
    Ok(rule.map_or(name.clone(), |rule: RenameRule| rule.apply_to_field(&name)))
}

/// Returns true if the type is `Option<T>` (path is `Option` or `std::option::Option`).
fn is_option_type(ty: &Type) -> bool {
    last_segment_is(ty, "Option")
}

/// Whether `ty` is a path whose last segment is `name` (`Nullable`, `json_schema_rs::Nullable`).
fn last_segment_is(ty: &Type, name: &str) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|seg| seg.ident == name)
}

/// Returns the serde name of an enum variant: its `rename`, else its name under the
/// container's `rename_all` rule.
fn variant_external_name(variant: &Variant, rule: Option<RenameRule>) -> SynResult<String> {
    if let Some(renamed) = serde_str(&variant.attrs, "rename")? {
        return Ok(renamed);
    }
    let name: String = variant.ident.unraw().to_string();
    Ok(rule.map_or(name.clone(), |rule: RenameRule| {
        rule.apply_to_variant(&name)
    }))
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

/// The `#[derive(ToJsonSchema)]` expansion, inside `const _: () = { ... };` with the library
/// imported as `__json_schema_rs` from its `#[json_schema(crate = "...")]` path (default
/// `::json_schema_rs`), as serde does for its own `crate` attribute: every path in the
/// expansion goes through that one import.
pub fn expand_to_json_schema(input: DeriveInput) -> SynResult<TokenStream2> {
    let krate: syn::Path = crate_path(&input.attrs)?;
    let body: TokenStream2 = expand_body(input)?;
    Ok(quote! {
        const _: () = {
            use #krate as __json_schema_rs;
            #body
        };
    })
}

/// The path of the library for a derive on an item with `attrs`: its
/// `#[json_schema(crate = "path")]`, else `::json_schema_rs`.
pub(crate) fn crate_path(attrs: &[Attribute]) -> SynResult<syn::Path> {
    for attr in attrs {
        if !attr.path().is_ident("json_schema") {
            continue;
        }
        let mut found: Option<syn::Path> = None;
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("crate") {
                let value: LitStr = meta.value()?.parse()?;
                found = Some(value.parse().map_err(|_| {
                    Error::new(
                        value.span(),
                        format!("`{}` is not a Rust path", value.value()),
                    )
                })?);
            } else if meta.input.peek(Token![=]) {
                let _: Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: TokenStream2 = content.parse()?;
            }
            Ok(())
        })?;
        if let Some(path) = found {
            return Ok(path);
        }
    }
    Ok(syn::parse_quote!(::json_schema_rs))
}

fn expand_body(input: DeriveInput) -> SynResult<TokenStream2> {
    let name: Ident = input.ident;
    let attrs: &[Attribute] = &input.attrs;

    if let Some(ty) = forwarded_type(&name, attrs)? {
        return Ok(forwarding_impl(&name, &ty));
    }
    if let Some(field) = transparent_field(&name, attrs, &input.data)? {
        return expand_newtype_to_json_schema(&name, attrs, &field);
    }

    if let syn::Data::Enum(data_enum) = &input.data {
        return expand_enum_to_json_schema(&name, attrs, data_enum);
    }

    let syn::Data::Struct(data_struct) = &input.data else {
        return Err(Error::new_spanned(
            name,
            "ToJsonSchema derive only supports structs and enums",
        ));
    };
    let container: ContainerExprs = container_exprs(attrs)?;
    let support: TokenStream2 = support();
    let schema_expr: TokenStream2 = match &data_struct.fields {
        Fields::Named(named) => {
            let spec: TokenStream2 = object_spec(
                &name,
                named.named.iter(),
                rename_rule_from_attrs(attrs, "rename_all")?,
                serde_has(attrs, "default")?,
            )?;
            quote! {
                static SPEC: #support::ObjectSpec = #spec;
                #support::object_schema(generator, &SPEC)
            }
        }
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
            return expand_newtype_to_json_schema(&name, attrs, &unnamed.unnamed[0]);
        }
        Fields::Unnamed(unnamed) => {
            let items: Vec<Type> = unnamed
                .unnamed
                .iter()
                .map(|field: &Field| replace_self(&field.ty, &name))
                .collect();
            quote! {
                static ITEMS: &[#support::SchemaFn] = &[
                    #(<#items as __json_schema_rs::ToJsonSchema>::json_schema_with),*
                ];
                #support::tuple_schema(generator, ITEMS)
            }
        }
        // A unit struct deserializes from `null`.
        Fields::Unit => quote! {
            let _ = generator;
            __json_schema_rs::JsonSchema {
                type_: Some(__json_schema_rs::TypeSet::from(__json_schema_rs::JsonType::Null)),
                ..__json_schema_rs::JsonSchema::default()
            }
        },
    };
    let body: TokenStream2 = with_container(&container, &schema_expr);
    Ok(named_type_impl(&name, container.id.as_deref(), &body, None))
}

/// `schema_expr` (statements ending in a `JsonSchema` expression) with the container's `$id`,
/// `title`, `description` (or doc comments) and `comment` on top: each one set replaces what
/// the schema has.
fn with_container(container: &ContainerExprs, schema_expr: &TokenStream2) -> TokenStream2 {
    let ContainerExprs {
        title,
        description,
        comment,
        id_expr,
        ..
    } = container;
    quote! {
        let schema: __json_schema_rs::JsonSchema = { #schema_expr };
        __json_schema_rs::JsonSchema {
            id: #id_expr.or(schema.id),
            title: #title.or(schema.title),
            description: #description.or(schema.description),
            comment: #comment.or(schema.comment),
            ..schema
        }
    }
}

/// The type serde deserializes the container through, if it is not the container's own
/// shape: the type of `#[serde(from = "...")]` / `#[serde(try_from = "...")]`.
fn forwarded_type(name: &Ident, attrs: &[Attribute]) -> SynResult<Option<Type>> {
    for key in ["from", "try_from"] {
        if let Some(path) = serde_str(attrs, key)? {
            let ty: Type = syn::parse_str(&path).map_err(|error: Error| {
                Error::new_spanned(name, format!("serde({key} = \"{path}\"): {error}"))
            })?;
            return Ok(Some(ty));
        }
    }
    Ok(None)
}

/// The one read field of a `#[serde(transparent)]` struct: serde reads the struct as that
/// field, so it is a newtype of the field's type (a named type carrying the container's
/// constraints and annotations).
fn transparent_field(
    name: &Ident,
    attrs: &[Attribute],
    data: &syn::Data,
) -> SynResult<Option<Field>> {
    if !serde_flag(attrs, "transparent")? {
        return Ok(None);
    }
    let syn::Data::Struct(data_struct) = data else {
        return Err(Error::new_spanned(
            name,
            "#[serde(transparent)] applies to structs only",
        ));
    };
    for field in &data_struct.fields {
        if !serde_skipped(&field.attrs)? {
            return Ok(Some(Field {
                ty: replace_self(&field.ty, name),
                ..field.clone()
            }));
        }
    }
    Err(Error::new_spanned(
        name,
        "#[serde(transparent)] needs one field that is not skipped",
    ))
}

/// The impl of a type serde reads as another type `ty`: every method is `ty`'s, so the type
/// has exactly `ty`'s schema and shares its `$defs` entry.
fn forwarding_impl(name: &Ident, ty: &Type) -> TokenStream2 {
    quote! {
        #[allow(deprecated)]
        impl __json_schema_rs::ToJsonSchema for #name {
            fn json_schema() -> __json_schema_rs::ReverseCodeGenResult {
                <#ty as __json_schema_rs::ToJsonSchema>::json_schema()
            }

            fn json_schema_root(
                generator: __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::ReverseCodeGenResult {
                <#ty as __json_schema_rs::ToJsonSchema>::json_schema_root(generator)
            }

            fn json_schema_with(
                generator: &mut __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::JsonSchema {
                <#ty as __json_schema_rs::ToJsonSchema>::json_schema_with(generator)
            }

            fn json_schema_flattened(
                generator: &mut __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::JsonSchema {
                <#ty as __json_schema_rs::ToJsonSchema>::json_schema_flattened(generator)
            }

            fn schema_id() -> &'static str {
                <#ty as __json_schema_rs::ToJsonSchema>::schema_id()
            }
        }
    }
}

/// The constant `ObjectSpec` expression of the named fields `fields` of type `name` (a struct
/// or a struct variant), keys named by `rule`; `container_default` is the container's
/// `#[serde(default)]`, which makes every field optional.
///
/// Fields serde never reads (`skip`, `skip_deserializing`) are left out; `#[serde(flatten)]`
/// fields become flatten entries, merged into the object by the library.
fn object_spec<'a>(
    name: &Ident,
    fields: impl Iterator<Item = &'a Field>,
    rule: Option<RenameRule>,
    container_default: bool,
) -> SynResult<TokenStream2> {
    let support: TokenStream2 = support();
    let mut field_specs: Vec<TokenStream2> = Vec::new();
    let mut flattens: Vec<TokenStream2> = Vec::new();
    for field in fields {
        if serde_skipped(&field.attrs)? {
            continue;
        }
        if serde_flag(&field.attrs, "flatten")? {
            let optional: bool = is_option_type(&field.ty);
            let ty: Type = if let Some(with) = field_str_attr(field, "with")? {
                parse_with(field, &with)?
            } else if optional {
                optional_inner_type(&field.ty).clone()
            } else {
                field.ty.clone()
            };
            let ty: Type = replace_self(&ty, name);
            flattens.push(quote! {
                #support::FlattenSpec {
                    schema: <#ty as __json_schema_rs::ToJsonSchema>::json_schema_flattened,
                    optional: #optional,
                }
            });
            continue;
        }
        field_specs.push(field_spec(name, field, rule, container_default)?);
    }
    Ok(quote! {
        #support::ObjectSpec {
            fields: &[#(#field_specs),*],
            flattens: &[#(#flattens),*],
            closed: true,
        }
    })
}

/// The type named by a field's `#[json_schema(with = "Type")]`.
fn parse_with(field: &Field, with: &str) -> SynResult<Type> {
    syn::parse_str(with).map_err(|error: Error| {
        Error::new_spanned(field, format!("json_schema(with = \"{with}\"): {error}"))
    })
}

/// The type whose schema a variant's field has: its `#[json_schema(with = "Type")]`, else its
/// own type.
fn schema_type(field: &Field) -> SynResult<Type> {
    match field_str_attr(field, "with")? {
        Some(with) => parse_with(field, &with),
        None => Ok(field.ty.clone()),
    }
}

/// The constant `FieldSpec` expression of one (non-flattened) named field of type `name`.
///
/// A field is optional when serde fills it in when absent: an `Option` (one `Option`
/// stripped from its schema) read by serde's own deserializer, a `Nullable`, or a field with
/// `#[serde(default)]` (its own or the container's, `container_default`).
/// `#[json_schema(required)]` makes any field required, an `Option` keeping its nullable
/// schema. `#[json_schema(with = "Type")]` gives the field `Type`'s schema, and
/// `#[json_schema(schema_with = "path")]` the schema `path(generator)` returns.
fn field_spec(
    name: &Ident,
    field: &Field,
    rule: Option<RenameRule>,
    container_default: bool,
) -> SynResult<TokenStream2> {
    let support: TokenStream2 = support();
    let ty: &Type = &field.ty;
    let key: String = field_property_key(field, rule)?;
    let span = field
        .ident
        .as_ref()
        .map_or_else(proc_macro2::Span::call_site, syn::spanned::Spanned::span);
    let key_lit: LitStr = LitStr::new(&key, span);
    let is_required: bool = json_schema_flag(&field.attrs, "required")?;
    // serde fills in an absent `Option` only through its own deserializer: a field with
    // `deserialize_with` or `with` is required unless it has a default.
    let custom_deserializer: bool =
        serde_has(&field.attrs, "deserialize_with")? || serde_has(&field.attrs, "with")?;
    let has_default: bool = container_default || serde_has(&field.attrs, "default")?;
    let strip_option: bool = is_option_type(ty) && !is_required;
    // A `Nullable<T>` field is optional and its schema allows `null` (the type's own schema).
    let is_nullable: bool = last_segment_is(ty, "Nullable") && !is_required;
    let absent_is_none: bool = strip_option && !custom_deserializer;
    let required: bool = is_required || (!absent_is_none && !is_nullable && !has_default);
    let schema_fn: TokenStream2 = if let Some(path) = field_str_attr(field, "schema_with")? {
        let path: syn::ExprPath = syn::parse_str(&path).map_err(|error: Error| {
            Error::new_spanned(
                field,
                format!("json_schema(schema_with = \"{path}\"): {error}"),
            )
        })?;
        quote! { #path }
    } else {
        let schema_ty: Type = if let Some(with) = field_str_attr(field, "with")? {
            parse_with(field, &with)?
        } else if strip_option {
            optional_inner_type(ty).clone()
        } else {
            ty.clone()
        };
        let schema_ty: Type = replace_self(&schema_ty, name);
        quote! { <#schema_ty as __json_schema_rs::ToJsonSchema>::json_schema_with }
    };
    // `#[json_schema(description = "...")]` replaces the doc comments (a blank one is no
    // description).
    let description: Option<String> = field_str_attr(field, "description")?;
    let description_lit: Option<LitStr> = description.map(|d| LitStr::new(&d, span));
    let doc: Vec<TokenStream2> = match &description_lit {
        Some(lit) => vec![quote! { #lit }],
        None => doc_exprs(&field.attrs)
            .into_iter()
            .map(|expr| quote! { #expr })
            .collect(),
    };
    let minimum: TokenStream2 = field_number_literal(field, "minimum")?;
    let maximum: TokenStream2 = field_number_literal(field, "maximum")?;
    let exclusive_minimum: TokenStream2 = field_number_literal(field, "exclusive_minimum")?;
    let exclusive_maximum: TokenStream2 = field_number_literal(field, "exclusive_maximum")?;
    let multiple_of: TokenStream2 = field_number_literal(field, "multiple_of")?;
    let u64_expr = |value: Option<u64>| -> TokenStream2 {
        value.map_or_else(
            || quote! { None },
            |n| {
                let lit = proc_macro2::Literal::u64_unsuffixed(n);
                quote! { Some(#lit) }
            },
        )
    };
    let min_items: TokenStream2 = u64_expr(field_min_items(field)?);
    let max_items: TokenStream2 = u64_expr(field_max_items(field)?);
    let min_length: TokenStream2 = u64_expr(field_min_length(field)?);
    let max_length: TokenStream2 = u64_expr(field_max_length(field)?);
    let unique_items: bool = json_schema_flag(&field.attrs, "unique_items")?;
    let pattern: TokenStream2 = field_pattern(field)?.map_or_else(
        || quote! { None },
        |p| {
            let lit = LitStr::new(&p, span);
            quote! { Some(#lit) }
        },
    );
    let deprecated: bool = field_deprecated(field)? == Some(true);
    let aliases: Vec<LitStr> = serde_aliases(&field.attrs, span)?;
    let default: TokenStream2 =
        field_default(field)?.map_or_else(|| quote! { None }, |d| quote! { Some(#d) });
    Ok(quote! {
        #support::FieldSpec {
            key: #key_lit,
            aliases: &[#(#aliases),*],
            schema: #schema_fn,
            required: #required,
            doc: &[#(#doc),*],
            minimum: #minimum,
            maximum: #maximum,
            exclusive_minimum: #exclusive_minimum,
            exclusive_maximum: #exclusive_maximum,
            multiple_of: #multiple_of,
            min_items: #min_items,
            max_items: #max_items,
            unique_items: #unique_items,
            min_length: #min_length,
            max_length: #max_length,
            pattern: #pattern,
            deprecated: #deprecated,
            default: #default,
        }
    })
}

/// Expand `ToJsonSchema` for a newtype (`struct Name(T);`): `T`'s schema with the constraint
/// keywords of the container's `#[json_schema(...)]` (`min_length`, `max_length`, `pattern`,
/// `minimum`, `maximum`, `exclusive_minimum`, `exclusive_maximum`, `multiple_of`) and its
/// `title`, `description` (or doc comments), `comment` and `id`. The newtype is a named type,
/// in `$defs` like a struct.
fn expand_newtype_to_json_schema(
    name: &Ident,
    attrs: &[Attribute],
    field: &Field,
) -> SynResult<TokenStream2> {
    let ty: &Type = &field.ty;
    // The constraint attributes are the container's: read them as a field's.
    let container_field: Field = Field {
        attrs: attrs.to_vec(),
        ..field.clone()
    };
    let field: &Field = &container_field;
    let ContainerExprs {
        title: title_expr,
        description: description_expr,
        comment: comment_expr,
        id_expr,
        id,
    } = container_exprs(attrs)?;
    let min_expr: TokenStream2 = field_number_expr(field, "minimum")?;
    let max_expr: TokenStream2 = field_number_expr(field, "maximum")?;
    let exclusive_min_expr: TokenStream2 = field_number_expr(field, "exclusive_minimum")?;
    let exclusive_max_expr: TokenStream2 = field_number_expr(field, "exclusive_maximum")?;
    let multiple_of_expr: TokenStream2 = field_number_expr(field, "multiple_of")?;
    let u64_expr = |value: Option<u64>| -> TokenStream2 {
        value.map_or_else(
            || quote! { None },
            |n| {
                let lit = proc_macro2::Literal::u64_unsuffixed(n);
                quote! { Some(#lit) }
            },
        )
    };
    let min_length_expr: TokenStream2 = u64_expr(field_min_length(field)?);
    let max_length_expr: TokenStream2 = u64_expr(field_max_length(field)?);
    let pattern_expr: TokenStream2 = field_pattern(field)?.map_or_else(
        || quote! { None },
        |p| {
            let lit = LitStr::new(&p, name.span());
            quote! { Some(#lit.to_string()) }
        },
    );
    let body: TokenStream2 = quote! {
        let mut schema: __json_schema_rs::JsonSchema =
            <#ty as __json_schema_rs::ToJsonSchema>::json_schema_with(generator);
        schema.id = #id_expr.or(schema.id);
        schema.title = #title_expr.or(schema.title);
        schema.description = #description_expr.or(schema.description);
        schema.comment = #comment_expr.or(schema.comment);
        schema.minimum = #min_expr.or(schema.minimum);
        schema.maximum = #max_expr.or(schema.maximum);
        schema.exclusive_minimum = #exclusive_min_expr.or(schema.exclusive_minimum);
        schema.exclusive_maximum = #exclusive_max_expr.or(schema.exclusive_maximum);
        schema.multiple_of = #multiple_of_expr.or(schema.multiple_of);
        schema.min_length = #min_length_expr.or(schema.min_length);
        schema.max_length = #max_length_expr.or(schema.max_length);
        schema.pattern = #pattern_expr.or(schema.pattern);
        schema
    };
    Ok(named_type_impl(name, id.as_deref(), &body, None))
}

/// Emits the `ToJsonSchema` impl for a named type (derived struct or enum) whose schema body
/// is `body`, statements ending in a `JsonSchema` that may use the generator `generator`.
///
/// The body lives in a hidden inherent fn so it is written once: `json_schema_with` registers
/// it in the generator's `$defs` and returns a `$ref`; `json_schema` builds it as the root
/// through `json_schema_root` (the generator adds `$schema`); `json_schema_flattened` inlines it
/// (without `$id`) for `#[serde(flatten)]`, or inlines `flattened_body` when the type reads differently
/// there (an externally tagged enum's unit variants). The type's identity in the generator is
/// its full path (`module_path!()` expands where the type is defined), so same-name types from
/// different modules get separate `$defs` entries.
///
/// A type with `id` is its own schema resource, built through the generator's resource
/// methods so it carries the `$defs` its refs need and refers to itself as `#`.
fn named_type_impl(
    name: &Ident,
    id: Option<&str>,
    body: &TokenStream2,
    flattened_body: Option<&TokenStream2>,
) -> TokenStream2 {
    let name_lit: LitStr = LitStr::new(&name.to_string(), name.span());
    let (root_call, subschema_call): (TokenStream2, TokenStream2) = if let Some(id) = id {
        let id_lit: LitStr = LitStr::new(id, name.span());
        (
            quote! {
                .into_resource_root_schema(
                    <Self as __json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    #id_lit,
                    Self::__json_schema_rs_body,
                )
            },
            quote! {
                generator.resource_subschema(
                    <Self as __json_schema_rs::ToJsonSchema>::schema_id(),
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
                    <Self as __json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    Self::__json_schema_rs_body,
                )
            },
            quote! {
                generator.named_subschema(
                    <Self as __json_schema_rs::ToJsonSchema>::schema_id(),
                    #name_lit,
                    Self::__json_schema_rs_body,
                )
            },
        )
    };
    let (flattened_fn, flattened_path): (TokenStream2, TokenStream2) = match flattened_body {
        Some(flattened_body) => (
            quote! {
                #[doc(hidden)]
                fn __json_schema_rs_flattened_body(
                    generator: &mut __json_schema_rs::SchemaGenerator,
                ) -> __json_schema_rs::JsonSchema {
                    #flattened_body
                }
            },
            quote! { Self::__json_schema_rs_flattened_body },
        ),
        None => (quote! {}, quote! { Self::__json_schema_rs_body }),
    };
    quote! {
        // A deprecated type is still described, as serde's derive reads and writes it.
        #[allow(deprecated)]
        impl #name {
            #[doc(hidden)]
            fn __json_schema_rs_body(
                generator: &mut __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::JsonSchema {
                #body
            }

            #flattened_fn
        }

        #[allow(deprecated)]
        impl __json_schema_rs::ToJsonSchema for #name {
            fn json_schema() -> __json_schema_rs::ReverseCodeGenResult {
                <Self as __json_schema_rs::ToJsonSchema>::json_schema_root(
                    __json_schema_rs::SchemaGenerator::default(),
                )
            }

            fn json_schema_root(
                generator: __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::ReverseCodeGenResult {
                generator #root_call
            }

            fn json_schema_with(
                generator: &mut __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::JsonSchema {
                #subschema_call
            }

            fn json_schema_flattened(
                generator: &mut __json_schema_rs::SchemaGenerator,
            ) -> __json_schema_rs::JsonSchema {
                let schema: __json_schema_rs::JsonSchema = generator.inline_named(
                    <Self as __json_schema_rs::ToJsonSchema>::schema_id(),
                    #flattened_path,
                    <Self as __json_schema_rs::ToJsonSchema>::json_schema_with,
                );
                __json_schema_rs::JsonSchema { id: None, ..schema }
            }

            fn schema_id() -> &'static str {
                ::core::concat!(::core::module_path!(), "::", #name_lit)
            }
        }
    }
}

/// Expand `ToJsonSchema` for an enum, as serde deserializes it.
///
/// An externally tagged enum whose variants serde reads are all unit variants is a value enum
/// (see [`value_enum_body`]). Any other enum is described variant by variant by the library's
/// `enum_schema`, under serde's representation: external, internal (`tag`), adjacent (`tag`
/// and `content`) or untagged, with `#[serde(untagged)]` variants after the tagged ones.
fn expand_enum_to_json_schema(
    name: &Ident,
    attrs: &[Attribute],
    data_enum: &syn::DataEnum,
) -> SynResult<TokenStream2> {
    let support: TokenStream2 = support();
    let container: ContainerExprs = container_exprs(attrs)?;
    let rule: Option<RenameRule> = rename_rule_from_attrs(attrs, "rename_all")?;
    let fields_rule: Option<RenameRule> = rename_rule_from_attrs(attrs, "rename_all_fields")?;
    let tag: Option<String> = serde_str(attrs, "tag")?;
    let content: Option<String> = serde_str(attrs, "content")?;
    let untagged: bool = serde_flag(attrs, "untagged")?;
    let one_of: bool = json_schema_flag(attrs, "one_of")?;
    let flatten_single_entry: bool = json_schema_flag(attrs, "flatten_single_entry")?;
    let tagging: TokenStream2 = match (&tag, &content, untagged) {
        (_, _, true) => quote! {
            #support::Tagging::Untagged {
                one_of: #one_of,
                flatten_single_entry: #flatten_single_entry,
            }
        },
        (Some(tag), Some(content), false) => {
            quote! { #support::Tagging::Adjacent { tag: #tag, content: #content } }
        }
        (Some(tag), None, false) => quote! { #support::Tagging::Internal { tag: #tag } },
        (None, _, false) => quote! { #support::Tagging::External },
    };
    let internal: bool = tag.is_some() && content.is_none() && !untagged;
    let variants: Vec<&Variant> = data_enum
        .variants
        .iter()
        .map(|variant: &Variant| Ok((variant, serde_skipped(&variant.attrs)?)))
        .collect::<SynResult<Vec<(&Variant, bool)>>>()?
        .into_iter()
        .filter_map(|(variant, skipped)| (!skipped).then_some(variant))
        .collect();

    let mut value_enum: bool = tag.is_none() && !untagged;
    for variant in &variants {
        value_enum &= matches!(variant.fields, Fields::Unit)
            && !serde_flag(&variant.attrs, "untagged")?
            && !serde_flag(&variant.attrs, "other")?;
    }
    if value_enum {
        let body: TokenStream2 = value_enum_body(&container, &variants, rule)?;
        // Under `#[serde(flatten)]` serde reads a unit variant as its key with a `null` value.
        let flattened: TokenStream2 = enum_spec_body(
            name,
            &variants,
            &quote! { #support::Tagging::External },
            rule,
            fields_rule,
            true,
        )?;
        return Ok(named_type_impl(
            name,
            container.id.as_deref(),
            &body,
            Some(&with_container(&container, &flattened)),
        ));
    }
    for variant in &variants {
        if variant_value_tokens(variant)?.is_some() {
            return Err(Error::new_spanned(
                variant,
                "json_schema(value = ...) applies to the variants of an enum of unit variants only",
            ));
        }
        if internal
            && !serde_flag(&variant.attrs, "untagged")?
            && matches!(&variant.fields, Fields::Unnamed(unnamed) if unnamed.unnamed.len() > 1)
        {
            return Err(Error::new_spanned(
                variant,
                "serde cannot deserialize a tuple variant of an internally tagged enum",
            ));
        }
    }
    let body: TokenStream2 = enum_spec_body(name, &variants, &tagging, rule, fields_rule, false)?;
    let flattened: Option<TokenStream2> = if tag.is_none() && !untagged {
        Some(with_container(
            &container,
            &enum_spec_body(name, &variants, &tagging, rule, fields_rule, true)?,
        ))
    } else {
        None
    };
    Ok(named_type_impl(
        name,
        container.id.as_deref(),
        &with_container(&container, &body),
        flattened.as_ref(),
    ))
}

/// Statements building the enum's schema through the library's `enum_schema` from a static
/// `EnumSpec`; `flattened` selects the shape serde reads under `#[serde(flatten)]`.
fn enum_spec_body(
    name: &Ident,
    variants: &[&Variant],
    tagging: &TokenStream2,
    rule: Option<RenameRule>,
    fields_rule: Option<RenameRule>,
    flattened: bool,
) -> SynResult<TokenStream2> {
    let support: TokenStream2 = support();
    let mut specs: Vec<TokenStream2> = Vec::new();
    for variant in variants {
        let variant_name: String = variant_external_name(variant, rule)?;
        let name_lit: LitStr = LitStr::new(&variant_name, variant.ident.span());
        let doc: Vec<&Expr> = doc_exprs(&variant.attrs);
        let shape: TokenStream2 = match &variant.fields {
            Fields::Unit => quote! { #support::VariantShape::Unit },
            Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
                let ty: Type = replace_self(&schema_type(&unnamed.unnamed[0])?, name);
                quote! {
                    #support::VariantShape::Newtype {
                        schema: <#ty as __json_schema_rs::ToJsonSchema>::json_schema_with,
                        flattened: <#ty as __json_schema_rs::ToJsonSchema>::json_schema_flattened,
                    }
                }
            }
            Fields::Unnamed(unnamed) => {
                let items: Vec<Type> = unnamed
                    .unnamed
                    .iter()
                    .map(|field: &Field| Ok(replace_self(&schema_type(field)?, name)))
                    .collect::<SynResult<Vec<Type>>>()?;
                quote! {
                    #support::VariantShape::Tuple(&[
                        #(<#items as __json_schema_rs::ToJsonSchema>::json_schema_with),*
                    ])
                }
            }
            Fields::Named(named) => {
                let variant_rule: Option<RenameRule> =
                    rename_rule_from_attrs(&variant.attrs, "rename_all")?.or(fields_rule);
                let spec: TokenStream2 = object_spec(
                    name,
                    named.named.iter(),
                    variant_rule,
                    serde_has(&variant.attrs, "default")?,
                )?;
                quote! { #support::VariantShape::Struct(#spec) }
            }
        };
        let untagged: bool = serde_flag(&variant.attrs, "untagged")?;
        let other: bool = serde_flag(&variant.attrs, "other")?;
        let aliases: Vec<LitStr> = serde_aliases(&variant.attrs, variant.ident.span())?;
        specs.push(quote! {
            #support::VariantSpec {
                name: #name_lit,
                aliases: &[#(#aliases),*],
                doc: &[#(#doc),*],
                shape: #shape,
                untagged: #untagged,
                other: #other,
            }
        });
    }
    Ok(quote! {
        static SPEC: #support::EnumSpec = #support::EnumSpec {
            tagging: #tagging,
            variants: &[#(#specs),*],
            closed: true,
            flattened: #flattened,
        };
        #support::enum_schema(generator, &SPEC)
    })
}

/// The schema of an enum of unit variants: its variants' values as `enum` (`const` for one
/// variant).
///
/// A variant's value is its `#[json_schema(value = <JSON literal>)]`, else its serde name as a
/// string. When every value is a serde name the schema also has `"type": "string"`; when any
/// variant has `value`, the values may be of any JSON type and no `type` is emitted. An enum
/// with no variants is `{"enum": []}`, which no instance matches.
///
/// The values are static data (a serde name is a string, a `value` literal a fn that builds
/// it), turned into `enum` / `const` by one library call whatever the number of variants.
fn value_enum_body(
    container: &ContainerExprs,
    variants: &[&Variant],
    rule: Option<RenameRule>,
) -> SynResult<TokenStream2> {
    let support: TokenStream2 = support();
    let mut values: Vec<TokenStream2> = Vec::new();
    let mut any_explicit_value: bool = false;
    for variant in variants {
        if let Some(tokens) = variant_value_tokens(variant)? {
            any_explicit_value = true;
            values.push(quote! { #support::EnumValue::Json(|| ::serde_json::json!(#tokens)) });
        } else {
            let external: String = variant_external_name(variant, rule)?;
            let lit: LitStr = LitStr::new(&external, variant.ident.span());
            values.push(quote! { #support::EnumValue::Name(#lit) });
            for alias in serde_aliases(&variant.attrs, variant.ident.span())? {
                values.push(quote! { #support::EnumValue::Name(#alias) });
            }
        }
    }

    let type_expr: TokenStream2 = if any_explicit_value || values.is_empty() {
        quote! { None }
    } else {
        quote! { Some(__json_schema_rs::TypeSet::from(__json_schema_rs::JsonType::String)) }
    };

    let ContainerExprs {
        title,
        description,
        comment,
        id_expr,
        ..
    } = container;
    Ok(quote! {
        let _ = generator;
        static VALUES: &[#support::EnumValue] = &[#(#values),*];
        let (enum_values, const_value): (
            ::core::option::Option<::std::vec::Vec<::serde_json::Value>>,
            ::core::option::Option<::serde_json::Value>,
        ) = #support::value_enum_keywords(VALUES);
        __json_schema_rs::JsonSchema {
            id: #id_expr,
            type_: #type_expr,
            title: #title,
            description: #description,
            comment: #comment,
            enum_values,
            const_value,
            ..__json_schema_rs::JsonSchema::default()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::expand_to_json_schema;
    use syn::DeriveInput;

    /// The error message `expand_to_json_schema` gives for `source`, or `None` when it expands.
    fn expansion_error(source: &str) -> Option<String> {
        let input: DeriveInput = syn::parse_str(source).expect("derive input");
        expand_to_json_schema(input)
            .err()
            .map(|error: syn::Error| error.to_string())
    }

    #[test]
    fn crate_that_is_not_a_path_is_an_error() {
        let expected: Option<String> = Some("`not a path` is not a Rust path".to_string());
        let actual: Option<String> =
            expansion_error(r#"#[json_schema(crate = "not a path")] struct S { a: i32 }"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_beside_other_container_keys_expands() {
        let expected: Option<String> = None;
        let actual: Option<String> = expansion_error(
            r#"#[json_schema(title = "T", crate = "crate::facade::jsr", id = "urn:t")] struct S { a: i32 }"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn internally_tagged_tuple_variant_is_an_error() {
        let expected: Option<String> = Some(
            "serde cannot deserialize a tuple variant of an internally tagged enum".to_string(),
        );
        let actual: Option<String> =
            expansion_error(r#"#[serde(tag = "t")] enum E { A(i32, i32) }"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn internally_tagged_untagged_tuple_variant_expands() {
        let expected: Option<String> = None;
        let actual: Option<String> = expansion_error(
            r#"#[serde(tag = "t")] enum E { A { x: i32 }, #[serde(untagged)] B(i32, i32) }"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_on_a_variant_of_a_non_unit_enum_is_an_error() {
        let expected: Option<String> = Some(
            "json_schema(value = ...) applies to the variants of an enum of unit variants only"
                .to_string(),
        );
        let actual: Option<String> =
            expansion_error("enum E { #[json_schema(value = 1)] A, B(i32) }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn transparent_without_a_read_field_is_an_error() {
        let expected: Option<String> =
            Some("#[serde(transparent)] needs one field that is not skipped".to_string());
        let actual: Option<String> =
            expansion_error("#[serde(transparent)] struct S { #[serde(skip)] a: i32 }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn transparent_enum_is_an_error() {
        let expected: Option<String> =
            Some("#[serde(transparent)] applies to structs only".to_string());
        let actual: Option<String> = expansion_error("#[serde(transparent)] enum E { A }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unparsable_from_type_is_an_error() {
        let expected: bool = true;
        let actual: bool = expansion_error(r#"#[serde(from = "not a type")] struct S;"#)
            .is_some_and(|message: String| message.starts_with("serde(from = \"not a type\"): "));
        assert_eq!(expected, actual);
    }
}
