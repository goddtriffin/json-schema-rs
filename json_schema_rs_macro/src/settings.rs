//! Settings arguments of `json_schema_to_rust!`: `name = value` after the schema literals, one
//! per [`CodeGenSettings`] field and [`JsonSchemaSettings`] field, named like the builder
//! methods. A value is `true` / `false` for a flag and the variant name for an enum
//! (`dedupe_mode = Disabled`, `forced_spec_version = Draft07`). A setting left out keeps its
//! builder default.

use json_schema_rs::{
    CodeGenSettings, DedupeMode, ExclusiveBounds, FormatMode, FormatTypes, IntegerSemantics,
    JsonSchemaSettings, ModelNameSource, RefSiblings, SpecVersion, UniqueItemsType,
    UnknownProperties, UnknownSchemaUri,
};
use proc_macro2::Span;
use std::collections::BTreeSet;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitBool, Result as SynResult, Token};

/// One `name = value` argument, as written.
pub(crate) struct SettingArg {
    pub(crate) name: Ident,
    pub(crate) value: SettingValue,
}

/// The value of a [`SettingArg`]: a boolean literal or an identifier (an enum variant name).
pub(crate) enum SettingValue {
    Bool(LitBool),
    Ident(Ident),
}

impl SettingValue {
    fn span(&self) -> Span {
        match self {
            SettingValue::Bool(lit) => lit.span(),
            SettingValue::Ident(ident) => ident.span(),
        }
    }
}

impl Parse for SettingArg {
    fn parse(input: ParseStream<'_>) -> SynResult<Self> {
        let name: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value: SettingValue = if input.peek(LitBool) {
            SettingValue::Bool(input.parse()?)
        } else if input.peek(Ident) {
            SettingValue::Ident(input.parse()?)
        } else {
            return Err(input.error("expected a setting value: `true`, `false` or a variant name"));
        };
        Ok(SettingArg { name, value })
    }
}

/// Every setting name, in the order the error for an unknown name lists them.
const SETTING_NAMES: [&str; 16] = [
    "model_name_source",
    "dedupe_mode",
    "unknown_properties",
    "unique_items",
    "flatten_single_entry_combinators",
    "self_validating",
    "constrained_newtypes",
    "format_types",
    "disallow_unknown_fields",
    "forced_spec_version",
    "default_spec_version",
    "format_mode",
    "integer_semantics",
    "exclusive_bounds",
    "ref_siblings",
    "unknown_schema_uri",
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

/// The variant names of `unique_items`, whether or not this build has `IndexSet`.
const UNIQUE_ITEMS_NAMES: &[&str] = &["Vec", "HashSet", "BTreeSet", "IndexSet"];

/// The settings the arguments set; `None` keeps the builder default.
#[derive(Default)]
struct Overrides {
    model_name_source: Option<ModelNameSource>,
    dedupe_mode: Option<DedupeMode>,
    unknown_properties: Option<UnknownProperties>,
    unique_items: Option<UniqueItemsType>,
    flatten_single_entry_combinators: Option<bool>,
    self_validating: Option<bool>,
    constrained_newtypes: Option<bool>,
    format_types: Option<FormatTypes>,
    disallow_unknown_fields: Option<bool>,
    forced_spec_version: Option<SpecVersion>,
    default_spec_version: Option<SpecVersion>,
    format_mode: Option<FormatMode>,
    integer_semantics: Option<IntegerSemantics>,
    exclusive_bounds: Option<ExclusiveBounds>,
    ref_siblings: Option<RefSiblings>,
    unknown_schema_uri: Option<UnknownSchemaUri>,
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
        SettingValue::Ident(ident) => Err(syn::Error::new(
            ident.span(),
            format!("`{setting}` takes `true` or `false`, not `{ident}`"),
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
        SettingValue::Bool(_) => None,
    };
    found.ok_or_else(|| unknown_value(setting, value, &names))
}

fn unknown_value(setting: &str, value: &SettingValue, names: &[&str]) -> syn::Error {
    let written: String = match value {
        SettingValue::Bool(lit) => lit.value.to_string(),
        SettingValue::Ident(ident) => ident.to_string(),
    };
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
        CodeGenSettings, DedupeMode, ExclusiveBounds, FormatMode, FormatTypes, IntegerSemantics,
        JsonSchemaSettings, ModelNameSource, RefSiblings, SpecVersion, UniqueItemsType,
        UnknownProperties, UnknownSchemaUri,
    };
    use proc_macro2::TokenStream;
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
                .flatten_single_entry_combinators(true)
                .self_validating(false)
                .constrained_newtypes(false)
                .format_types(FormatTypes::FollowDialect)
                .build(),
            json_schema: JsonSchemaSettings::builder()
                .disallow_unknown_fields(true)
                .forced_spec_version(SpecVersion::Draft07)
                .default_spec_version(SpecVersion::Draft04)
                .format_mode(FormatMode::Assertion)
                .integer_semantics(IntegerSemantics::Lexical)
                .exclusive_bounds(ExclusiveBounds::BooleanModifier)
                .ref_siblings(RefSiblings::Evaluated)
                .unknown_schema_uri(UnknownSchemaUri::Reject)
                .build(),
        };
        let actual: MacroSettings = resolve_source(
            "model_name_source = PropertyKeyFirst, dedupe_mode = Disabled, \
             unknown_properties = Drop, unique_items = BTreeSet, \
             flatten_single_entry_combinators = true, self_validating = false, \
             constrained_newtypes = false, format_types = FollowDialect, disallow_unknown_fields = true, \
             forced_spec_version = Draft07, default_spec_version = Draft04, \
             format_mode = Assertion, integer_semantics = Lexical, \
             exclusive_bounds = BooleanModifier, ref_siblings = Evaluated, \
             unknown_schema_uri = Reject",
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
                .integer_semantics(IntegerSemantics::Numeric)
                .exclusive_bounds(ExclusiveBounds::Numeric)
                .ref_siblings(RefSiblings::Ignored)
                .unknown_schema_uri(UnknownSchemaUri::Fallback)
                .build(),
        };
        let actual: MacroSettings = resolve_source(
            "model_name_source = TitleFirst, dedupe_mode = Functional, \
             unknown_properties = Capture, unique_items = HashSet, \
             flatten_single_entry_combinators = false, self_validating = true, \
             constrained_newtypes = true, format_types = Typed, disallow_unknown_fields = false, \
             forced_spec_version = Draft201909, default_spec_version = Draft06, \
             format_mode = Annotation, integer_semantics = Numeric, \
             exclusive_bounds = Numeric, ref_siblings = Ignored, \
             unknown_schema_uri = Fallback",
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
             `unknown_properties`, `unique_items`, `flatten_single_entry_combinators`, \
             `self_validating`, `constrained_newtypes`, `format_types`, `disallow_unknown_fields`, \
             `forced_spec_version`, `default_spec_version`, `format_mode`, \
             `integer_semantics`, `exclusive_bounds`, `ref_siblings` or `unknown_schema_uri`"
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
                 `unknown_properties`, `unique_items`, `flatten_single_entry_combinators`, \
                 `self_validating`, `constrained_newtypes`, `format_types`, `disallow_unknown_fields`, \
                 `forced_spec_version`, `default_spec_version`, `format_mode`, \
                 `integer_semantics`, `exclusive_bounds`, `ref_siblings` or `unknown_schema_uri`"
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
}
