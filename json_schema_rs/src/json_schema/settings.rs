//! JSON Schema ingestion settings.

/// Settings that affect how JSON Schema definitions are ingested (parsed).
///
/// Default values from [`JsonSchemaSettingsBuilder::build`] target **JSON Schema
/// Draft 2020-12**. When no spec version is explicitly provided, use
/// `JsonSchemaSettings::builder().build()`; it is equivalent to
/// [`SpecVersion::Draft202012.default_schema_settings()`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonSchemaSettings {
    /// When `true`, schema ingestion fails if any schema object contains keys
    /// other than the known keywords we model (`type`, `properties`, `required`, `title`).
    pub disallow_unknown_fields: bool,
}

/// Builder for [`JsonSchemaSettings`].
#[derive(Debug, Clone, Default)]
pub struct JsonSchemaSettingsBuilder {
    disallow_unknown_fields: Option<bool>,
}

impl JsonSchemaSettingsBuilder {
    /// Set whether unknown fields in the schema definition are disallowed.
    #[must_use]
    pub fn disallow_unknown_fields(mut self, value: bool) -> Self {
        self.disallow_unknown_fields = Some(value);
        self
    }

    /// Build the settings. Any option not set uses its per-option default.
    #[must_use]
    pub fn build(self) -> JsonSchemaSettings {
        JsonSchemaSettings {
            disallow_unknown_fields: self.disallow_unknown_fields.unwrap_or(false),
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

#[cfg(test)]
mod tests {
    use super::JsonSchemaSettings;

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
}
