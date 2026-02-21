//! JSON Schema specification version.

use super::settings::JsonSchemaSettings;

/// JSON Schema specification version. One variant per vendored spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecVersion {
    Draft00,
    Draft01,
    Draft02,
    Draft03,
    Draft04,
    Draft05,
    Draft06,
    Draft07,
    Draft201909,
    Draft202012,
}

impl SpecVersion {
    /// Returns [`JsonSchemaSettings`] tuned for this spec version.
    /// Callers can use the builder to override individual options.
    ///
    /// **Default (latest) spec:** [`Draft202012`](SpecVersion::Draft202012) is
    /// the latest supported spec; its settings match
    /// `JsonSchemaSettings::builder().build()` when no options are set.
    #[must_use]
    pub fn default_schema_settings(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::json_schema::SpecVersion;
    use crate::json_schema::settings::JsonSchemaSettings;

    #[test]
    fn spec_version_returns_settings() {
        let settings: JsonSchemaSettings = SpecVersion::Draft07.default_schema_settings();
        assert!(!settings.disallow_unknown_fields);
    }

    /// Default builder output matches Draft 2020-12 (latest spec) settings.
    #[test]
    fn default_settings_match_draft202012() {
        let from_builder: JsonSchemaSettings = JsonSchemaSettings::builder().build();
        let from_spec: JsonSchemaSettings = SpecVersion::Draft202012.default_schema_settings();
        assert_eq!(from_builder, from_spec);
    }
}
