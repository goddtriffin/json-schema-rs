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
    #[must_use]
    pub fn default_schema_settings(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonSchemaSettings, SpecVersion};

    #[test]
    fn spec_version_returns_settings() {
        let settings: JsonSchemaSettings = SpecVersion::Draft07.default_schema_settings();
        assert!(!settings.disallow_unknown_fields);
    }
}
