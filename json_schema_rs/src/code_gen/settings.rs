//! Code generation settings (model naming, dedupe mode, etc.).

/// How to choose the generated struct/type name when both `title` and property key are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelNameSource {
    /// Use `title` first, then property key, then `"Root"` for the root schema. (Current behavior.)
    #[default]
    TitleFirst,
    /// Use property key first, then `title`, then `"Root"` for the root schema.
    PropertyKeyFirst,
}

/// Whether and how to deduplicate structurally identical object schemas across and within schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DedupeMode {
    /// No deduping. Identical Rust models are generated duplicately.
    Disabled,
    /// Only pivotal/functional data is considered (type_, properties, required, title, constraints).
    /// Excludes non-functional fields such as description. Comparison is deep.
    Functional,
    /// Everything is considered, including non-functional fields like description. Comparison is deep.
    #[default]
    Full,
}

/// Language-agnostic code generation settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGenSettings {
    /// Which source to prefer for struct/type names: title or property key.
    pub model_name_source: ModelNameSource,
    /// Whether and how to deduplicate identical object schemas (Disabled, Functional, Full).
    pub dedupe_mode: DedupeMode,
}

impl Default for CodeGenSettings {
    fn default() -> Self {
        Self {
            model_name_source: ModelNameSource::TitleFirst,
            dedupe_mode: DedupeMode::Full,
        }
    }
}

/// Builder for [`CodeGenSettings`].
#[derive(Debug, Clone, Default)]
pub struct CodeGenSettingsBuilder {
    model_name_source: Option<ModelNameSource>,
    dedupe_mode: Option<DedupeMode>,
}

impl CodeGenSettingsBuilder {
    /// Set the model name source (title first vs property key first).
    #[must_use]
    pub fn model_name_source(mut self, value: ModelNameSource) -> Self {
        self.model_name_source = Some(value);
        self
    }

    /// Set the dedupe mode (Disabled, Functional, or Full).
    #[must_use]
    pub fn dedupe_mode(mut self, value: DedupeMode) -> Self {
        self.dedupe_mode = Some(value);
        self
    }

    /// Build the settings. Any option not set uses its per-option default.
    #[must_use]
    pub fn build(self) -> CodeGenSettings {
        CodeGenSettings {
            model_name_source: self.model_name_source.unwrap_or_default(),
            dedupe_mode: self.dedupe_mode.unwrap_or_default(),
        }
    }
}

impl CodeGenSettings {
    /// Start a builder with all options unset (per-option defaults will be used on [`build`](CodeGenSettingsBuilder::build)).
    #[must_use]
    pub fn builder() -> CodeGenSettingsBuilder {
        CodeGenSettingsBuilder::default()
    }
}
