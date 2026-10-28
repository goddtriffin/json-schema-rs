//! Per-draft dialects: the keywords each JSON Schema draft defines and the behaviour that differs
//! between drafts.
//!
//! A [`Dialect`] is the effective rule set for one schema document: the base draft's defaults
//! (see [`Dialect::for_spec_version`]) with any overrides from
//! [`JsonSchemaSettings`](super::JsonSchemaSettings) applied. Draft-dependent code reads the
//! dialect instead of branching on [`SpecVersion`].

use super::spec_version::SpecVersion;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

/// How `$ref` treats the other keywords in the same schema object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefSiblings {
    /// Keywords next to `$ref` are ignored (drafts 4, 6 and 7).
    Ignored,
    /// Keywords next to `$ref` are evaluated alongside it (2019-09 and 2020-12).
    Evaluated,
}

/// Which JSON numbers `"type": "integer"` accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegerSemantics {
    /// Only numbers written without a fraction or exponent part (draft 4): `1.0` is not an integer.
    Lexical,
    /// Any number with a zero fractional part (draft 6 and later): `1.0` is an integer.
    Numeric,
}

/// Whether the `format` keyword is checked or only recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatMode {
    /// `format` is an annotation and never fails validation (2019-09 and 2020-12 default).
    Annotation,
    /// Every format this library implements is checked (drafts 4, 6 and 7 default).
    Assertion,
}

/// The shape of `exclusiveMinimum` / `exclusiveMaximum`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExclusiveBounds {
    /// A boolean that makes `minimum` / `maximum` exclusive (draft 4).
    BooleanModifier,
    /// A number that is itself the exclusive bound (draft 6 and later).
    Numeric,
}

/// Where a keyword's value holds subschemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubschemaPosition {
    /// The value is one schema (`not`, `additionalProperties`).
    Single,
    /// The value is an array of schemas (`allOf`, `prefixItems`).
    Array,
    /// The value is an object whose values are schemas (`properties`, `$defs`). Values that are
    /// not schemas (the string arrays of draft-4 `dependencies`) are skipped.
    Map,
    /// The value is one schema or an array of schemas (`items` before 2020-12).
    SingleOrArray,
}

/// A keyword a draft defines, according to that draft's metaschema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keyword {
    /// The keyword as written in a schema (e.g. `"properties"`).
    pub name: &'static str,
    /// Where its value holds subschemas, or `None` when it holds none.
    pub subschemas: Option<SubschemaPosition>,
}

/// A vocabulary a draft's metaschema declares in `$vocabulary` (2019-09 and later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Vocabulary {
    /// The vocabulary URI.
    pub uri: &'static str,
    /// `true` when the metaschema marks it required (an implementation that does not know it
    /// must refuse the schema); `false` when it is optional.
    pub required: bool,
}

/// The effective JSON Schema rules for one schema document.
///
/// Built from a draft's defaults ([`Dialect::for_spec_version`]) with any overrides from
/// [`JsonSchemaSettings`](super::JsonSchemaSettings) applied. Read a parsed schema's dialect with
/// [`JsonSchema::dialect`](super::JsonSchema::dialect).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Dialect {
    /// The draft whose keyword table applies. Drafts before 4 use draft 4's rules (see
    /// [`DialectWarning::UnsupportedDraft`]); draft 5 is draft 4 with editorial changes only.
    pub spec_version: SpecVersion,
    /// How `$ref` treats sibling keywords.
    pub ref_siblings: RefSiblings,
    /// Which numbers `"type": "integer"` accepts.
    pub integer_semantics: IntegerSemantics,
    /// Whether `format` is checked.
    pub format_mode: FormatMode,
    /// The shape of `exclusiveMinimum` / `exclusiveMaximum`.
    pub exclusive_bounds: ExclusiveBounds,
}

/// The drafts that have their own keyword table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Draft {
    Draft4,
    Draft6,
    Draft7,
    Draft201909,
    Draft202012,
}

impl Draft {
    fn of(spec_version: SpecVersion) -> Draft {
        match spec_version {
            SpecVersion::Draft00
            | SpecVersion::Draft01
            | SpecVersion::Draft02
            | SpecVersion::Draft03
            | SpecVersion::Draft04
            | SpecVersion::Draft05 => Draft::Draft4,
            SpecVersion::Draft06 => Draft::Draft6,
            SpecVersion::Draft07 => Draft::Draft7,
            SpecVersion::Draft201909 => Draft::Draft201909,
            SpecVersion::Draft202012 => Draft::Draft202012,
        }
    }
}

/// Draft 4's keywords: its metaschema's properties plus `$ref` (defined by JSON Reference).
const DRAFT4_KEYWORDS: &[Keyword] = &[
    kw("$ref"),
    kw("$schema"),
    sub("additionalItems", SubschemaPosition::Single),
    sub("additionalProperties", SubschemaPosition::Single),
    sub("allOf", SubschemaPosition::Array),
    sub("anyOf", SubschemaPosition::Array),
    kw("default"),
    sub("definitions", SubschemaPosition::Map),
    sub("dependencies", SubschemaPosition::Map),
    kw("description"),
    kw("enum"),
    kw("exclusiveMaximum"),
    kw("exclusiveMinimum"),
    kw("format"),
    kw("id"),
    sub("items", SubschemaPosition::SingleOrArray),
    kw("maxItems"),
    kw("maxLength"),
    kw("maxProperties"),
    kw("maximum"),
    kw("minItems"),
    kw("minLength"),
    kw("minProperties"),
    kw("minimum"),
    kw("multipleOf"),
    sub("not", SubschemaPosition::Single),
    sub("oneOf", SubschemaPosition::Array),
    kw("pattern"),
    sub("patternProperties", SubschemaPosition::Map),
    sub("properties", SubschemaPosition::Map),
    kw("required"),
    kw("title"),
    kw("type"),
    kw("uniqueItems"),
];

/// Draft 6: `id` becomes `$id`; adds `const`, `contains`, `examples`, `propertyNames`.
const DRAFT6_REMOVED: &[&str] = &["id"];
const DRAFT6_ADDED: &[Keyword] = &[
    kw("$id"),
    kw("const"),
    sub("contains", SubschemaPosition::Single),
    kw("examples"),
    sub("propertyNames", SubschemaPosition::Single),
];

/// Draft 7: adds `$comment`, `if` / `then` / `else`, `readOnly`, `writeOnly`, content keywords.
const DRAFT7_ADDED: &[Keyword] = &[
    kw("$comment"),
    kw("contentEncoding"),
    kw("contentMediaType"),
    sub("else", SubschemaPosition::Single),
    sub("if", SubschemaPosition::Single),
    kw("readOnly"),
    sub("then", SubschemaPosition::Single),
    kw("writeOnly"),
];

/// 2019-09: vocabularies, `$defs`, anchors, recursive references, `dependent*`, `unevaluated*`,
/// `min/maxContains`, `deprecated`, `contentSchema`. The metaschema keeps `definitions` and
/// `dependencies` for compatibility.
const DRAFT201909_ADDED: &[Keyword] = &[
    kw("$anchor"),
    sub("$defs", SubschemaPosition::Map),
    kw("$recursiveAnchor"),
    kw("$recursiveRef"),
    kw("$vocabulary"),
    sub("contentSchema", SubschemaPosition::Single),
    kw("dependentRequired"),
    sub("dependentSchemas", SubschemaPosition::Map),
    kw("deprecated"),
    kw("maxContains"),
    kw("minContains"),
    sub("unevaluatedItems", SubschemaPosition::Single),
    sub("unevaluatedProperties", SubschemaPosition::Single),
];

/// 2020-12: `prefixItems` replaces the array form of `items` and `additionalItems`; dynamic
/// references. The metaschema keeps `$recursiveAnchor` / `$recursiveRef` for compatibility.
const DRAFT202012_REMOVED: &[&str] = &["additionalItems"];
const DRAFT202012_ADDED: &[Keyword] = &[
    kw("$dynamicAnchor"),
    kw("$dynamicRef"),
    sub("items", SubschemaPosition::Single),
    sub("prefixItems", SubschemaPosition::Array),
];

const DRAFT201909_VOCABULARIES: &[Vocabulary] = &[
    vocab("https://json-schema.org/draft/2019-09/vocab/core", true),
    vocab(
        "https://json-schema.org/draft/2019-09/vocab/applicator",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2019-09/vocab/validation",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2019-09/vocab/meta-data",
        true,
    ),
    vocab("https://json-schema.org/draft/2019-09/vocab/format", false),
    vocab("https://json-schema.org/draft/2019-09/vocab/content", true),
];

const DRAFT202012_VOCABULARIES: &[Vocabulary] = &[
    vocab("https://json-schema.org/draft/2020-12/vocab/core", true),
    vocab(
        "https://json-schema.org/draft/2020-12/vocab/applicator",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2020-12/vocab/unevaluated",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2020-12/vocab/validation",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2020-12/vocab/meta-data",
        true,
    ),
    vocab(
        "https://json-schema.org/draft/2020-12/vocab/format-annotation",
        true,
    ),
    vocab("https://json-schema.org/draft/2020-12/vocab/content", true),
];

const fn kw(name: &'static str) -> Keyword {
    Keyword {
        name,
        subschemas: None,
    }
}

const fn sub(name: &'static str, position: SubschemaPosition) -> Keyword {
    Keyword {
        name,
        subschemas: Some(position),
    }
}

const fn vocab(uri: &'static str, required: bool) -> Vocabulary {
    Vocabulary { uri, required }
}

/// Builds a draft's keyword table from the previous draft's: drops `removed`, then adds `added`
/// (an added keyword replaces a same-named one). The result is sorted by name.
fn derive_keywords(base: &[Keyword], removed: &[&'static str], added: &[Keyword]) -> Vec<Keyword> {
    let mut by_name: BTreeMap<&'static str, Keyword> = base.iter().map(|k| (k.name, *k)).collect();
    for name in removed {
        by_name.remove(name);
    }
    for keyword in added {
        by_name.insert(keyword.name, *keyword);
    }
    by_name.into_values().collect()
}

static DRAFT4_TABLE: LazyLock<Vec<Keyword>> =
    LazyLock::new(|| derive_keywords(DRAFT4_KEYWORDS, &[], &[]));
static DRAFT6_TABLE: LazyLock<Vec<Keyword>> =
    LazyLock::new(|| derive_keywords(&DRAFT4_TABLE, DRAFT6_REMOVED, DRAFT6_ADDED));
static DRAFT7_TABLE: LazyLock<Vec<Keyword>> =
    LazyLock::new(|| derive_keywords(&DRAFT6_TABLE, &[], DRAFT7_ADDED));
static DRAFT201909_TABLE: LazyLock<Vec<Keyword>> =
    LazyLock::new(|| derive_keywords(&DRAFT7_TABLE, &[], DRAFT201909_ADDED));
static DRAFT202012_TABLE: LazyLock<Vec<Keyword>> =
    LazyLock::new(|| derive_keywords(&DRAFT201909_TABLE, DRAFT202012_REMOVED, DRAFT202012_ADDED));

impl Dialect {
    /// The default rules of `spec_version`, as its specification defines them.
    ///
    /// Drafts 00–03 have no table of their own and use draft 4's rules; draft 5 is draft 4.
    #[must_use]
    pub fn for_spec_version(spec_version: SpecVersion) -> Dialect {
        let (ref_siblings, integer_semantics, format_mode, exclusive_bounds) =
            match Draft::of(spec_version) {
                Draft::Draft4 => (
                    RefSiblings::Ignored,
                    IntegerSemantics::Lexical,
                    FormatMode::Assertion,
                    ExclusiveBounds::BooleanModifier,
                ),
                Draft::Draft6 | Draft::Draft7 => (
                    RefSiblings::Ignored,
                    IntegerSemantics::Numeric,
                    FormatMode::Assertion,
                    ExclusiveBounds::Numeric,
                ),
                Draft::Draft201909 | Draft::Draft202012 => (
                    RefSiblings::Evaluated,
                    IntegerSemantics::Numeric,
                    FormatMode::Annotation,
                    ExclusiveBounds::Numeric,
                ),
            };
        Dialect {
            spec_version,
            ref_siblings,
            integer_semantics,
            format_mode,
            exclusive_bounds,
        }
    }

    /// The keyword that sets a schema resource's URI: `"id"` in draft 4, `"$id"` from draft 6.
    #[must_use]
    pub fn id_keyword(&self) -> &'static str {
        match Draft::of(self.spec_version) {
            Draft::Draft4 => "id",
            Draft::Draft6 | Draft::Draft7 | Draft::Draft201909 | Draft::Draft202012 => "$id",
        }
    }

    /// Every keyword this dialect's metaschema defines, sorted by name.
    #[must_use]
    pub fn keywords(&self) -> &'static [Keyword] {
        let table: &'static LazyLock<Vec<Keyword>> = match Draft::of(self.spec_version) {
            Draft::Draft4 => &DRAFT4_TABLE,
            Draft::Draft6 => &DRAFT6_TABLE,
            Draft::Draft7 => &DRAFT7_TABLE,
            Draft::Draft201909 => &DRAFT201909_TABLE,
            Draft::Draft202012 => &DRAFT202012_TABLE,
        };
        table.as_slice()
    }

    /// The keyword named `name`, when this dialect defines it.
    #[must_use]
    pub fn keyword(&self, name: &str) -> Option<&'static Keyword> {
        let keywords: &'static [Keyword] = self.keywords();
        keywords
            .binary_search_by(|k| k.name.cmp(name))
            .ok()
            .map(|i| &keywords[i])
    }

    /// The vocabularies the draft's metaschema declares (empty before 2019-09).
    #[must_use]
    pub fn vocabularies(&self) -> &'static [Vocabulary] {
        match Draft::of(self.spec_version) {
            Draft::Draft4 | Draft::Draft6 | Draft::Draft7 => &[],
            Draft::Draft201909 => DRAFT201909_VOCABULARIES,
            Draft::Draft202012 => DRAFT202012_VOCABULARIES,
        }
    }
}

impl Default for Dialect {
    /// Draft 2020-12, the latest supported draft.
    fn default() -> Self {
        Dialect::for_spec_version(SpecVersion::Draft202012)
    }
}

/// Something about a schema's dialect that the caller should know, even though parsing
/// succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialectWarning {
    /// `$schema` names a metaschema this library does not recognise; the schema uses the
    /// default dialect (Draft 2020-12) instead.
    UnknownSchemaUri {
        /// The unrecognised `$schema` value.
        uri: String,
    },
    /// The draft has no rule set of its own (drafts 00–03); draft 4's rules are used.
    UnsupportedDraft {
        /// The requested draft.
        spec_version: SpecVersion,
    },
}

impl fmt::Display for DialectWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DialectWarning::UnknownSchemaUri { uri } => write!(
                f,
                "unknown $schema \"{uri}\"; using JSON Schema Draft 2020-12 rules"
            ),
            DialectWarning::UnsupportedDraft { spec_version } => write!(
                f,
                "{spec_version:?} has no rule set of its own; using JSON Schema Draft 4 rules"
            ),
        }
    }
}

/// A schema's effective [`Dialect`] together with any [`DialectWarning`]s from choosing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDialect {
    /// The effective rules.
    pub dialect: Dialect,
    /// Warnings raised while choosing the rules, in the order they were found.
    pub warnings: Vec<DialectWarning>,
}

#[cfg(test)]
mod tests {
    use super::{
        Dialect, ExclusiveBounds, FormatMode, IntegerSemantics, Keyword, RefSiblings,
        SubschemaPosition, Vocabulary,
    };
    use crate::json_schema::SpecVersion;

    fn names(dialect: Dialect) -> Vec<&'static str> {
        dialect.keywords().iter().map(|k| k.name).collect()
    }

    #[test]
    fn draft4_dialect_flags() {
        let expected: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            RefSiblings::Ignored,
            IntegerSemantics::Lexical,
            FormatMode::Assertion,
            ExclusiveBounds::BooleanModifier,
        );
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft04);
        let actual: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            d.ref_siblings,
            d.integer_semantics,
            d.format_mode,
            d.exclusive_bounds,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_dialect_flags() {
        let expected: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            RefSiblings::Ignored,
            IntegerSemantics::Numeric,
            FormatMode::Assertion,
            ExclusiveBounds::Numeric,
        );
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft06);
        let actual: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            d.ref_siblings,
            d.integer_semantics,
            d.format_mode,
            d.exclusive_bounds,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_dialect_flags() {
        let expected: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            RefSiblings::Ignored,
            IntegerSemantics::Numeric,
            FormatMode::Assertion,
            ExclusiveBounds::Numeric,
        );
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft07);
        let actual: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            d.ref_siblings,
            d.integer_semantics,
            d.format_mode,
            d.exclusive_bounds,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft201909_dialect_flags() {
        let expected: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            RefSiblings::Evaluated,
            IntegerSemantics::Numeric,
            FormatMode::Annotation,
            ExclusiveBounds::Numeric,
        );
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft201909);
        let actual: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            d.ref_siblings,
            d.integer_semantics,
            d.format_mode,
            d.exclusive_bounds,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft202012_dialect_flags() {
        let expected: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            RefSiblings::Evaluated,
            IntegerSemantics::Numeric,
            FormatMode::Annotation,
            ExclusiveBounds::Numeric,
        );
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
        let actual: (RefSiblings, IntegerSemantics, FormatMode, ExclusiveBounds) = (
            d.ref_siblings,
            d.integer_semantics,
            d.format_mode,
            d.exclusive_bounds,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_dialect_is_draft202012() {
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
        let actual: Dialect = Dialect::default();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft05_uses_draft4_rules() {
        let expected: (Vec<&'static str>, &'static str) =
            (names(Dialect::for_spec_version(SpecVersion::Draft04)), "id");
        let d: Dialect = Dialect::for_spec_version(SpecVersion::Draft05);
        let actual: (Vec<&'static str>, &'static str) = (names(d), d.id_keyword());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft03_uses_draft4_rules() {
        let expected: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft04));
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft03));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_keyword_is_id() {
        let expected: &str = "id";
        let actual: &str = Dialect::for_spec_version(SpecVersion::Draft04).id_keyword();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_id_keyword_is_dollar_id() {
        let expected: &str = "$id";
        let actual: &str = Dialect::for_spec_version(SpecVersion::Draft06).id_keyword();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_keywords() {
        let expected: Vec<&'static str> = vec![
            "$ref",
            "$schema",
            "additionalItems",
            "additionalProperties",
            "allOf",
            "anyOf",
            "default",
            "definitions",
            "dependencies",
            "description",
            "enum",
            "exclusiveMaximum",
            "exclusiveMinimum",
            "format",
            "id",
            "items",
            "maxItems",
            "maxLength",
            "maxProperties",
            "maximum",
            "minItems",
            "minLength",
            "minProperties",
            "minimum",
            "multipleOf",
            "not",
            "oneOf",
            "pattern",
            "patternProperties",
            "properties",
            "required",
            "title",
            "type",
            "uniqueItems",
        ];
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft04));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_keywords() {
        let expected: Vec<&'static str> = vec![
            "$id",
            "$ref",
            "$schema",
            "additionalItems",
            "additionalProperties",
            "allOf",
            "anyOf",
            "const",
            "contains",
            "default",
            "definitions",
            "dependencies",
            "description",
            "enum",
            "examples",
            "exclusiveMaximum",
            "exclusiveMinimum",
            "format",
            "items",
            "maxItems",
            "maxLength",
            "maxProperties",
            "maximum",
            "minItems",
            "minLength",
            "minProperties",
            "minimum",
            "multipleOf",
            "not",
            "oneOf",
            "pattern",
            "patternProperties",
            "properties",
            "propertyNames",
            "required",
            "title",
            "type",
            "uniqueItems",
        ];
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft06));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_keywords() {
        let expected: Vec<&'static str> = vec![
            "$comment",
            "$id",
            "$ref",
            "$schema",
            "additionalItems",
            "additionalProperties",
            "allOf",
            "anyOf",
            "const",
            "contains",
            "contentEncoding",
            "contentMediaType",
            "default",
            "definitions",
            "dependencies",
            "description",
            "else",
            "enum",
            "examples",
            "exclusiveMaximum",
            "exclusiveMinimum",
            "format",
            "if",
            "items",
            "maxItems",
            "maxLength",
            "maxProperties",
            "maximum",
            "minItems",
            "minLength",
            "minProperties",
            "minimum",
            "multipleOf",
            "not",
            "oneOf",
            "pattern",
            "patternProperties",
            "properties",
            "propertyNames",
            "readOnly",
            "required",
            "then",
            "title",
            "type",
            "uniqueItems",
            "writeOnly",
        ];
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft07));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft201909_keywords() {
        let expected: Vec<&'static str> = vec![
            "$anchor",
            "$comment",
            "$defs",
            "$id",
            "$recursiveAnchor",
            "$recursiveRef",
            "$ref",
            "$schema",
            "$vocabulary",
            "additionalItems",
            "additionalProperties",
            "allOf",
            "anyOf",
            "const",
            "contains",
            "contentEncoding",
            "contentMediaType",
            "contentSchema",
            "default",
            "definitions",
            "dependencies",
            "dependentRequired",
            "dependentSchemas",
            "deprecated",
            "description",
            "else",
            "enum",
            "examples",
            "exclusiveMaximum",
            "exclusiveMinimum",
            "format",
            "if",
            "items",
            "maxContains",
            "maxItems",
            "maxLength",
            "maxProperties",
            "maximum",
            "minContains",
            "minItems",
            "minLength",
            "minProperties",
            "minimum",
            "multipleOf",
            "not",
            "oneOf",
            "pattern",
            "patternProperties",
            "properties",
            "propertyNames",
            "readOnly",
            "required",
            "then",
            "title",
            "type",
            "unevaluatedItems",
            "unevaluatedProperties",
            "uniqueItems",
            "writeOnly",
        ];
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft201909));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft202012_keywords() {
        let expected: Vec<&'static str> = vec![
            "$anchor",
            "$comment",
            "$defs",
            "$dynamicAnchor",
            "$dynamicRef",
            "$id",
            "$recursiveAnchor",
            "$recursiveRef",
            "$ref",
            "$schema",
            "$vocabulary",
            "additionalProperties",
            "allOf",
            "anyOf",
            "const",
            "contains",
            "contentEncoding",
            "contentMediaType",
            "contentSchema",
            "default",
            "definitions",
            "dependencies",
            "dependentRequired",
            "dependentSchemas",
            "deprecated",
            "description",
            "else",
            "enum",
            "examples",
            "exclusiveMaximum",
            "exclusiveMinimum",
            "format",
            "if",
            "items",
            "maxContains",
            "maxItems",
            "maxLength",
            "maxProperties",
            "maximum",
            "minContains",
            "minItems",
            "minLength",
            "minProperties",
            "minimum",
            "multipleOf",
            "not",
            "oneOf",
            "pattern",
            "patternProperties",
            "prefixItems",
            "properties",
            "propertyNames",
            "readOnly",
            "required",
            "then",
            "title",
            "type",
            "unevaluatedItems",
            "unevaluatedProperties",
            "uniqueItems",
            "writeOnly",
        ];
        let actual: Vec<&'static str> = names(Dialect::for_spec_version(SpecVersion::Draft202012));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft201909_items_holds_schema_or_array() {
        let expected: Option<&Keyword> = Some(&Keyword {
            name: "items",
            subschemas: Some(SubschemaPosition::SingleOrArray),
        });
        let actual: Option<&Keyword> =
            Dialect::for_spec_version(SpecVersion::Draft201909).keyword("items");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft202012_items_holds_one_schema() {
        let expected: Option<&Keyword> = Some(&Keyword {
            name: "items",
            subschemas: Some(SubschemaPosition::Single),
        });
        let actual: Option<&Keyword> =
            Dialect::for_spec_version(SpecVersion::Draft202012).keyword("items");
        assert_eq!(expected, actual);
    }

    #[test]
    fn keyword_lookup_misses_unknown_keyword() {
        let expected: Option<&Keyword> = None;
        let actual: Option<&Keyword> = Dialect::default().keyword("notAKeyword");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_has_no_vocabularies() {
        let expected: &[Vocabulary] = &[];
        let actual: &[Vocabulary] = Dialect::for_spec_version(SpecVersion::Draft07).vocabularies();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft201909_vocabularies_match_metaschema() {
        let expected: Vec<(&str, bool)> = vec![
            ("https://json-schema.org/draft/2019-09/vocab/core", true),
            (
                "https://json-schema.org/draft/2019-09/vocab/applicator",
                true,
            ),
            (
                "https://json-schema.org/draft/2019-09/vocab/validation",
                true,
            ),
            (
                "https://json-schema.org/draft/2019-09/vocab/meta-data",
                true,
            ),
            ("https://json-schema.org/draft/2019-09/vocab/format", false),
            ("https://json-schema.org/draft/2019-09/vocab/content", true),
        ];
        let actual: Vec<(&str, bool)> = Dialect::for_spec_version(SpecVersion::Draft201909)
            .vocabularies()
            .iter()
            .map(|v| (v.uri, v.required))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft202012_vocabularies_match_metaschema() {
        let expected: Vec<(&str, bool)> = vec![
            ("https://json-schema.org/draft/2020-12/vocab/core", true),
            (
                "https://json-schema.org/draft/2020-12/vocab/applicator",
                true,
            ),
            (
                "https://json-schema.org/draft/2020-12/vocab/unevaluated",
                true,
            ),
            (
                "https://json-schema.org/draft/2020-12/vocab/validation",
                true,
            ),
            (
                "https://json-schema.org/draft/2020-12/vocab/meta-data",
                true,
            ),
            (
                "https://json-schema.org/draft/2020-12/vocab/format-annotation",
                true,
            ),
            ("https://json-schema.org/draft/2020-12/vocab/content", true),
        ];
        let actual: Vec<(&str, bool)> = Dialect::for_spec_version(SpecVersion::Draft202012)
            .vocabularies()
            .iter()
            .map(|v| (v.uri, v.required))
            .collect();
        assert_eq!(expected, actual);
    }
}
