//! JSON Schema ingestion settings.

use super::json_schema::JsonSchema;
use super::spec_version::SpecVersion;

/// Settings that affect how JSON Schema definitions are ingested (parsed).
///
/// Default values from [`JsonSchemaSettingsBuilder::build`] target **JSON Schema
/// Draft 2020-12**. When no spec version is explicitly provided, use
/// `JsonSchemaSettings::builder().build()`; it is equivalent to
/// [`SpecVersion::Draft202012.default_schema_settings()`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonSchemaSettings {
    /// When `true`, schema ingestion fails if any schema object contains keys
    /// other than the known keywords we model (including `$schema`, `type`, `properties`, `required`, `title`, `description`, etc.).
    pub disallow_unknown_fields: bool,

    /// Explicit spec version to use. When `Some(v)`, that version is used and `$schema` is not used for draft selection.
    /// When `None`, the spec version is inferred from the root schema's `$schema` keyword; if absent or unrecognized, **Draft 2020-12** is used.
    pub spec_version: Option<SpecVersion>,
}

/// Builder for [`JsonSchemaSettings`].
#[derive(Debug, Clone, Default)]
pub struct JsonSchemaSettingsBuilder {
    disallow_unknown_fields: Option<bool>,
    spec_version: Option<SpecVersion>,
}

impl JsonSchemaSettingsBuilder {
    /// Set whether unknown fields in the schema definition are disallowed.
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

    /// Build the settings. Any option not set uses its per-option default.
    #[must_use]
    pub fn build(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: self.disallow_unknown_fields.unwrap_or(false),
            spec_version: self.spec_version,
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

/// Returns the effective spec version for a parsed root schema and settings.
///
/// When [`JsonSchemaSettings::spec_version`] is `Some(v)`, returns `v`.
/// Otherwise, infers from the root schema's `$schema` keyword via [`SpecVersion::from_schema_uri`];
/// if `$schema` is absent or unrecognized, returns **Draft 2020-12** (latest).
#[must_use]
pub fn resolved_spec_version(schema: &JsonSchema, settings: &JsonSchemaSettings) -> SpecVersion {
    if let Some(v) = settings.spec_version {
        return v;
    }
    schema
        .schema
        .as_deref()
        .and_then(SpecVersion::from_schema_uri)
        .unwrap_or(SpecVersion::Draft202012)
}

#[cfg(test)]
mod tests {
    use super::{JsonSchemaSettings, resolved_spec_version};
    use crate::json_schema::SpecVersion;
    use crate::json_schema::json_schema::JsonSchema;

    #[test]
    fn builder_default_disallow_is_false() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
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
    fn resolved_spec_version_uses_explicit_when_set() {
        let schema: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            ..JsonSchema::default()
        };
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft04)
            .build();
        let expected: SpecVersion = SpecVersion::Draft04;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_infers_from_schema_uri_when_not_set() {
        let schema: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            ..JsonSchema::default()
        };
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_absent() {
        let schema: JsonSchema = JsonSchema::default();
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolved_spec_version_defaults_to_2020_12_when_schema_unknown() {
        let schema: JsonSchema = JsonSchema {
            schema: Some("https://unknown.example/schema".to_string()),
            ..JsonSchema::default()
        };
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let expected: SpecVersion = SpecVersion::Draft202012;
        let actual: SpecVersion = resolved_spec_version(&schema, &settings);
        assert_eq!(expected, actual);
    }
}
