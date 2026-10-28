//! JSON Schema ingestion settings.

use super::dialect::{
    Dialect, DialectWarning, ExclusiveBounds, FormatMode, IntegerSemantics, ResolvedDialect,
};
use super::error::{JsonSchemaParseError, JsonSchemaParseResult};
use super::json_schema::JsonSchema;
use super::spec_version::SpecVersion;

/// What to do when `$schema` names a metaschema this library does not recognise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnknownSchemaUri {
    /// Use the default dialect (Draft 2020-12) and report a
    /// [`DialectWarning::UnknownSchemaUri`].
    #[default]
    Fallback,
    /// Refuse the schema with [`JsonSchemaParseError::UnknownSchemaUri`].
    Reject,
}

/// Settings that affect how JSON Schema definitions are ingested (parsed).
///
/// [`Default`] follows each document's `$schema` (Draft 2020-12 when absent) with no overrides.
/// The draft decides behaviour that differs between drafts (see [`Dialect`]); the `Option`
/// settings below override one piece of it and are `None` (follow the draft) by default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonSchemaSettings {
    /// When `true` (strict), schema ingestion fails with
    /// [`JsonSchemaParseError::UnknownField`](super::JsonSchemaParseError::UnknownField) if any
    /// schema object has a key that the document's draft does not define as a keyword (its
    /// keyword table, [`Dialect::keywords`]): `$defs` is unknown in draft 7, `id` is known only in
    /// draft 4. Keywords the draft defines are accepted even when this library does not act on
    /// them. When `false` (the default), unknown keys are ignored.
    pub disallow_unknown_fields: bool,

    /// Explicit spec version to use. When `Some(v)`, that version is used and `$schema` is not used for draft selection.
    /// When `None`, the spec version is inferred from the root schema's `$schema` keyword; if absent or unrecognized, **Draft 2020-12** is used.
    pub spec_version: Option<SpecVersion>,

    /// Whether `format` is checked. `None` follows the draft: drafts 4, 6 and 7 check it,
    /// 2019-09 and 2020-12 treat it as an annotation.
    pub format_mode: Option<FormatMode>,

    /// Which numbers `"type": "integer"` accepts. `None` follows the draft: draft 4 is lexical
    /// (`1.0` is not an integer), draft 6 and later numeric (`1.0` is an integer).
    pub integer_semantics: Option<IntegerSemantics>,

    /// How `exclusiveMinimum` / `exclusiveMaximum` are read. `None` follows the draft: draft 4's
    /// boolean modifier of `minimum` / `maximum`, or draft 6+'s numeric bound.
    pub exclusive_bounds: Option<ExclusiveBounds>,

    /// What to do when `$schema` is not a recognised metaschema URI.
    pub unknown_schema_uri: UnknownSchemaUri,
}

/// Builder for [`JsonSchemaSettings`].
#[derive(Debug, Clone, Default)]
pub struct JsonSchemaSettingsBuilder {
    disallow_unknown_fields: Option<bool>,
    spec_version: Option<SpecVersion>,
    format_mode: Option<FormatMode>,
    integer_semantics: Option<IntegerSemantics>,
    exclusive_bounds: Option<ExclusiveBounds>,
    unknown_schema_uri: Option<UnknownSchemaUri>,
}

impl JsonSchemaSettingsBuilder {
    /// Set whether keys the document's draft does not define as keywords are rejected
    /// (see [`JsonSchemaSettings::disallow_unknown_fields`]).
    #[must_use]
    pub fn disallow_unknown_fields(mut self, value: bool) -> Self {
        self.disallow_unknown_fields = Some(value);
        self
    }

    /// Set the JSON Schema spec version explicitly. When set, parsing uses this version and does not infer from `$schema`.
    /// When not set (default), the version is inferred from the root schema's `$schema` with a default of Draft 2020-12.
    #[must_use]
    pub fn spec_version(mut self, value: SpecVersion) -> Self {
        self.spec_version = Some(value);
        self
    }

    /// Check `format` ([`FormatMode::Assertion`]) or only record it ([`FormatMode::Annotation`]),
    /// whatever the draft's default.
    #[must_use]
    pub fn format_mode(mut self, value: FormatMode) -> Self {
        self.format_mode = Some(value);
        self
    }

    /// Accept only lexical integers ([`IntegerSemantics::Lexical`]) or any number with a zero
    /// fractional part ([`IntegerSemantics::Numeric`]) for `"type": "integer"`, whatever the
    /// draft's default.
    #[must_use]
    pub fn integer_semantics(mut self, value: IntegerSemantics) -> Self {
        self.integer_semantics = Some(value);
        self
    }

    /// Read `exclusiveMinimum` / `exclusiveMaximum` as draft 4's boolean modifier
    /// ([`ExclusiveBounds::BooleanModifier`]) or as numeric bounds ([`ExclusiveBounds::Numeric`]),
    /// whatever the draft's default.
    #[must_use]
    pub fn exclusive_bounds(mut self, value: ExclusiveBounds) -> Self {
        self.exclusive_bounds = Some(value);
        self
    }

    /// Set what happens when `$schema` is not a recognised metaschema URI (default:
    /// [`UnknownSchemaUri::Fallback`]).
    #[must_use]
    pub fn unknown_schema_uri(mut self, value: UnknownSchemaUri) -> Self {
        self.unknown_schema_uri = Some(value);
        self
    }

    /// Build the settings. Any option not set uses its per-option default.
    #[must_use]
    pub fn build(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: self.disallow_unknown_fields.unwrap_or(false),
            spec_version: self.spec_version,
            format_mode: self.format_mode,
            integer_semantics: self.integer_semantics,
            exclusive_bounds: self.exclusive_bounds,
            unknown_schema_uri: self.unknown_schema_uri.unwrap_or_default(),
        }
    }
}

impl JsonSchemaSettings {
    /// Start a builder with all options unset (per-option defaults will be used on [`build`](JsonSchemaSettingsBuilder::build)).
    #[must_use]
    pub fn builder() -> JsonSchemaSettingsBuilder {
        JsonSchemaSettingsBuilder::default()
    }
}

/// Returns the effective [`Dialect`] for a root schema under `settings`, with any warnings.
///
/// The draft is chosen in this order: [`JsonSchemaSettings::spec_version`] when set; otherwise
/// the schema's `$schema` when it is a recognised metaschema URI; otherwise **Draft 2020-12**.
/// An unrecognised `$schema` falls back with a [`DialectWarning::UnknownSchemaUri`], or is an
/// error under [`UnknownSchemaUri::Reject`]. The settings' overrides are then applied to the
/// draft's defaults.
///
/// # Errors
///
/// Returns [`JsonSchemaParseError::UnknownSchemaUri`] when `$schema` is unrecognised, no spec
/// version is set, and [`JsonSchemaSettings::unknown_schema_uri`] is [`UnknownSchemaUri::Reject`].
pub fn resolve_dialect(
    schema: &JsonSchema,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<ResolvedDialect> {
    resolve_dialect_for_uri(schema.schema.as_deref(), settings)
}

/// [`resolve_dialect`] for a root schema whose `$schema` value is `schema_uri`.
pub(crate) fn resolve_dialect_for_uri(
    schema_uri: Option<&str>,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<ResolvedDialect> {
    let mut warnings: Vec<DialectWarning> = Vec::new();
    let spec_version: SpecVersion = match (settings.spec_version, schema_uri) {
        (Some(v), _) => v,
        (None, None) => SpecVersion::Draft202012,
        (None, Some(uri)) => match SpecVersion::from_schema_uri(uri) {
            Some(v) => v,
            None => match settings.unknown_schema_uri {
                UnknownSchemaUri::Reject => {
                    return Err(JsonSchemaParseError::UnknownSchemaUri {
                        uri: uri.to_string(),
                    });
                }
                UnknownSchemaUri::Fallback => {
                    warnings.push(DialectWarning::UnknownSchemaUri {
                        uri: uri.to_string(),
                    });
                    SpecVersion::Draft202012
                }
            },
        },
    };
    if matches!(
        spec_version,
        SpecVersion::Draft00 | SpecVersion::Draft01 | SpecVersion::Draft02 | SpecVersion::Draft03
    ) {
        warnings.push(DialectWarning::UnsupportedDraft { spec_version });
    }
    let mut dialect: Dialect = Dialect::for_spec_version(spec_version);
    if let Some(format_mode) = settings.format_mode {
        dialect.format_mode = format_mode;
    }
    if let Some(integer_semantics) = settings.integer_semantics {
        dialect.integer_semantics = integer_semantics;
    }
    if let Some(exclusive_bounds) = settings.exclusive_bounds {
        dialect.exclusive_bounds = exclusive_bounds;
    }
    Ok(ResolvedDialect { dialect, warnings })
}

#[cfg(test)]
mod tests {
    use super::{JsonSchemaSettings, UnknownSchemaUri, resolve_dialect};
    use crate::json_schema::SpecVersion;
    use crate::json_schema::dialect::{
        Dialect, DialectWarning, ExclusiveBounds, FormatMode, IntegerSemantics, ResolvedDialect,
    };
    use crate::json_schema::error::JsonSchemaParseError;
    use crate::json_schema::json_schema::JsonSchema;

    fn schema_with_uri(uri: &str) -> JsonSchema {
        JsonSchema {
            schema: Some(uri.to_string()),
            ..JsonSchema::default()
        }
    }

    #[test]
    fn builder_default_disallow_is_false() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        assert!(!settings.disallow_unknown_fields);
    }

    #[test]
    fn builder_disallow_unknown_fields() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        assert!(settings.disallow_unknown_fields);
    }

    #[test]
    fn builder_spec_version_explicit() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft07)
            .build();
        assert_eq!(settings.spec_version, Some(SpecVersion::Draft07));
    }

    #[test]
    fn builder_default_matches_default() {
        let expected: JsonSchemaSettings = JsonSchemaSettings::default();
        let actual: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_sets_every_option() {
        let expected: JsonSchemaSettings = JsonSchemaSettings {
            disallow_unknown_fields: true,
            spec_version: Some(SpecVersion::Draft06),
            format_mode: Some(FormatMode::Annotation),
            integer_semantics: Some(IntegerSemantics::Lexical),
            exclusive_bounds: Some(ExclusiveBounds::BooleanModifier),
            unknown_schema_uri: UnknownSchemaUri::Reject,
        };
        let actual: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .spec_version(SpecVersion::Draft06)
            .format_mode(FormatMode::Annotation)
            .integer_semantics(IntegerSemantics::Lexical)
            .exclusive_bounds(ExclusiveBounds::BooleanModifier)
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_uses_explicit_spec_version_over_schema_uri() {
        let schema: JsonSchema = schema_with_uri("https://json-schema.org/draft/2020-12/schema");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft04)
            .build();
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft04),
            warnings: vec![],
        };
        let actual: ResolvedDialect = resolve_dialect(&schema, &settings).expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_follows_schema_uri() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-07/schema#");
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft07),
            warnings: vec![],
        };
        let actual: ResolvedDialect =
            resolve_dialect(&schema, &JsonSchemaSettings::default()).expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_defaults_to_2020_12_without_schema_uri() {
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft202012),
            warnings: vec![],
        };
        let actual: ResolvedDialect =
            resolve_dialect(&JsonSchema::default(), &JsonSchemaSettings::default())
                .expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_unknown_schema_uri_falls_back_with_warning() {
        let schema: JsonSchema = schema_with_uri("https://example.com/custom-meta");
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft202012),
            warnings: vec![DialectWarning::UnknownSchemaUri {
                uri: "https://example.com/custom-meta".to_string(),
            }],
        };
        let actual: ResolvedDialect =
            resolve_dialect(&schema, &JsonSchemaSettings::default()).expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_unknown_schema_uri_rejected_when_strict() {
        let schema: JsonSchema = schema_with_uri("https://example.com/custom-meta");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let actual: Result<ResolvedDialect, JsonSchemaParseError> =
            resolve_dialect(&schema, &settings);
        assert!(matches!(
            actual,
            Err(JsonSchemaParseError::UnknownSchemaUri { ref uri }) if uri == "https://example.com/custom-meta"
        ));
    }

    #[test]
    fn resolve_dialect_unknown_schema_uri_ignored_when_spec_version_set() {
        let schema: JsonSchema = schema_with_uri("https://example.com/custom-meta");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft07)
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft07),
            warnings: vec![],
        };
        let actual: ResolvedDialect = resolve_dialect(&schema, &settings).expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_pre_draft4_warns_and_uses_draft4_rules() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-03/schema#");
        let expected: ResolvedDialect = ResolvedDialect {
            dialect: Dialect::for_spec_version(SpecVersion::Draft03),
            warnings: vec![DialectWarning::UnsupportedDraft {
                spec_version: SpecVersion::Draft03,
            }],
        };
        let actual: ResolvedDialect =
            resolve_dialect(&schema, &JsonSchemaSettings::default()).expect("resolve");
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_format_mode_override_beats_draft_default() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-07/schema#");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .format_mode(FormatMode::Annotation)
            .build();
        let expected: FormatMode = FormatMode::Annotation;
        let actual: FormatMode = resolve_dialect(&schema, &settings)
            .expect("resolve")
            .dialect
            .format_mode;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_without_override_uses_draft_format_mode() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-07/schema#");
        let expected: FormatMode = FormatMode::Assertion;
        let actual: FormatMode = resolve_dialect(&schema, &JsonSchemaSettings::default())
            .expect("resolve")
            .dialect
            .format_mode;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_integer_semantics_override_beats_draft_default() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-04/schema#");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .integer_semantics(IntegerSemantics::Numeric)
            .build();
        let expected: IntegerSemantics = IntegerSemantics::Numeric;
        let actual: IntegerSemantics = resolve_dialect(&schema, &settings)
            .expect("resolve")
            .dialect
            .integer_semantics;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_without_override_uses_draft_integer_semantics() {
        let schema: JsonSchema = schema_with_uri("http://json-schema.org/draft-04/schema#");
        let expected: IntegerSemantics = IntegerSemantics::Lexical;
        let actual: IntegerSemantics = resolve_dialect(&schema, &JsonSchemaSettings::default())
            .expect("resolve")
            .dialect
            .integer_semantics;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_exclusive_bounds_override_beats_draft_default() {
        let schema: JsonSchema = schema_with_uri("https://json-schema.org/draft/2020-12/schema");
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .exclusive_bounds(ExclusiveBounds::BooleanModifier)
            .build();
        let expected: ExclusiveBounds = ExclusiveBounds::BooleanModifier;
        let actual: ExclusiveBounds = resolve_dialect(&schema, &settings)
            .expect("resolve")
            .dialect
            .exclusive_bounds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_dialect_without_override_uses_draft_exclusive_bounds() {
        let schema: JsonSchema = schema_with_uri("https://json-schema.org/draft/2020-12/schema");
        let expected: ExclusiveBounds = ExclusiveBounds::Numeric;
        let actual: ExclusiveBounds = resolve_dialect(&schema, &JsonSchemaSettings::default())
            .expect("resolve")
            .dialect
            .exclusive_bounds;
        assert_eq!(expected, actual);
    }
}
