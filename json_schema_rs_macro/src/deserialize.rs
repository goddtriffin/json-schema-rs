//! Derive macro for `serde::Deserialize` on the structs codegen emits.
//!
//! The expansion is a map visitor in which every field is read by one call to the shared
//! generic `json_schema_rs::generated_serde::__private::read_field`, so its code grows
//! linearly with the struct's fields. serde's derive writes each field's read into one
//! `visit_map`, which LLVM optimizes in superlinear time: an optimized build of a struct with
//! hundreds of fields takes minutes with serde's derive and seconds with this one.
//!
//! It accepts the serde attributes codegen emits and rejects every other one at its span:
//! container `deny_unknown_fields`; field `rename = "..."`, `default`, `deserialize_with =
//! "path"`, `flatten` (the map of additional properties; with `deserialize_with`, each of its
//! values reads through the path) and `skip_serializing_if` (serialization only, ignored).

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Error, ExprPath, Fields, Ident, LitStr, Result as SynResult, Type};

/// One named field and what its serde attributes say.
struct FieldSpec {
    ident: Ident,
    ty: Type,
    /// The object key (`rename`, else the field's name).
    key: String,
    /// `default`: an absent key is `Default::default()`.
    default: bool,
    /// `deserialize_with`: the function that reads the value (each value, on the flatten field).
    with: Option<ExprPath>,
    /// `flatten`: the map of additional properties.
    flatten: bool,
    /// Where an error about the field points.
    span: Span,
}

/// Add `error` to `errors`, the errors found so far (each becomes its own compile error).
fn push_error(errors: &mut Option<Error>, error: Error) {
    match errors {
        Some(all) => all.combine(error),
        None => *errors = Some(error),
    }
}

/// Whether the container says `deny_unknown_fields`; every other serde attribute is an error.
fn container_deny_unknown(input: &DeriveInput, errors: &mut Option<Error>) -> bool {
    let mut deny: bool = false;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let parsed: SynResult<()> = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("deny_unknown_fields") {
                deny = true;
                Ok(())
            } else {
                Err(meta.error("unsupported serde container attribute"))
            }
        });
        if let Err(error) = parsed {
            push_error(errors, error);
        }
    }
    deny
}

/// The field's spec from its serde attributes; an unsupported one is an error.
fn field_spec(field: &syn::Field, errors: &mut Option<Error>) -> FieldSpec {
    let ident: Ident = field.ident.clone().expect("a named field");
    let mut spec: FieldSpec = FieldSpec {
        key: ident.unraw().to_string(),
        ident,
        ty: field.ty.clone(),
        default: false,
        with: None,
        flatten: false,
        span: field.span(),
    };
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let parsed: SynResult<()> = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let lit: LitStr = meta.value()?.parse()?;
                spec.key = lit.value();
            } else if meta.path.is_ident("default") {
                if meta.input.peek(syn::Token![=]) {
                    return Err(meta.error("`default = \"...\"` is unsupported; use `default`"));
                }
                spec.default = true;
            } else if meta.path.is_ident("deserialize_with") {
                let lit: LitStr = meta.value()?.parse()?;
                spec.with = Some(lit.parse()?);
            } else if meta.path.is_ident("flatten") {
                spec.flatten = true;
            } else if meta.path.is_ident("skip_serializing_if") {
                // Serialization only.
                let _: LitStr = meta.value()?.parse()?;
            } else {
                return Err(meta.error("unsupported serde field attribute"));
            }
            Ok(())
        });
        if let Err(error) = parsed {
            push_error(errors, error);
        }
    }
    spec
}

/// The `#[derive(Deserialize)]` expansion for a struct with named fields.
pub fn expand_deserialize(input: &DeriveInput) -> SynResult<TokenStream2> {
    let name: &Ident = &input.ident;
    let mut errors: Option<Error> = None;
    if !input.generics.params.is_empty() || input.generics.where_clause.is_some() {
        push_error(
            &mut errors,
            Error::new(input.generics.span(), "generic structs are unsupported"),
        );
    }
    let named = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => Some(named),
            Fields::Unnamed(_) | Fields::Unit => None,
        },
        Data::Enum(_) | Data::Union(_) => None,
    };
    let Some(named) = named else {
        push_error(
            &mut errors,
            Error::new(name.span(), "only structs with named fields are supported"),
        );
        return Err(errors.expect("an error"));
    };
    let deny: bool = container_deny_unknown(input, &mut errors);
    let specs: Vec<FieldSpec> = named
        .named
        .iter()
        .map(|field| field_spec(field, &mut errors))
        .collect();

    let mut flatten: Option<&FieldSpec> = None;
    let mut fields: Vec<&FieldSpec> = Vec::new();
    for spec in &specs {
        if !spec.flatten {
            if let Some(earlier) = fields.iter().find(|f| f.key == spec.key) {
                push_error(
                    &mut errors,
                    Error::new(
                        spec.span,
                        format!("key \"{}\" is also field `{}`'s", spec.key, earlier.ident),
                    ),
                );
            }
            fields.push(spec);
        } else if flatten.is_some() {
            push_error(
                &mut errors,
                Error::new(spec.span, "at most one field can be `flatten`"),
            );
        } else if spec.default || spec.ident.unraw() != spec.key {
            push_error(
                &mut errors,
                Error::new(
                    spec.span,
                    "a `flatten` field takes no `rename` or `default`",
                ),
            );
        } else {
            flatten = Some(spec);
        }
    }
    if let Some(spec) = flatten
        && deny
    {
        push_error(
            &mut errors,
            Error::new(
                spec.span,
                "`flatten` keeps every unknown key, so `deny_unknown_fields` cannot apply",
            ),
        );
    }
    if let Some(errors) = errors {
        return Err(errors);
    }
    Ok(render(name, deny, &fields, flatten))
}

/// The impl for the checked struct `name`.
fn render(
    name: &Ident,
    deny: bool,
    fields: &[&FieldSpec],
    flatten: Option<&FieldSpec>,
) -> TokenStream2 {
    let private: TokenStream2 = quote! { ::json_schema_rs::generated_serde::__private };
    let expecting: String = format!("struct {name}");
    let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
    let mut wrappers: Vec<TokenStream2> = Vec::new();
    let mut slots: Vec<TokenStream2> = Vec::new();
    let mut arms: Vec<TokenStream2> = Vec::new();
    let mut inits: Vec<TokenStream2> = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let slot: Ident = format_ident!("__field{index}");
        let ty: &Type = &field.ty;
        let key: &str = &field.key;
        let ident: &Ident = &field.ident;
        // A plain field's slot type is inferred from the field it initialises, so an `Option`
        // field never spells out `Option<Option<T>>` (clippy's `option_option` lint in the
        // user's crate); a `deserialize_with` slot names its wrapper, whose `.0` needs the type.
        slots.push(if let Some(path) = &field.with {
            let wrapper: Ident = format_ident!("__DeserializeWith{index}");
            wrappers.push(wrapper_type(&wrapper, ty, path));
            quote! {
                let mut #slot: ::core::option::Option<#wrapper> = ::core::option::Option::None;
            }
        } else {
            quote! {
                let mut #slot = ::core::option::Option::None;
            }
        });
        arms.push(quote! { #key => #private::read_field(&mut __map, &mut #slot, #key)?, });
        let value: TokenStream2 = if field.with.is_some() {
            quote! { __value.0 }
        } else {
            quote! { __value }
        };
        let absent: TokenStream2 = match (field.default, field.with.is_some()) {
            (true, _) => quote! { ::core::default::Default::default() },
            (false, true) => quote! {
                return ::core::result::Result::Err(
                    <__A::Error as ::serde::de::Error>::missing_field(#key),
                )
            },
            (false, false) => quote! { #private::missing_field::<_, __A::Error>(#key)? },
        };
        inits.push(quote! {
            #ident: match #slot {
                ::core::option::Option::Some(__value) => #value,
                ::core::option::Option::None => #absent,
            },
        });
    }
    let (additional, fallback): (TokenStream2, TokenStream2) =
        unknown_keys(&private, deny, flatten, &mut wrappers, &mut inits);
    quote! {
        // A deprecated type or field is still read, as serde's derive does.
        #[automatically_derived]
        #[allow(deprecated)]
        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<__D: ::serde::Deserializer<'de>>(
                __deserializer: __D,
            ) -> ::core::result::Result<Self, __D::Error> {
                #(#wrappers)*

                struct __Visitor;

                impl<'de> ::serde::de::Visitor<'de> for __Visitor {
                    type Value = #name;

                    fn expecting(
                        &self,
                        __formatter: &mut ::core::fmt::Formatter<'_>,
                    ) -> ::core::fmt::Result {
                        __formatter.write_str(#expecting)
                    }

                    fn visit_map<__A: ::serde::de::MapAccess<'de>>(
                        self,
                        mut __map: __A,
                    ) -> ::core::result::Result<#name, __A::Error> {
                        const __KEYS: &[&str] = &[#(#keys),*];
                        #(#slots)*
                        #additional
                        while let ::core::option::Option::Some(__key) =
                            ::serde::de::MapAccess::next_key_seed(&mut __map, #private::KeySeed(__KEYS))?
                        {
                            match __key.as_str() {
                                #(#arms)*
                                _ => #fallback,
                            }
                        }
                        ::core::result::Result::Ok(#name { #(#inits)* })
                    }
                }

                ::serde::Deserializer::deserialize_map(__deserializer, __Visitor)
            }
        }
    }
}

/// What the visitor does with a key that is not a field: the declaration of the flattened map
/// it goes into (adding the map's field to `inits` and its value wrapper to `wrappers`), and the
/// match arm's expression.
fn unknown_keys(
    private: &TokenStream2,
    deny: bool,
    flatten: Option<&FieldSpec>,
    wrappers: &mut Vec<TokenStream2>,
    inits: &mut Vec<TokenStream2>,
) -> (TokenStream2, TokenStream2) {
    match flatten {
        Some(field) => {
            let ty: &Type = &field.ty;
            let ident: &Ident = &field.ident;
            inits.push(quote! { #ident: __additional, });
            let convert: TokenStream2 = if let Some(path) = &field.with {
                let wrapper: Ident = format_ident!("__DeserializeWithAdditional");
                let value_ty: TokenStream2 =
                    quote! { <#ty as #private::AdditionalProperties>::Value };
                wrappers.push(wrapper_type(&wrapper, &syn::parse_quote!(#value_ty), path));
                quote! { |__value: #wrapper| __value.0 }
            } else {
                quote! { ::core::convert::identity }
            };
            (
                quote! { let mut __additional: #ty = ::core::default::Default::default(); },
                quote! { #private::read_additional(&mut __map, &mut __additional, __key, #convert)? },
            )
        }
        None if deny => (
            TokenStream2::new(),
            quote! { return ::core::result::Result::Err(#private::unknown_field(&__key, __KEYS)) },
        ),
        None => (
            TokenStream2::new(),
            quote! { #private::skip_value(&mut __map)? },
        ),
    }
}

/// A newtype around `ty` whose `Deserialize` is the `deserialize_with` function `path`.
fn wrapper_type(wrapper: &Ident, ty: &Type, path: &ExprPath) -> TokenStream2 {
    quote! {
        // The wrapper repeats the field's type: a nullable field's `Option<Option<T>>` is
        // deliberate (absent vs null), so clippy's lint on that spelling stays on the field.
        #[allow(clippy::option_option)]
        struct #wrapper(#ty);

        impl<'de> ::serde::Deserialize<'de> for #wrapper {
            fn deserialize<__D: ::serde::Deserializer<'de>>(
                __deserializer: __D,
            ) -> ::core::result::Result<Self, __D::Error> {
                // A path such as `Option::deserialize` names the trait's method.
                #[allow(unused_imports)]
                use ::serde::Deserialize as _;
                #path(__deserializer).map(#wrapper)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::expand_deserialize;
    use proc_macro2::TokenStream;
    use std::str::FromStr;
    use syn::DeriveInput;

    /// The errors the derive reports for `source`, each with its line (1-based) and column
    /// (0-based); empty when it expands.
    fn derive_errors(source: &str) -> Vec<(String, usize, usize)> {
        let tokens: TokenStream = TokenStream::from_str(source).expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        match expand_deserialize(&input) {
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

    /// The expansion for `source`, formatted.
    fn expansion(source: &str) -> String {
        let tokens: TokenStream = TokenStream::from_str(source).expect("tokenize");
        let input: DeriveInput = syn::parse2(tokens).expect("parse");
        let expanded: TokenStream = expand_deserialize(&input).expect("expand");
        expanded.to_string()
    }

    #[test]
    fn every_supported_attribute_expands() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> = derive_errors(
            r#"struct S {
                #[serde(rename = "a-b", skip_serializing_if = "Option::is_none")]
                a: Option<u8>,
                #[serde(default, deserialize_with = "f")]
                b: u8,
                #[serde(flatten, deserialize_with = "g")]
                rest: BTreeMap<String, u8>,
            }"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn deny_unknown_fields_expands() {
        let expected: Vec<(String, usize, usize)> = Vec::new();
        let actual: Vec<(String, usize, usize)> =
            derive_errors("#[serde(deny_unknown_fields)] struct S { a: u8 }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unsupported_attributes_are_errors_at_their_span() {
        let expected: Vec<(String, usize, usize)> = vec![
            ("unsupported serde container attribute".to_string(), 1, 8),
            ("unsupported serde field attribute".to_string(), 1, 54),
            (
                "`default = \"...\"` is unsupported; use `default`".to_string(),
                1,
                83,
            ),
        ];
        let actual: Vec<(String, usize, usize)> = derive_errors(
            r#"#[serde(rename_all = "camelCase")] struct S { #[serde(alias = "x")] a: u8, #[serde(default = "d")] b: u8 }"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn generic_struct_is_an_error() {
        let expected: Vec<(String, usize, usize)> =
            vec![("generic structs are unsupported".to_string(), 1, 8)];
        let actual: Vec<(String, usize, usize)> = derive_errors("struct S<T> { a: T }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn tuple_struct_is_an_error() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "only structs with named fields are supported".to_string(),
            1,
            7,
        )];
        let actual: Vec<(String, usize, usize)> = derive_errors("struct S(u8);");
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_is_an_error() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "only structs with named fields are supported".to_string(),
            1,
            5,
        )];
        let actual: Vec<(String, usize, usize)> = derive_errors("enum E { A }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn second_flatten_field_is_an_error() {
        let expected: Vec<(String, usize, usize)> =
            vec![("at most one field can be `flatten`".to_string(), 1, 54)];
        let actual: Vec<(String, usize, usize)> = derive_errors(
            "struct S { #[serde(flatten)] a: BTreeMap<String, u8>, #[serde(flatten)] b: BTreeMap<String, u8> }",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn renamed_flatten_field_is_an_error() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "a `flatten` field takes no `rename` or `default`".to_string(),
            1,
            11,
        )];
        let actual: Vec<(String, usize, usize)> = derive_errors(
            r#"struct S { #[serde(flatten, rename = "x")] a: BTreeMap<String, u8> }"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn flatten_with_deny_unknown_fields_is_an_error() {
        let expected: Vec<(String, usize, usize)> = vec![(
            "`flatten` keeps every unknown key, so `deny_unknown_fields` cannot apply".to_string(),
            1,
            41,
        )];
        let actual: Vec<(String, usize, usize)> = derive_errors(
            "#[serde(deny_unknown_fields)] struct S { #[serde(flatten)] a: BTreeMap<String, u8> }",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn repeated_key_is_an_error() {
        let expected: Vec<(String, usize, usize)> =
            vec![("key \"a\" is also field `a`'s".to_string(), 1, 18)];
        let actual: Vec<(String, usize, usize)> =
            derive_errors(r#"struct S { a: u8, #[serde(rename = "a")] b: u8 }"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn expansion_reads_every_field_through_the_shared_reader() {
        let expected: String = concat!(
            "# [automatically_derived] # [allow (deprecated)] ",
            "impl < 'de > :: serde :: Deserialize < 'de > for Pet { ",
            "fn deserialize < __D : :: serde :: Deserializer < 'de >> (__deserializer : __D ,) ",
            "-> :: core :: result :: Result < Self , __D :: Error > { ",
            "# [allow (clippy :: option_option)] struct __DeserializeWith1 (Option < u8 >) ; ",
            "impl < 'de > :: serde :: Deserialize < 'de > for __DeserializeWith1 { ",
            "fn deserialize < __D : :: serde :: Deserializer < 'de >> (__deserializer : __D ,) ",
            "-> :: core :: result :: Result < Self , __D :: Error > { ",
            "# [allow (unused_imports)] use :: serde :: Deserialize as _ ; ",
            "Option :: deserialize (__deserializer) . map (__DeserializeWith1) } } ",
            "struct __Visitor ; ",
            "impl < 'de > :: serde :: de :: Visitor < 'de > for __Visitor { ",
            "type Value = Pet ; ",
            "fn expecting (& self , __formatter : & mut :: core :: fmt :: Formatter < '_ > ,) ",
            "-> :: core :: fmt :: Result { __formatter . write_str (\"struct Pet\") } ",
            "fn visit_map < __A : :: serde :: de :: MapAccess < 'de >> (self , mut __map : __A ,) ",
            "-> :: core :: result :: Result < Pet , __A :: Error > { ",
            "const __KEYS : & [& str] = & [\"name\" , \"age-years\"] ; ",
            "let mut __field0 = :: core :: option :: Option :: None ; ",
            "let mut __field1 : :: core :: option :: Option < __DeserializeWith1 > = :: core :: option :: Option :: None ; ",
            "while let :: core :: option :: Option :: Some (__key) = ",
            ":: serde :: de :: MapAccess :: next_key_seed (& mut __map , ",
            ":: json_schema_rs :: generated_serde :: __private :: KeySeed (__KEYS)) ? { ",
            "match __key . as_str () { ",
            "\"name\" => :: json_schema_rs :: generated_serde :: __private :: read_field (& mut __map , & mut __field0 , \"name\") ? , ",
            "\"age-years\" => :: json_schema_rs :: generated_serde :: __private :: read_field (& mut __map , & mut __field1 , \"age-years\") ? , ",
            "_ => :: json_schema_rs :: generated_serde :: __private :: skip_value (& mut __map) ? , } } ",
            ":: core :: result :: Result :: Ok (Pet { ",
            "name : match __field0 { :: core :: option :: Option :: Some (__value) => __value , ",
            ":: core :: option :: Option :: None => :: json_schema_rs :: generated_serde :: __private :: missing_field :: < _ , __A :: Error > (\"name\") ? , } , ",
            "age_years : match __field1 { :: core :: option :: Option :: Some (__value) => __value . 0 , ",
            ":: core :: option :: Option :: None => return :: core :: result :: Result :: Err (< __A :: Error as :: serde :: de :: Error > :: missing_field (\"age-years\") ,) , } , ",
            "}) } } ",
            ":: serde :: Deserializer :: deserialize_map (__deserializer , __Visitor) } }",
        )
        .to_string();
        let actual: String = expansion(
            r#"struct Pet {
                name: String,
                #[serde(rename = "age-years", deserialize_with = "Option::deserialize")]
                age_years: Option<u8>,
            }"#,
        );
        assert_eq!(expected, actual);
    }
}
