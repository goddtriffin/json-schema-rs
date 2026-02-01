//! Settings for JSON Schema code generation.

/// Settings that control code generation behavior.
#[derive(Debug, Clone, Default)]
pub struct GenerateSettings {
    /// When true, fail before code generation if the schema contains invalid
    /// or unsupported JSON Schema features. Collects all issues and returns
    /// them together.
    ///
    /// **Default: false (disabled).** This is the lenient default—consumers
    /// must opt in to strict validation. When false, unsupported features are
    /// silently ignored (current behavior).
    pub deny_invalid_unknown_json_schema: bool,
}
