//! Settings arguments of `json_schema_to_rust!`: `name = value` after the schema literals, one
//! per [`CodeGenSettings`] field and [`JsonSchemaSettings`] field, named like the builder
//! methods. A value is `true` / `false` for a flag and the variant name for an enum
//! (`dedupe_mode = Disabled`, `forced_spec_version = Draft07`), a bracketed list for
//! `extra_derives` (paths) and `extra_attrs` (the text inside `#[...]`), and a braced map of type
//! names to `{ rename = "Name", derives = [..], attrs = [..] }` for `patch`. A setting left out
//! keeps its builder default.

use json_schema_rs::{
    CodeGenSettings, ContentMode, CrateSpec, DedupeMode, ExclusiveBounds, FormatMode, FormatTypes,
    IntegerSemantics, JsonSchemaSettings, MapType, MetaschemaCheck, ModelNameSource, RefSiblings,
    SpecVersion, TypeConversion, TypePatch, UniqueItemsType, UnknownCrates, UnknownFormats,
    UnknownProperties, UnknownSchemaUri,
};
use proc_macro2::{Delimiter, Span, TokenTree};
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, LitBool, LitStr, Meta, Result as SynResult, Token};

/// One `name = value` argument, as written.
pub(crate) struct SettingArg {
    pub(crate) name: Ident,
    pub(crate) value: SettingValue,
}

/// The value of a [`SettingArg`]: a boolean literal, an identifier (an enum variant name), a
/// string literal, a bracketed list of paths or attributes, or a braced list of arguments.
pub(crate) enum SettingValue {
    Bool(LitBool),
    Ident(Ident),
    Str(LitStr),
    List(Span, Vec<Meta>),
    Group(Span, Vec<SettingArg>),
    /// `convert = { schema => path, .. }`.
    Conversions(Span, Vec<(Span, TypeConversion)>),
    /// `replace = { Name = path, .. }`.
    Replacements(Span, Vec<(Ident, String)>),
    /// `crates = ["[rename=]crate@version", ..]`.
    Strings(Span, Vec<LitStr>),
}

impl SettingValue {
    fn span(&self) -> Span {
        match self {
            SettingValue::Bool(lit) => lit.span(),
            SettingValue::Ident(ident) => ident.span(),
            SettingValue::Str(lit) => lit.span(),
            SettingValue::List(span, _)
            | SettingValue::Group(span, _)
            | SettingValue::Conversions(span, _)
            | SettingValue::Replacements(span, _)
            | SettingValue::Strings(span, _) => *span,
        }
    }

    /// The value as written, for an error message.
    fn written(&self) -> String {
        match self {
            SettingValue::Bool(lit) => lit.value.to_string(),
            SettingValue::Ident(ident) => ident.to_string(),
            SettingValue::Str(lit) => format!("{:?}", lit.value()),
            SettingValue::List(..) | SettingValue::Strings(..) => "[..]".to_string(),
            SettingValue::Group(..)
            | SettingValue::Conversions(..)
            | SettingValue::Replacements(..) => "{..}".to_string(),
        }
    }
}

impl Parse for SettingArg {
    fn parse(input: ParseStream<'_>) -> SynResult<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let braced: bool = input.peek(syn::token::Brace);
        let value: SettingValue = if name == "convert" && braced {
            parse_conversions(input)?
        } else if name == "replace" && braced {
            parse_replacements(input)?
        } else if name == "crates" && input.peek(syn::token::Bracket) {
            let content;
            let bracket = syn::bracketed!(content in input);
            let items: Punctuated<LitStr, Token![,]> = Punctuated::parse_terminated(&content)?;
            SettingValue::Strings(bracket.span.join(), items.into_iter().collect())
        } else if input.peek(LitBool) {
            SettingValue::Bool(input.parse()?)
        } else if input.peek(Ident) {
            SettingValue::Ident(input.parse()?)
        } else if input.peek(LitStr) {
            SettingValue::Str(input.parse()?)
        } else if input.peek(syn::token::Bracket) {
            let content;
            let bracket = syn::bracketed!(content in input);
            let items: Punctuated<Meta, Token![,]> = Punctuated::parse_terminated(&content)?;
            SettingValue::List(bracket.span.join(), items.into_iter().collect())
        } else if input.peek(syn::token::Brace) {
            let content;
            let brace = syn::braced!(content in input);
            let args: Punctuated<SettingArg, Token![,]> = Punctuated::parse_terminated(&content)?;
            SettingValue::Group(brace.span.join(), args.into_iter().collect())
        } else {
            return Err(input.error(
                "expected a setting value: `true`, `false`, a variant name, a string, `[..]` or \
                 `{..}`",
            ));
        };
        Ok(SettingArg { name, value })
    }
}

/// `{ schema => path, .. }`: each schema JSON tokens or a string literal holding JSON.
fn parse_conversions(input: ParseStream<'_>) -> SynResult<SettingValue> {
    let content;
    let brace = syn::braced!(content in input);
    let mut out: Vec<(Span, TypeConversion)> = Vec::new();
    while !content.is_empty() {
        let schema: TokenTree = content.parse()?;
        let span: Span = schema.span();
        let json: String = match syn::parse2::<LitStr>(schema.to_token_stream()) {
            Ok(lit) => lit.value(),
            Err(_) => json_text(schema),
        };
        let schema: serde_json::Value = serde_json::from_str(&json).map_err(|error| {
            syn::Error::new(span, format!("conversion schema is not JSON: {error}"))
        })?;
        content.parse::<Token![=>]>()?;
        let ty: syn::Type = content.parse()?;
        out.push((
            span,
            TypeConversion {
                schema,
                rust_type: type_text(&ty),
            },
        ));
        if !content.is_empty() {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(SettingValue::Conversions(brace.span.join(), out))
}

/// `{ Name = path, .. }`.
fn parse_replacements(input: ParseStream<'_>) -> SynResult<SettingValue> {
    let content;
    let brace = syn::braced!(content in input);
    let mut out: Vec<(Ident, String)> = Vec::new();
    while !content.is_empty() {
        let name: Ident = content.parse()?;
        content.parse::<Token![=]>()?;
        let ty: syn::Type = content.parse()?;
        out.push((name, type_text(&ty)));
        if !content.is_empty() {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(SettingValue::Replacements(brace.span.join(), out))
}

/// A type as the path text codegen takes (`rust_decimal::Decimal`, `Vec<u8>`, `&'a str`): the
/// tokens with a space kept only between two word characters, where it separates them.
fn type_text(ty: &syn::Type) -> String {
    let tokens: String = ty.to_token_stream().to_string();
    let word = |c: Option<char>| c.is_some_and(|c: char| c.is_alphanumeric() || c == '_');
    let mut out: String = String::with_capacity(tokens.len());
    let mut chars = tokens.chars().peekable();
    while let Some(c) = chars.next() {
        if c != ' ' || (word(out.chars().last()) && word(chars.peek().copied())) {
            out.push(c);
        }
    }
    out
}

/// The JSON text of JSON written as Rust tokens: every token as written, with no space between
/// (a space would split `-1` and is never needed between JSON tokens). Iterative, so deeply
/// nested input cannot overflow the stack.
fn json_text(tree: TokenTree) -> String {
    let mut out: String = String::new();
    // Each open group's remaining tokens and its closing delimiter.
    let mut stack: Vec<(std::vec::IntoIter<TokenTree>, &'static str)> =
        vec![(vec![tree].into_iter(), "")];
    while let Some((tokens, _)) = stack.last_mut() {
        let Some(token) = tokens.next() else {
            if let Some((_, close)) = stack.pop() {
                out.push_str(close);
            }
            continue;
        };
        match token {
            TokenTree::Group(group) => {
                let (open, close): (&str, &'static str) = match group.delimiter() {
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::None => ("", ""),
                };
                out.push_str(open);
                let inner: Vec<TokenTree> = group.stream().into_iter().collect();
                stack.push((inner.into_iter(), close));
            }
            other => out.push_str(&other.to_string()),
        }
    }
    out
}

/// Every setting name, in the order the error for an unknown name lists them.
const SETTING_NAMES: [&str; 27] = [
    "model_name_source",
    "dedupe_mode",
    "unknown_properties",
    "unique_items",
    "map_type",
    "flatten_single_entry_combinators",
    "self_validating",
    "constrained_newtypes",
    "format_types",
    "extra_derives",
    "extra_attrs",
    "patch",
    "convert",
    "replace",
    "crates",
    "unknown_crates",
    "disallow_unknown_fields",
    "forced_spec_version",
    "default_spec_version",
    "format_mode",
    "content_mode",
    "unknown_formats",
    "integer_semantics",
    "exclusive_bounds",
    "ref_siblings",
    "unknown_schema_uri",
    "metaschema_check",
];

const UNKNOWN_CRATES: &[(&str, UnknownCrates)] = &[
    ("Generate", UnknownCrates::Generate),
    ("Allow", UnknownCrates::Allow),
    ("Deny", UnknownCrates::Deny),
];

const MODEL_NAME_SOURCES: &[(&str, ModelNameSource)] = &[
    ("TitleFirst", ModelNameSource::TitleFirst),
    ("PropertyKeyFirst", ModelNameSource::PropertyKeyFirst),
];

const DEDUPE_MODES: &[(&str, DedupeMode)] = &[
    ("Disabled", DedupeMode::Disabled),
    ("Functional", DedupeMode::Functional),
    ("Full", DedupeMode::Full),
];

const UNKNOWN_PROPERTIES: &[(&str, UnknownProperties)] = &[
    ("Capture", UnknownProperties::Capture),
    ("Drop", UnknownProperties::Drop),
];

const FORMAT_TYPES: &[(&str, FormatTypes)] = &[
    ("Typed", FormatTypes::Typed),
    ("FollowDialect", FormatTypes::FollowDialect),
];

const SPEC_VERSIONS: &[(&str, SpecVersion)] = &[
    ("Draft00", SpecVersion::Draft00),
    ("Draft01", SpecVersion::Draft01),
    ("Draft02", SpecVersion::Draft02),
    ("Draft03", SpecVersion::Draft03),
    ("Draft04", SpecVersion::Draft04),
    ("Draft05", SpecVersion::Draft05),
    ("Draft06", SpecVersion::Draft06),
    ("Draft07", SpecVersion::Draft07),
    ("Draft201909", SpecVersion::Draft201909),
    ("Draft202012", SpecVersion::Draft202012),
];

const FORMAT_MODES: &[(&str, FormatMode)] = &[
    ("Annotation", FormatMode::Annotation),
    ("Assertion", FormatMode::Assertion),
];

const CONTENT_MODES: &[(&str, ContentMode)] = &[
    ("Annotation", ContentMode::Annotation),
    ("Assertion", ContentMode::Assertion),
];

const UNKNOWN_FORMATS: &[(&str, UnknownFormats)] = &[
    ("Ignore", UnknownFormats::Ignore),
    ("Reject", UnknownFormats::Reject),
];

const INTEGER_SEMANTICS: &[(&str, IntegerSemantics)] = &[
    ("Lexical", IntegerSemantics::Lexical),
    ("Numeric", IntegerSemantics::Numeric),
];

const EXCLUSIVE_BOUNDS: &[(&str, ExclusiveBounds)] = &[
    ("BooleanModifier", ExclusiveBounds::BooleanModifier),
    ("Numeric", ExclusiveBounds::Numeric),
];

const REF_SIBLINGS: &[(&str, RefSiblings)] = &[
    ("Ignored", RefSiblings::Ignored),
    ("Evaluated", RefSiblings::Evaluated),
];

const UNKNOWN_SCHEMA_URIS: &[(&str, UnknownSchemaUri)] = &[
    ("Fallback", UnknownSchemaUri::Fallback),
    ("Reject", UnknownSchemaUri::Reject),
];

const METASCHEMA_CHECKS: &[(&str, MetaschemaCheck)] = &[
    ("Skip", MetaschemaCheck::Skip),
    ("Validate", MetaschemaCheck::Validate),
];

/// The variant names of `unique_items`, whether or not this build has `IndexSet`.
const UNIQUE_ITEMS_NAMES: &[&str] = &["Vec", "HashSet", "BTreeSet", "IndexSet"];

/// The variant names of `map_type`, whether or not this build has `IndexMap`.
const MAP_TYPE_NAMES: &[&str] = &["BTreeMap", "HashMap", "IndexMap"];

/// The settings the arguments set; `None` keeps the builder default.
#[derive(Default)]
struct Overrides {
    model_name_source: Option<ModelNameSource>,
    dedupe_mode: Option<DedupeMode>,
    unknown_properties: Option<UnknownProperties>,
    unique_items: Option<UniqueItemsType>,
    map_type: Option<MapType>,
    flatten_single_entry_combinators: Option<bool>,
    self_validating: Option<bool>,
    constrained_newtypes: Option<bool>,
    format_types: Option<FormatTypes>,
    extra_derives: Option<Vec<String>>,
    extra_attrs: Option<Vec<String>>,
    patches: Option<BTreeMap<String, TypePatch>>,
    conversions: Option<Vec<TypeConversion>>,
    replacements: Option<BTreeMap<String, String>>,
    crates: Option<BTreeMap<String, CrateSpec>>,
    unknown_crates: Option<UnknownCrates>,
    disallow_unknown_fields: Option<bool>,
    forced_spec_version: Option<SpecVersion>,
    default_spec_version: Option<SpecVersion>,
    format_mode: Option<FormatMode>,
    content_mode: Option<ContentMode>,
    unknown_formats: Option<UnknownFormats>,
    integer_semantics: Option<IntegerSemantics>,
    exclusive_bounds: Option<ExclusiveBounds>,
    ref_siblings: Option<RefSiblings>,
    unknown_schema_uri: Option<UnknownSchemaUri>,
    metaschema_check: Option<MetaschemaCheck>,
}

/// The settings a macro call's arguments produce.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MacroSettings {
    pub(crate) code_gen: CodeGenSettings,
    pub(crate) json_schema: JsonSchemaSettings,
}

impl Overrides {
    /// Build both settings. Each struct is written out field by field (no `..`), so a field added
    /// to either fails to compile here until the macro accepts it.
    fn build(self) -> MacroSettings {
        let code_gen_default: CodeGenSettings = CodeGenSettings::builder().build();
        let json_schema_default: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        MacroSettings {
            code_gen: CodeGenSettings {
                model_name_source: self
                    .model_name_source
                    .unwrap_or(code_gen_default.model_name_source),
                dedupe_mode: self.dedupe_mode.unwrap_or(code_gen_default.dedupe_mode),
                unknown_properties: self
                    .unknown_properties
                    .or(code_gen_default.unknown_properties),
                unique_items: self.unique_items.unwrap_or(code_gen_default.unique_items),
                map_type: self.map_type.unwrap_or(code_gen_default.map_type),
                flatten_single_entry_combinators: self
                    .flatten_single_entry_combinators
                    .unwrap_or(code_gen_default.flatten_single_entry_combinators),
                self_validating: self
                    .self_validating
                    .unwrap_or(code_gen_default.self_validating),
                constrained_newtypes: self
                    .constrained_newtypes
                    .unwrap_or(code_gen_default.constrained_newtypes),
                format_types: self.format_types.unwrap_or(code_gen_default.format_types),
                extra_derives: self.extra_derives.unwrap_or(code_gen_default.extra_derives),
                extra_attrs: self.extra_attrs.unwrap_or(code_gen_default.extra_attrs),
                patches: self.patches.unwrap_or(code_gen_default.patches),
                conversions: self.conversions.unwrap_or(code_gen_default.conversions),
                replacements: self.replacements.unwrap_or(code_gen_default.replacements),
                crates: self.crates.unwrap_or(code_gen_default.crates),
                unknown_crates: self
                    .unknown_crates
                    .unwrap_or(code_gen_default.unknown_crates),
            },
            json_schema: JsonSchemaSettings {
                disallow_unknown_fields: self
                    .disallow_unknown_fields
                    .unwrap_or(json_schema_default.disallow_unknown_fields),
                forced_spec_version: self
                    .forced_spec_version
                    .or(json_schema_default.forced_spec_version),
                default_spec_version: self
                    .default_spec_version
                    .or(json_schema_default.default_spec_version),
                format_mode: self.format_mode.or(json_schema_default.format_mode),
                content_mode: self.content_mode.or(json_schema_default.content_mode),
                unknown_formats: self.unknown_formats.or(json_schema_default.unknown_formats),
                integer_semantics: self
                    .integer_semantics
                    .or(json_schema_default.integer_semantics),
                exclusive_bounds: self
                    .exclusive_bounds
                    .or(json_schema_default.exclusive_bounds),
                ref_siblings: self.ref_siblings.or(json_schema_default.ref_siblings),
                unknown_schema_uri: self
                    .unknown_schema_uri
                    .unwrap_or(json_schema_default.unknown_schema_uri),
                metaschema_check: self
                    .metaschema_check
                    .unwrap_or(json_schema_default.metaschema_check),
            },
        }
    }
}

/// `names` as `` `a`, `b` or `c` ``.
fn one_of(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// The boolean `value` of setting `setting`.
fn boolean(setting: &str, value: &SettingValue) -> SynResult<bool> {
    match value {
        SettingValue::Bool(lit) => Ok(lit.value),
        _ => Err(syn::Error::new(
            value.span(),
            format!(
                "`{setting}` takes `true` or `false`, not `{}`",
                value.written()
            ),
        )),
    }
}

/// The variant `value` names among `variants` of setting `setting`.
fn variant<T: Copy>(setting: &str, value: &SettingValue, variants: &[(&str, T)]) -> SynResult<T> {
    let names: Vec<&str> = variants.iter().map(|(name, _)| *name).collect();
    let found: Option<T> = match value {
        SettingValue::Ident(ident) => variants
            .iter()
            .find(|(name, _)| ident == name)
            .map(|(_, variant)| *variant),
        _ => None,
    };
    found.ok_or_else(|| unknown_value(setting, value, &names))
}

fn unknown_value(setting: &str, value: &SettingValue, names: &[&str]) -> syn::Error {
    let written: String = value.written();
    syn::Error::new(
        value.span(),
        format!(
            "unknown value `{written}` for `{setting}`; expected {}",
            one_of(names)
        ),
    )
}

#[cfg(feature = "indexmap")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "fails only without the `indexmap` feature"
)]
fn index_set(_span: Span) -> SynResult<UniqueItemsType> {
    Ok(UniqueItemsType::IndexSet)
}

/// `unique_items = IndexSet` when the macro is built without its `indexmap` feature.
#[cfg(not(feature = "indexmap"))]
fn index_set(span: Span) -> SynResult<UniqueItemsType> {
    Err(syn::Error::new(
        span,
        "`unique_items = IndexSet` needs the `indexmap` feature of `json-schema-rs-macro`",
    ))
}

/// The `unique_items` variant `value` names.
fn unique_items(value: &SettingValue) -> SynResult<UniqueItemsType> {
    match value {
        SettingValue::Ident(ident) if ident == "IndexSet" => index_set(ident.span()),
        _ => variant(
            "unique_items",
            value,
            &[
                ("Vec", UniqueItemsType::Vec),
                ("HashSet", UniqueItemsType::HashSet),
                ("BTreeSet", UniqueItemsType::BTreeSet),
            ],
        )
        .map_err(|_| unknown_value("unique_items", value, UNIQUE_ITEMS_NAMES)),
    }
}

#[cfg(feature = "indexmap")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "fails only without the `indexmap` feature"
)]
fn index_map(_span: Span) -> SynResult<MapType> {
    Ok(MapType::IndexMap)
}

/// `map_type = IndexMap` when the macro is built without its `indexmap` feature.
#[cfg(not(feature = "indexmap"))]
fn index_map(span: Span) -> SynResult<MapType> {
    Err(syn::Error::new(
        span,
        "`map_type = IndexMap` needs the `indexmap` feature of `json-schema-rs-macro`",
    ))
}

/// The `map_type` variant `value` names.
fn map_type(value: &SettingValue) -> SynResult<MapType> {
    match value {
        SettingValue::Ident(ident) if ident == "IndexMap" => index_map(ident.span()),
        _ => variant(
            "map_type",
            value,
            &[
                ("BTreeMap", MapType::BTreeMap),
                ("HashMap", MapType::HashMap),
            ],
        )
        .map_err(|_| unknown_value("map_type", value, MAP_TYPE_NAMES)),
    }
}

/// The items of the list `value` of setting `setting`.
fn list<'v>(setting: &str, value: &'v SettingValue) -> SynResult<&'v [Meta]> {
    match value {
        SettingValue::List(_, items) => Ok(items),
        _ => Err(syn::Error::new(
            value.span(),
            format!("`{setting}` takes a list `[..]`, not `{}`", value.written()),
        )),
    }
}

/// The derive paths of the list `value` (`[Hash, schemars::JsonSchema]`).
fn derives(setting: &str, value: &SettingValue) -> SynResult<Vec<String>> {
    list(setting, value)?
        .iter()
        .map(|item: &Meta| match item {
            Meta::Path(path) if path.segments.iter().all(|s| s.arguments.is_none()) => {
                let segments: Vec<String> =
                    path.segments.iter().map(|s| s.ident.to_string()).collect();
                let leading: &str = if path.leading_colon.is_some() {
                    "::"
                } else {
                    ""
                };
                Ok(format!("{leading}{}", segments.join("::")))
            }
            _ => Err(syn::Error::new_spanned(
                item,
                format!("`{setting}` takes derive macro paths"),
            )),
        })
        .collect()
}

/// The attributes of the list `value` (`[non_exhaustive, serde(deny_unknown_fields)]`), each
/// the text inside `#[...]`.
fn attrs(setting: &str, value: &SettingValue) -> SynResult<Vec<String>> {
    Ok(list(setting, value)?
        .iter()
        .map(|item: &Meta| item.to_token_stream().to_string())
        .collect())
}

/// The arguments of the braced `value`, which `what` takes.
fn group<'v>(value: &'v SettingValue, what: &str) -> SynResult<&'v [SettingArg]> {
    match value {
        SettingValue::Group(_, args) => Ok(args),
        _ => Err(syn::Error::new(
            value.span(),
            format!("{what} takes `{{..}}`, not `{}`", value.written()),
        )),
    }
}

/// The patches of the map `value`: `{ Name = { rename = "New", derives = [..], attrs = [..] } }`.
fn patches(value: &SettingValue) -> SynResult<BTreeMap<String, TypePatch>> {
    let mut out: BTreeMap<String, TypePatch> = BTreeMap::new();
    let mut errors: Option<syn::Error> = None;
    for entry in group(value, "`patch`")? {
        let name: String = entry.name.to_string();
        let mut patch: TypePatch = TypePatch::default();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let fields: &[SettingArg] = match group(&entry.value, &format!("the patch for `{name}`")) {
            Ok(fields) => fields,
            Err(error) => {
                crate::push_error(&mut errors, error);
                continue;
            }
        };
        for field in fields {
            let key: String = field.name.to_string();
            let result: SynResult<()> = if seen.insert(key.clone()) {
                match key.as_str() {
                    "rename" => match &field.value {
                        SettingValue::Str(lit) => {
                            patch.rename = Some(lit.value());
                            Ok(())
                        }
                        other => Err(syn::Error::new(
                            other.span(),
                            format!("`rename` takes a string, not `{}`", other.written()),
                        )),
                    },
                    "derives" => derives("derives", &field.value).map(|d| patch.derives = d),
                    "attrs" => attrs("attrs", &field.value).map(|a| patch.attrs = a),
                    _ => Err(syn::Error::new(
                        field.name.span(),
                        format!(
                            "unknown patch field `{key}`; expected `rename`, `derives` or `attrs`"
                        ),
                    )),
                }
            } else {
                Err(syn::Error::new(
                    field.name.span(),
                    format!("`{key}` is given more than once"),
                ))
            };
            if let Err(error) = result {
                crate::push_error(&mut errors, error);
            }
        }
        if out.insert(name.clone(), patch).is_some() {
            crate::push_error(
                &mut errors,
                syn::Error::new(
                    entry.name.span(),
                    format!("type `{name}` is patched more than once"),
                ),
            );
        }
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(out),
    }
}

/// The conversions of `convert`, in order (the first matching one wins).
fn conversions(value: &SettingValue) -> SynResult<Vec<TypeConversion>> {
    match value {
        SettingValue::Conversions(_, entries) => Ok(entries
            .iter()
            .map(|(_, conversion)| conversion.clone())
            .collect()),
        other => Err(syn::Error::new(
            other.span(),
            format!(
                "`convert` takes `{{ schema => path, .. }}`, not `{}`",
                other.written()
            ),
        )),
    }
}

/// The replacements of `replace`; a type named twice is an error at each repeat.
fn replacements(value: &SettingValue) -> SynResult<BTreeMap<String, String>> {
    let SettingValue::Replacements(_, entries) = value else {
        return Err(syn::Error::new(
            value.span(),
            format!(
                "`replace` takes `{{ Name = path, .. }}`, not `{}`",
                value.written()
            ),
        ));
    };
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut errors: Option<syn::Error> = None;
    for (name, rust_type) in entries {
        if out.insert(name.to_string(), rust_type.clone()).is_some() {
            crate::push_error(
                &mut errors,
                syn::Error::new(
                    name.span(),
                    format!("type `{name}` is replaced more than once"),
                ),
            );
        }
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(out),
    }
}

/// The crates of `crates`, each `[rename=]crate@version`; a crate named twice is an error at
/// its second spec.
fn crates(value: &SettingValue) -> SynResult<BTreeMap<String, CrateSpec>> {
    let SettingValue::Strings(_, specs) = value else {
        return Err(syn::Error::new(
            value.span(),
            format!(
                "`crates` takes `[\"[rename=]crate@version\", ..]`, not `{}`",
                value.written()
            ),
        ));
    };
    let mut out: BTreeMap<String, CrateSpec> = BTreeMap::new();
    let mut errors: Option<syn::Error> = None;
    for lit in specs {
        let error: Option<String> = match CrateSpec::parse_named(&lit.value()) {
            Ok((name, spec)) => out
                .insert(name.clone(), spec)
                .map(|_| format!("crate `{name}` is listed more than once")),
            Err(message) => Some(message),
        };
        if let Some(message) = error {
            crate::push_error(&mut errors, syn::Error::new(lit.span(), message));
        }
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(out),
    }
}

/// Store `value` in `slot`.
fn set<T>(slot: &mut Option<T>, value: SynResult<T>) -> SynResult<()> {
    *slot = Some(value?);
    Ok(())
}

/// Apply one argument to `overrides`.
fn apply(overrides: &mut Overrides, setting: &str, arg: &SettingArg) -> SynResult<()> {
    let value: &SettingValue = &arg.value;
    match setting {
        "model_name_source" => set(
            &mut overrides.model_name_source,
            variant(setting, value, MODEL_NAME_SOURCES),
        ),
        "dedupe_mode" => set(
            &mut overrides.dedupe_mode,
            variant(setting, value, DEDUPE_MODES),
        ),
        "unknown_properties" => set(
            &mut overrides.unknown_properties,
            variant(setting, value, UNKNOWN_PROPERTIES),
        ),
        "unique_items" => set(&mut overrides.unique_items, unique_items(value)),
        "map_type" => set(&mut overrides.map_type, map_type(value)),
        "flatten_single_entry_combinators" => set(
            &mut overrides.flatten_single_entry_combinators,
            boolean(setting, value),
        ),
        "self_validating" => set(&mut overrides.self_validating, boolean(setting, value)),
        "constrained_newtypes" => set(&mut overrides.constrained_newtypes, boolean(setting, value)),
        "format_types" => set(
            &mut overrides.format_types,
            variant(setting, value, FORMAT_TYPES),
        ),
        "extra_derives" => set(&mut overrides.extra_derives, derives(setting, value)),
        "extra_attrs" => set(&mut overrides.extra_attrs, attrs(setting, value)),
        "patch" => set(&mut overrides.patches, patches(value)),
        "convert" => set(&mut overrides.conversions, conversions(value)),
        "replace" => set(&mut overrides.replacements, replacements(value)),
        "crates" => set(&mut overrides.crates, crates(value)),
        "unknown_crates" => set(
            &mut overrides.unknown_crates,
            variant(setting, value, UNKNOWN_CRATES),
        ),
        "disallow_unknown_fields" => set(
            &mut overrides.disallow_unknown_fields,
            boolean(setting, value),
        ),
        "forced_spec_version" => set(
            &mut overrides.forced_spec_version,
            variant(setting, value, SPEC_VERSIONS),
        ),
        "default_spec_version" => set(
            &mut overrides.default_spec_version,
            variant(setting, value, SPEC_VERSIONS),
        ),
        "format_mode" => set(
            &mut overrides.format_mode,
            variant(setting, value, FORMAT_MODES),
        ),
        "content_mode" => set(
            &mut overrides.content_mode,
            variant(setting, value, CONTENT_MODES),
        ),
        "unknown_formats" => set(
            &mut overrides.unknown_formats,
            variant(setting, value, UNKNOWN_FORMATS),
        ),
        "integer_semantics" => set(
            &mut overrides.integer_semantics,
            variant(setting, value, INTEGER_SEMANTICS),
        ),
        "exclusive_bounds" => set(
            &mut overrides.exclusive_bounds,
            variant(setting, value, EXCLUSIVE_BOUNDS),
        ),
        "ref_siblings" => set(
            &mut overrides.ref_siblings,
            variant(setting, value, REF_SIBLINGS),
        ),
        "unknown_schema_uri" => set(
            &mut overrides.unknown_schema_uri,
            variant(setting, value, UNKNOWN_SCHEMA_URIS),
        ),
        "metaschema_check" => set(
            &mut overrides.metaschema_check,
            variant(setting, value, METASCHEMA_CHECKS),
        ),
        _ => Err(unknown_setting(arg)),
    }
}

fn unknown_setting(arg: &SettingArg) -> syn::Error {
    syn::Error::new(
        arg.name.span(),
        format!(
            "unknown setting `{}`; expected {}",
            arg.name,
            one_of(&SETTING_NAMES)
        ),
    )
}

/// The settings `args` produce, or one error per problem: an unknown setting name (at the name),
/// a setting given more than once (at each repeat), and an unknown or ill-typed value (at the
/// value).
pub(crate) fn resolve(args: &[SettingArg]) -> SynResult<MacroSettings> {
    let mut overrides: Overrides = Overrides::default();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut errors: Option<syn::Error> = None;
    for arg in args {
        let setting: String = arg.name.to_string();
        let result: SynResult<()> = if !SETTING_NAMES.contains(&setting.as_str()) {
            Err(unknown_setting(arg))
        } else if seen.insert(setting.clone()) {
            apply(&mut overrides, &setting, arg)
        } else {
            Err(syn::Error::new(
                arg.name.span(),
                format!("setting `{setting}` is given more than once"),
            ))
        };
        if let Err(error) = result {
            crate::push_error(&mut errors, error);
        }
    }
    match errors {
        Some(errors) => Err(errors),
        None => Ok(overrides.build()),
    }
}

#[cfg(test)]
mod tests {
    use super::{MacroSettings, SettingArg, resolve};
    use json_schema_rs::{
        CodeGenSettings, ContentMode, CrateSpec, CrateVersion, DedupeMode, ExclusiveBounds,
        FormatMode, FormatTypes, IntegerSemantics, JsonSchemaSettings, MapType, MetaschemaCheck,
        ModelNameSource, RefSiblings, SpecVersion, TypeConversion, TypePatch, UniqueItemsType,
        UnknownCrates, UnknownFormats, UnknownProperties, UnknownSchemaUri,
    };
    use proc_macro2::TokenStream;
    use std::collections::BTreeMap;
    use std::str::FromStr;
    use syn::Token;
    use syn::parse::Parser;
    use syn::punctuated::Punctuated;

    /// One error: its message, line (1-based) and column (0-based).
    type Located = (String, usize, usize);

    fn resolve_source(source: &str) -> syn::Result<MacroSettings> {
        let tokens: TokenStream = TokenStream::from_str(source).expect("tokenize");
        let args: Punctuated<SettingArg, Token![,]> = Punctuated::parse_terminated
            .parse2(tokens)
            .expect("parse settings");
        resolve(&args.into_iter().collect::<Vec<SettingArg>>())
    }

    fn errors(source: &str) -> Vec<Located> {
        match resolve_source(source) {
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
    fn no_settings_are_builder_defaults() {
        let expected: MacroSettings = MacroSettings {
            code_gen: CodeGenSettings::builder().build(),
            json_schema: JsonSchemaSettings::builder().build(),
        };
        let actual: MacroSettings = resolve_source("").expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_setting_reaches_its_field() {
        let expected: MacroSettings = MacroSettings {
            code_gen: CodeGenSettings::builder()
                .model_name_source(ModelNameSource::PropertyKeyFirst)
                .dedupe_mode(DedupeMode::Disabled)
                .unknown_properties(UnknownProperties::Drop)
                .unique_items(UniqueItemsType::BTreeSet)
                .map_type(MapType::HashMap)
                .flatten_single_entry_combinators(true)
                .self_validating(false)
                .constrained_newtypes(false)
                .format_types(FormatTypes::FollowDialect)
                .extra_derives(["schemars::JsonSchema", "::core::hash::Hash"])
                .extra_attrs(["non_exhaustive", "serde (rename_all = \"camelCase\")"])
                .patch(
                    "Veg",
                    TypePatch {
                        rename: Some("Vegetable".to_string()),
                        derives: vec!["Hash".to_string()],
                        attrs: vec!["must_use".to_string()],
                    },
                )
                .patch("Fruit", TypePatch::default())
                .build(),
            json_schema: JsonSchemaSettings::builder()
                .disallow_unknown_fields(true)
                .forced_spec_version(SpecVersion::Draft07)
                .default_spec_version(SpecVersion::Draft04)
                .format_mode(FormatMode::Assertion)
                .content_mode(ContentMode::Annotation)
                .unknown_formats(UnknownFormats::Reject)
                .integer_semantics(IntegerSemantics::Lexical)
                .exclusive_bounds(ExclusiveBounds::BooleanModifier)
                .ref_siblings(RefSiblings::Evaluated)
                .unknown_schema_uri(UnknownSchemaUri::Reject)
                .metaschema_check(MetaschemaCheck::Validate)
                .build(),
        };
        let actual: MacroSettings = resolve_source(
            "model_name_source = PropertyKeyFirst, dedupe_mode = Disabled, \
             unknown_properties = Drop, unique_items = BTreeSet, map_type = HashMap, \
             flatten_single_entry_combinators = true, self_validating = false, \
             constrained_newtypes = false, format_types = FollowDialect, \
             extra_derives = [schemars::JsonSchema, ::core::hash::Hash], \
             extra_attrs = [non_exhaustive, serde(rename_all = \"camelCase\")], \
             patch = { Veg = { rename = \"Vegetable\", derives = [Hash], attrs = [must_use] }, Fruit = {} }, \
             disallow_unknown_fields = true, \
             forced_spec_version = Draft07, default_spec_version = Draft04, \
             format_mode = Assertion, content_mode = Annotation, unknown_formats = Reject, integer_semantics = Lexical, \
             exclusive_bounds = BooleanModifier, ref_siblings = Evaluated, \
             unknown_schema_uri = Reject, metaschema_check = Validate",
        )
        .expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_setting_takes_its_other_values() {
        let expected: MacroSettings = MacroSettings {
            code_gen: CodeGenSettings::builder()
                .model_name_source(ModelNameSource::TitleFirst)
                .dedupe_mode(DedupeMode::Functional)
                .unknown_properties(UnknownProperties::Capture)
                .unique_items(UniqueItemsType::HashSet)
                .map_type(MapType::BTreeMap)
                .flatten_single_entry_combinators(false)
                .self_validating(true)
                .constrained_newtypes(true)
                .format_types(FormatTypes::Typed)
                .build(),
            json_schema: JsonSchemaSettings::builder()
                .disallow_unknown_fields(false)
                .forced_spec_version(SpecVersion::Draft201909)
                .default_spec_version(SpecVersion::Draft06)
                .format_mode(FormatMode::Annotation)
                .content_mode(ContentMode::Assertion)
                .unknown_formats(UnknownFormats::Ignore)
                .integer_semantics(IntegerSemantics::Numeric)
                .exclusive_bounds(ExclusiveBounds::Numeric)
                .ref_siblings(RefSiblings::Ignored)
                .unknown_schema_uri(UnknownSchemaUri::Fallback)
                .metaschema_check(MetaschemaCheck::Skip)
                .build(),
        };
        let actual: MacroSettings = resolve_source(
            "model_name_source = TitleFirst, dedupe_mode = Functional, \
             unknown_properties = Capture, unique_items = HashSet, map_type = BTreeMap, \
             flatten_single_entry_combinators = false, self_validating = true, \
             constrained_newtypes = true, format_types = Typed, disallow_unknown_fields = false, \
             forced_spec_version = Draft201909, default_spec_version = Draft06, \
             format_mode = Annotation, content_mode = Assertion, unknown_formats = Ignore, integer_semantics = Numeric, \
             exclusive_bounds = Numeric, ref_siblings = Ignored, \
             unknown_schema_uri = Fallback, metaschema_check = Skip",
        )
        .expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn spec_version_settings_take_every_draft() {
        let expected: Vec<Option<SpecVersion>> = [
            SpecVersion::Draft00,
            SpecVersion::Draft01,
            SpecVersion::Draft02,
            SpecVersion::Draft03,
            SpecVersion::Draft04,
            SpecVersion::Draft05,
            SpecVersion::Draft06,
            SpecVersion::Draft07,
            SpecVersion::Draft201909,
            SpecVersion::Draft202012,
        ]
        .map(Some)
        .to_vec();
        let actual: Vec<Option<SpecVersion>> = [
            "Draft00",
            "Draft01",
            "Draft02",
            "Draft03",
            "Draft04",
            "Draft05",
            "Draft06",
            "Draft07",
            "Draft201909",
            "Draft202012",
        ]
        .iter()
        .map(|name| {
            resolve_source(&format!("forced_spec_version = {name}"))
                .expect("resolve")
                .json_schema
                .forced_spec_version
        })
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_setting_is_an_error_at_its_name() {
        let expected: Vec<Located> = vec![(
            "unknown setting `dedupe`; expected `model_name_source`, `dedupe_mode`, \
             `unknown_properties`, `unique_items`, `map_type`, `flatten_single_entry_combinators`, \
             `self_validating`, `constrained_newtypes`, `format_types`, `extra_derives`, \
             `extra_attrs`, `patch`, `convert`, `replace`, `crates`, `unknown_crates`, `disallow_unknown_fields`, `forced_spec_version`, `default_spec_version`, `format_mode`, `content_mode`, `unknown_formats`, \
             `integer_semantics`, `exclusive_bounds`, `ref_siblings`, `unknown_schema_uri` or `metaschema_check`"
                .to_string(),
            1,
            0,
        )];
        let actual: Vec<Located> = errors("dedupe = Full");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_enum_value_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "unknown value `None` for `dedupe_mode`; expected `Disabled`, `Functional` or `Full`"
                .to_string(),
            1,
            14,
        )];
        let actual: Vec<Located> = errors("dedupe_mode = None");
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_for_enum_setting_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "unknown value `true` for `ref_siblings`; expected `Ignored` or `Evaluated`"
                .to_string(),
            1,
            15,
        )];
        let actual: Vec<Located> = errors("ref_siblings = true");
        assert_eq!(expected, actual);
    }

    #[test]
    fn identifier_for_flag_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "`self_validating` takes `true` or `false`, not `On`".to_string(),
            1,
            18,
        )];
        let actual: Vec<Located> = errors("self_validating = On");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_unique_items_value_lists_index_set() {
        let expected: Vec<Located> = vec![(
            "unknown value `Set` for `unique_items`; expected `Vec`, `HashSet`, `BTreeSet` or \
             `IndexSet`"
                .to_string(),
            1,
            15,
        )];
        let actual: Vec<Located> = errors("unique_items = Set");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_map_type_value_lists_index_map() {
        let expected: Vec<Located> = vec![(
            "unknown value `Map` for `map_type`; expected `BTreeMap`, `HashMap` or `IndexMap`"
                .to_string(),
            1,
            11,
        )];
        let actual: Vec<Located> = errors("map_type = Map");
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(feature = "indexmap")]
    fn index_map_with_feature_is_index_map() {
        let expected: MapType = MapType::IndexMap;
        let actual: MapType = resolve_source("map_type = IndexMap")
            .expect("resolve")
            .code_gen
            .map_type;
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(feature = "indexmap"))]
    fn index_map_without_feature_is_an_error_naming_the_feature() {
        let expected: Vec<Located> = vec![(
            "`map_type = IndexMap` needs the `indexmap` feature of `json-schema-rs-macro`"
                .to_string(),
            1,
            11,
        )];
        let actual: Vec<Located> = errors("map_type = IndexMap");
        assert_eq!(expected, actual);
    }

    #[test]
    fn repeated_setting_is_an_error_at_each_repeat() {
        let expected: Vec<Located> = vec![
            (
                "setting `dedupe_mode` is given more than once".to_string(),
                1,
                24,
            ),
            (
                "setting `dedupe_mode` is given more than once".to_string(),
                1,
                44,
            ),
        ];
        let actual: Vec<Located> =
            errors("dedupe_mode = Disabled, dedupe_mode = Full, dedupe_mode = Functional");
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_problem_is_its_own_error() {
        let expected: Vec<Located> = vec![
            (
                "unknown setting `colour`; expected `model_name_source`, `dedupe_mode`, \
                 `unknown_properties`, `unique_items`, `map_type`, `flatten_single_entry_combinators`, \
                 `self_validating`, `constrained_newtypes`, `format_types`, `extra_derives`, \
                 `extra_attrs`, `patch`, `convert`, `replace`, `crates`, `unknown_crates`, `disallow_unknown_fields`, `forced_spec_version`, `default_spec_version`, `format_mode`, `content_mode`, `unknown_formats`, \
                 `integer_semantics`, `exclusive_bounds`, `ref_siblings`, `unknown_schema_uri` or `metaschema_check`"
                    .to_string(),
                1,
                0,
            ),
            (
                "unknown value `Draft08` for `forced_spec_version`; expected `Draft00`, \
                 `Draft01`, `Draft02`, `Draft03`, `Draft04`, `Draft05`, `Draft06`, `Draft07`, \
                 `Draft201909` or `Draft202012`"
                    .to_string(),
                2,
                22,
            ),
            (
                "`constrained_newtypes` takes `true` or `false`, not `yes`".to_string(),
                3,
                23,
            ),
        ];
        let actual: Vec<Located> =
            errors("colour = Blue,\nforced_spec_version = Draft08,\nconstrained_newtypes = yes");
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(feature = "indexmap")]
    fn index_set_with_feature_is_index_set() {
        let expected: UniqueItemsType = UniqueItemsType::IndexSet;
        let actual: UniqueItemsType = resolve_source("unique_items = IndexSet")
            .expect("resolve")
            .code_gen
            .unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    #[cfg(not(feature = "indexmap"))]
    fn index_set_without_feature_is_an_error_naming_the_feature() {
        let expected: Vec<Located> = vec![(
            "`unique_items = IndexSet` needs the `indexmap` feature of `json-schema-rs-macro`"
                .to_string(),
            1,
            15,
        )];
        let actual: Vec<Located> = errors("unique_items = IndexSet");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_list_for_extra_derives_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "`extra_derives` takes a list `[..]`, not `Hash`".to_string(),
            1,
            16,
        )];
        let actual: Vec<Located> = errors("extra_derives = Hash");
        assert_eq!(expected, actual);
    }

    #[test]
    fn derive_that_is_not_a_path_is_an_error_at_it() {
        let expected: Vec<Located> = vec![(
            "`extra_derives` takes derive macro paths".to_string(),
            1,
            23,
        )];
        let actual: Vec<Located> = errors("extra_derives = [Hash, serde(x)]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_group_for_patch_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![("`patch` takes `{..}`, not `[..]`".to_string(), 1, 8)];
        let actual: Vec<Located> = errors("patch = []");
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_field_errors_are_collected_at_each_field() {
        let expected: Vec<Located> = vec![
            (
                "`rename` takes a string, not `Vegetable`".to_string(),
                1,
                27,
            ),
            (
                "unknown patch field `derive`; expected `rename`, `derives` or `attrs`".to_string(),
                1,
                38,
            ),
            (
                "the patch for `Fruit` takes `{..}`, not `true`".to_string(),
                1,
                65,
            ),
        ];
        let actual: Vec<Located> =
            errors("patch = { Veg = { rename = Vegetable, derive = [Hash] }, Fruit = true }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_patched_twice_is_an_error_at_its_second_name() {
        let expected: Vec<Located> =
            vec![("type `Veg` is patched more than once".to_string(), 1, 20)];
        let actual: Vec<Located> = errors("patch = { Veg = {}, Veg = {} }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn convert_and_replace_reach_their_fields() {
        let expected: (Vec<TypeConversion>, BTreeMap<String, String>) = (
            vec![
                TypeConversion {
                    schema: serde_json::json!({"type": "string", "format": "decimal"}),
                    rust_type: "rust_decimal::Decimal".to_string(),
                },
                TypeConversion {
                    schema: serde_json::json!({"type": "integer", "minimum": -1}),
                    rust_type: "&'static my::Small<i8>".to_string(),
                },
            ],
            BTreeMap::from([("Ipv4".to_string(), "my::Ipv4".to_string())]),
        );
        let settings: MacroSettings = resolve_source(
            r#"convert = { {"type":"string","format":"decimal"} => rust_decimal::Decimal, "{\"type\":\"integer\",\"minimum\":-1}" => &'static my::Small<i8> }, replace = { Ipv4 = my::Ipv4 }"#,
        )
        .expect("resolve");
        let actual: (Vec<TypeConversion>, BTreeMap<String, String>) = (
            settings.code_gen.conversions,
            settings.code_gen.replacements,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_schema_that_is_not_json_is_an_error_at_it() {
        let expected: (String, usize, usize) = (
            "conversion schema is not JSON: key must be a string at line 1 column 2".to_string(),
            1,
            12,
        );
        let tokens: TokenStream =
            TokenStream::from_str("convert = { {type: 1} => my::T }").expect("tokenize");
        let Err(error) = syn::parse2::<SettingArg>(tokens) else {
            panic!("a schema that is not JSON parsed");
        };
        let actual: (String, usize, usize) = (
            error.to_string(),
            error.span().start().line,
            error.span().start().column,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_group_for_convert_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "`convert` takes `{ schema => path, .. }`, not `[..]`".to_string(),
            1,
            10,
        )];
        let actual: Vec<Located> = errors("convert = []");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_group_for_replace_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "`replace` takes `{ Name = path, .. }`, not `true`".to_string(),
            1,
            10,
        )];
        let actual: Vec<Located> = errors("replace = true");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_replaced_twice_is_an_error_at_its_second_name() {
        let expected: Vec<Located> =
            vec![("type `Veg` is replaced more than once".to_string(), 1, 24)];
        let actual: Vec<Located> = errors("replace = { Veg = a::B, Veg = c::D }");
        assert_eq!(expected, actual);
    }

    #[test]
    fn crates_and_unknown_crates_reach_their_fields() {
        let expected: (BTreeMap<String, CrateSpec>, UnknownCrates) = (
            BTreeMap::from([
                (
                    "my-types".to_string(),
                    CrateSpec {
                        version: "1.2.0".parse::<CrateVersion>().expect("version"),
                        rename: Some("types".to_string()),
                    },
                ),
                (
                    "other".to_string(),
                    CrateSpec {
                        version: CrateVersion::Any,
                        rename: None,
                    },
                ),
            ]),
            UnknownCrates::Deny,
        );
        let settings: MacroSettings =
            resolve_source(r#"crates = ["types=my-types@1.2", "other@*"], unknown_crates = Deny"#)
                .expect("resolve");
        let actual: (BTreeMap<String, CrateSpec>, UnknownCrates) =
            (settings.code_gen.crates, settings.code_gen.unknown_crates);
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_list_for_crates_is_an_error_at_the_value() {
        let expected: Vec<Located> = vec![(
            "`crates` takes `[\"[rename=]crate@version\", ..]`, not `true`".to_string(),
            1,
            9,
        )];
        let actual: Vec<Located> = errors("crates = true");
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_that_does_not_parse_is_an_error_at_it() {
        let expected: Vec<Located> = vec![(
            "crate `my-types` is not of the form `[rename=]crate@version`".to_string(),
            1,
            10,
        )];
        let actual: Vec<Located> = errors(r#"crates = ["my-types"]"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_listed_twice_is_an_error_at_its_second_spec() {
        let expected: Vec<Located> =
            vec![("crate `a` is listed more than once".to_string(), 1, 17)];
        let actual: Vec<Located> = errors(r#"crates = ["a@1", "b=a@2"]"#);
        assert_eq!(expected, actual);
    }
}
