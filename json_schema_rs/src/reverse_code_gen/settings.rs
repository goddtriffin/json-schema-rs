//! Settings for [`SchemaGenerator`](super::SchemaGenerator): which dialect the schema is written
//! in and how the document is laid out.

use crate::json_schema::SpecVersion;
use std::fmt;

/// The JSON Schema dialect a generated schema is written in.
///
/// Each variant is a target the generator converts to exactly or refuses with a
/// [`ReverseCodeGenError::UnrepresentableKeyword`](super::ReverseCodeGenError) naming the keyword
/// the target cannot express; it never approximates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetDialect {
    /// JSON Schema draft-04.
    Draft04,
    /// JSON Schema draft-06.
    Draft06,
    /// JSON Schema draft-07.
    Draft07,
    /// JSON Schema 2019-09.
    Draft201909,
    /// JSON Schema 2020-12, the default.
    Draft202012,
    /// The Schema Object of `OpenAPI` 3.0: a draft-04 based subset with `nullable`.
    OpenApi30,
    /// The Schema Object of `OpenAPI` 3.1: JSON Schema 2020-12 in an `OpenAPI` document layout.
    OpenApi31,
}

impl TargetDialect {
    /// The draft whose keyword table and rules the target follows, or `None` for `OpenAPI` 3.0,
    /// which has its own table.
    pub(super) fn spec_version(self) -> Option<SpecVersion> {
        match self {
            TargetDialect::Draft04 => Some(SpecVersion::Draft04),
            TargetDialect::Draft06 => Some(SpecVersion::Draft06),
            TargetDialect::Draft07 => Some(SpecVersion::Draft07),
            TargetDialect::Draft201909 => Some(SpecVersion::Draft201909),
            TargetDialect::Draft202012 | TargetDialect::OpenApi31 => Some(SpecVersion::Draft202012),
            TargetDialect::OpenApi30 => None,
        }
    }

    /// The `$schema` URI a document in this dialect carries; none for `OpenAPI` documents, whose
    /// dialect the document itself declares.
    pub(super) fn schema_uri(self) -> Option<&'static str> {
        match self {
            TargetDialect::OpenApi30 | TargetDialect::OpenApi31 => None,
            _ => self.spec_version().map(SpecVersion::schema_uri),
        }
    }

    /// The JSON Pointer of the container the dialect keeps definitions in.
    pub(super) fn default_definitions_path(self) -> &'static str {
        match self {
            TargetDialect::Draft04 | TargetDialect::Draft06 | TargetDialect::Draft07 => {
                "/definitions"
            }
            TargetDialect::Draft201909 | TargetDialect::Draft202012 => "/$defs",
            TargetDialect::OpenApi30 | TargetDialect::OpenApi31 => "/components/schemas",
        }
    }

    /// Whether building as 2020-12 needs a conversion pass for this target.
    pub(super) fn needs_conversion(self) -> bool {
        !matches!(self, TargetDialect::Draft202012 | TargetDialect::OpenApi31)
    }
}

impl fmt::Display for TargetDialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TargetDialect::Draft04 => "draft-04",
            TargetDialect::Draft06 => "draft-06",
            TargetDialect::Draft07 => "draft-07",
            TargetDialect::Draft201909 => "2019-09",
            TargetDialect::Draft202012 => "2020-12",
            TargetDialect::OpenApi30 => "OpenAPI 3.0",
            TargetDialect::OpenApi31 => "OpenAPI 3.1",
        })
    }
}

/// What a generated document's root `$schema` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum MetaSchema {
    /// The target dialect's own metaschema URI; none for `OpenAPI` targets.
    #[default]
    FollowDialect,
    /// This URI, which must be absolute.
    Uri(String),
    /// No `$schema`.
    Omit,
}

/// The `format` a number type carries.
///
/// JSON Schema defines no `format` for numbers; these are the vocabularies other tools read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumberFormats {
    /// `OpenAPI`'s: `i8`, `i16`, `i32`, `u8` and `u16` are `int32`; `i64` and `u32` are `int64`;
    /// `f32` is `float`; `f64` is `double`. `u64` has none.
    OpenApi,
    /// schemars': `int8`, `int16`, `int32`, `int64`, `uint8`, `uint16`, `uint32`, `uint64`,
    /// `float` and `double`.
    Schemars,
}

/// How a [`SchemaGenerator`](super::SchemaGenerator) writes its document.
///
/// [`Default`] is the spec-exact output: JSON Schema 2020-12 with the definitions in `$defs`.
///
/// # Example
///
/// Writing a schema as draft-07 (the schemars `draft07()` generator):
///
/// ```
/// use json_schema_rs::reverse_code_gen::{ReverseCodeGenSettings, TargetDialect};
/// use json_schema_rs::{GeneratedSchemaResult, SchemaGenerator};
///
/// let settings: ReverseCodeGenSettings = ReverseCodeGenSettings {
///     dialect: Some(TargetDialect::Draft07),
///     ..Default::default()
/// };
/// let schema: GeneratedSchemaResult = SchemaGenerator::new(settings).into_root_schema_for::<Vec<String>>();
/// let expected: String = r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":{"type":"string"},"type":"array"}"#.to_string();
/// let actual: String = serde_json::to_string(&schema.unwrap()).unwrap();
/// assert_eq!(expected, actual);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReverseCodeGenSettings {
    /// The dialect to write. `None` is JSON Schema 2020-12.
    pub dialect: Option<TargetDialect>,

    /// A JSON Pointer to the document root's definitions: `/$defs` (2019-09 and later),
    /// `/definitions` (drafts 04 to 07) or `/components/schemas` (`OpenAPI`) when `None`. One
    /// leading `#` and one trailing `/` are ignored, so `#/components/schemas/` is
    /// `/components/schemas`. Every `$ref` to a definition is written to match, as a URI fragment:
    /// `/x y` is `#/x%20y/..`.
    pub definitions_path: Option<String>,

    /// Write each named type's schema where it is used instead of in the definitions, keeping a
    /// `$ref` and a definition only for a type that refers back to itself. Types with `$id` are
    /// schema resources and are never inlined.
    pub inline_subschemas: bool,

    /// What the root `$schema` says.
    pub meta_schema: MetaSchema,

    /// The `format` number types carry. `None` writes none.
    pub number_formats: Option<NumberFormats>,
}

impl ReverseCodeGenSettings {
    /// Starts a builder with every setting at its default.
    #[must_use]
    pub fn builder() -> ReverseCodeGenSettingsBuilder {
        ReverseCodeGenSettingsBuilder::default()
    }

    /// The dialect written: [`ReverseCodeGenSettings::dialect`], or 2020-12.
    pub(super) fn target(&self) -> TargetDialect {
        self.dialect.unwrap_or(TargetDialect::Draft202012)
    }
}

/// Builder for [`ReverseCodeGenSettings`].
#[derive(Debug, Clone, Default)]
pub struct ReverseCodeGenSettingsBuilder {
    settings: ReverseCodeGenSettings,
}

impl ReverseCodeGenSettingsBuilder {
    /// Sets [`ReverseCodeGenSettings::dialect`].
    #[must_use]
    pub fn dialect(mut self, value: TargetDialect) -> Self {
        self.settings.dialect = Some(value);
        self
    }

    /// Sets [`ReverseCodeGenSettings::definitions_path`].
    #[must_use]
    pub fn definitions_path(mut self, value: impl Into<String>) -> Self {
        self.settings.definitions_path = Some(value.into());
        self
    }

    /// Sets [`ReverseCodeGenSettings::inline_subschemas`].
    #[must_use]
    pub fn inline_subschemas(mut self, value: bool) -> Self {
        self.settings.inline_subschemas = value;
        self
    }

    /// Sets [`ReverseCodeGenSettings::meta_schema`].
    #[must_use]
    pub fn meta_schema(mut self, value: MetaSchema) -> Self {
        self.settings.meta_schema = value;
        self
    }

    /// Sets [`ReverseCodeGenSettings::number_formats`].
    #[must_use]
    pub fn number_formats(mut self, value: NumberFormats) -> Self {
        self.settings.number_formats = Some(value);
        self
    }

    /// Builds the settings. Any setting not set keeps its default.
    #[must_use]
    pub fn build(self) -> ReverseCodeGenSettings {
        self.settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_write_2020_12() {
        let expected: TargetDialect = TargetDialect::Draft202012;
        let actual: TargetDialect = ReverseCodeGenSettings::default().target();
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_sets_every_setting() {
        let expected: ReverseCodeGenSettings = ReverseCodeGenSettings {
            dialect: Some(TargetDialect::Draft07),
            definitions_path: Some("/x/y".to_string()),
            inline_subschemas: true,
            meta_schema: MetaSchema::Omit,
            number_formats: Some(NumberFormats::OpenApi),
        };
        let actual: ReverseCodeGenSettings = ReverseCodeGenSettings::builder()
            .dialect(TargetDialect::Draft07)
            .definitions_path("/x/y")
            .inline_subschemas(true)
            .meta_schema(MetaSchema::Omit)
            .number_formats(NumberFormats::OpenApi)
            .build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_without_calls_builds_the_default() {
        let expected: ReverseCodeGenSettings = ReverseCodeGenSettings::default();
        let actual: ReverseCodeGenSettings = ReverseCodeGenSettings::builder().build();
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_definitions_paths_follow_the_dialect() {
        let expected: Vec<&str> = vec![
            "/definitions",
            "/definitions",
            "/definitions",
            "/$defs",
            "/$defs",
            "/components/schemas",
            "/components/schemas",
        ];
        let actual: Vec<&str> = [
            TargetDialect::Draft04,
            TargetDialect::Draft06,
            TargetDialect::Draft07,
            TargetDialect::Draft201909,
            TargetDialect::Draft202012,
            TargetDialect::OpenApi30,
            TargetDialect::OpenApi31,
        ]
        .into_iter()
        .map(TargetDialect::default_definitions_path)
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_uris_follow_the_dialect_and_openapi_has_none() {
        let expected: Vec<Option<&str>> = vec![
            Some("http://json-schema.org/draft-04/schema#"),
            Some("http://json-schema.org/draft-06/schema#"),
            Some("http://json-schema.org/draft-07/schema#"),
            Some("https://json-schema.org/draft/2019-09/schema"),
            Some("https://json-schema.org/draft/2020-12/schema"),
            None,
            None,
        ];
        let actual: Vec<Option<&str>> = [
            TargetDialect::Draft04,
            TargetDialect::Draft06,
            TargetDialect::Draft07,
            TargetDialect::Draft201909,
            TargetDialect::Draft202012,
            TargetDialect::OpenApi30,
            TargetDialect::OpenApi31,
        ]
        .into_iter()
        .map(TargetDialect::schema_uri)
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn dialect_display_names_the_dialect() {
        let expected: Vec<String> = vec![
            "draft-04".to_string(),
            "draft-06".to_string(),
            "draft-07".to_string(),
            "2019-09".to_string(),
            "2020-12".to_string(),
            "OpenAPI 3.0".to_string(),
            "OpenAPI 3.1".to_string(),
        ];
        let actual: Vec<String> = [
            TargetDialect::Draft04,
            TargetDialect::Draft06,
            TargetDialect::Draft07,
            TargetDialect::Draft201909,
            TargetDialect::Draft202012,
            TargetDialect::OpenApi30,
            TargetDialect::OpenApi31,
        ]
        .into_iter()
        .map(|dialect: TargetDialect| dialect.to_string())
        .collect();
        assert_eq!(expected, actual);
    }
}
