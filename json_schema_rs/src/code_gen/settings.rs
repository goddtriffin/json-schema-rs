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
    /// Only pivotal/functional data is considered (the whole `type` set, properties, required,
    /// title, constraints). Excludes non-functional fields such as description. Comparison is deep.
    Functional,
    /// Everything is considered, including non-functional fields like description. Comparison is deep.
    #[default]
    Full,
}

/// What generated types do with object keys their schema does not name, overriding what the
/// schema says. Without an override, a schema whose `additionalProperties` is absent or `true`
/// (any key allowed, the spec's default) keeps unknown keys in a map field, so a deserialize /
/// serialize round trip loses nothing, and `additionalProperties: false` rejects them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownProperties {
    /// Keep unknown keys in a `#[serde(flatten)]` map field on every object type, even where
    /// the schema forbids them (`additionalProperties: false`).
    Capture,
    /// Ignore unknown keys: they are dropped when deserializing (faster: no map field, no
    /// serde buffering). `additionalProperties: false` still rejects them.
    Drop,
}

/// The Rust collection for an array with `uniqueItems: true`.
///
/// The default, [`Vec`](UniqueItemsType::Vec), is what the spec describes: a JSON array is an
/// ordered list (2020-12 core §4.2.1) and `uniqueItems` only asserts that its items are unique
/// (validation §6.4.3), so a `Vec` keeps the instance exactly, for every item type. The set
/// types make uniqueness part of the Rust type: a duplicate item is a deserialize error (never
/// silently dropped). A set applies to a property whose value is the array (`T`, `Option<T>`)
/// when Rust equality is JSON equality for the item type: strings, integers, booleans, `null`,
/// generated enums, and generated structs and unions built from these (which then derive
/// `Hash`, `Eq` and `Ord`). Other positions (array items, map values, union variants, a root
/// alias) and other item types stay `Vec`: floats, any value, `Uuid`, a struct that drops or
/// keeps untyped unknown keys or has a set field, and a union where `1` and `1.0` could land
/// in different variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UniqueItemsType {
    /// `Vec<T>`: keeps item order and the instance exactly; the validator checks uniqueness.
    #[default]
    Vec,
    /// `std::collections::HashSet<T>`: fast lookup; serializes in an unspecified order that can
    /// differ between runs.
    HashSet,
    /// `std::collections::BTreeSet<T>`: serializes in sorted order (not the instance's order).
    BTreeSet,
    /// `indexmap::IndexSet<T>`: keeps item order and makes uniqueness part of the type. The
    /// generated code needs the `indexmap` crate with its `serde` feature.
    #[cfg(feature = "indexmap")]
    IndexSet,
}

/// Language-agnostic code generation settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGenSettings {
    /// Which source to prefer for struct/type names: title or property key.
    pub model_name_source: ModelNameSource,
    /// Whether and how to deduplicate identical object schemas (Disabled, Functional, Full).
    pub dedupe_mode: DedupeMode,
    /// Override for unknown object keys; `None` follows each schema's `additionalProperties`.
    pub unknown_properties: Option<UnknownProperties>,
    /// The Rust collection for `uniqueItems` arrays (default `Vec`).
    pub unique_items: UniqueItemsType,
    /// Generate an `anyOf` / `oneOf` with exactly one entry as that entry's type instead of a
    /// one-variant union (default `false`). The two are equivalent for validation, so the type
    /// accepts the same instances either way; a one-variant union keeps the combinator on a
    /// round trip through the derive, the flattened type does not. A one-entry `allOf` is
    /// always merged into its schema.
    pub flatten_single_entry_combinators: bool,
}

impl Default for CodeGenSettings {
    fn default() -> Self {
        Self {
            model_name_source: ModelNameSource::TitleFirst,
            dedupe_mode: DedupeMode::Full,
            unknown_properties: None,
            unique_items: UniqueItemsType::Vec,
            flatten_single_entry_combinators: false,
        }
    }
}

/// Builder for [`CodeGenSettings`].
#[derive(Debug, Clone, Default)]
pub struct CodeGenSettingsBuilder {
    model_name_source: Option<ModelNameSource>,
    dedupe_mode: Option<DedupeMode>,
    unknown_properties: Option<UnknownProperties>,
    unique_items: Option<UniqueItemsType>,
    flatten_single_entry_combinators: Option<bool>,
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

    /// Override what generated types do with unknown object keys (see [`UnknownProperties`]).
    #[must_use]
    pub fn unknown_properties(mut self, value: UnknownProperties) -> Self {
        self.unknown_properties = Some(value);
        self
    }

    /// Set the Rust collection for `uniqueItems` arrays (see [`UniqueItemsType`]).
    #[must_use]
    pub fn unique_items(mut self, value: UniqueItemsType) -> Self {
        self.unique_items = Some(value);
        self
    }

    /// Generate a one-entry `anyOf` / `oneOf` as its entry's type (see
    /// [`CodeGenSettings::flatten_single_entry_combinators`]).
    #[must_use]
    pub fn flatten_single_entry_combinators(mut self, value: bool) -> Self {
        self.flatten_single_entry_combinators = Some(value);
        self
    }

    /// Build the settings. Any option not set uses its per-option default.
    #[must_use]
    pub fn build(self) -> CodeGenSettings {
        CodeGenSettings {
            model_name_source: self.model_name_source.unwrap_or_default(),
            dedupe_mode: self.dedupe_mode.unwrap_or_default(),
            unknown_properties: self.unknown_properties,
            unique_items: self.unique_items.unwrap_or_default(),
            flatten_single_entry_combinators: self
                .flatten_single_entry_combinators
                .unwrap_or_default(),
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
