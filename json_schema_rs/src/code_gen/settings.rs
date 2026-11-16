//! Code generation settings (model naming, dedupe mode, etc.).

use std::collections::BTreeMap;

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
    /// Ignore unknown keys: they are dropped when deserializing (faster: no map field to fill,
    /// and the struct derives serde's `Deserialize`). `additionalProperties: false` still
    /// rejects them.
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

/// The Rust map type for an object's string-keyed entries: a schema whose only keyword for keys
/// is `additionalProperties` (a map type alias, field or item), and a struct's flattened
/// additional properties.
///
/// A JSON object is an unordered set of members (RFC 8259 §4), so every map keeps the instance's
/// meaning; the choice is which Rust properties the map has. The default,
/// [`BTreeMap`](MapType::BTreeMap), serializes in sorted key order (deterministic output) and lets
/// the types holding it derive `Hash` and `Ord` (a `uniqueItems` set item).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapType {
    /// `std::collections::BTreeMap<String, V>`: sorted keys.
    #[default]
    BTreeMap,
    /// `std::collections::HashMap<String, V>`: fast lookup; serializes in an unspecified order
    /// that can differ between runs. A type holding one does not derive `Hash` or `Ord`, so it
    /// is not a `uniqueItems` set item.
    HashMap,
    /// `indexmap::IndexMap<String, V>`: keeps the instance's key order. A type holding one does
    /// not derive `Ord`, so it is not a `uniqueItems` set item. The generated code needs the
    /// `indexmap` crate with its `serde` feature.
    #[cfg(feature = "indexmap")]
    IndexMap,
}

/// Whether a `format` codegen has a Rust type for gets that type when the schema's dialect only
/// annotates formats. The typed formats: `uuid` → `uuid::Uuid` (the `uuid` feature),
/// `date-time` → `chrono::DateTime<chrono::FixedOffset>` and `date` → `chrono::NaiveDate` (the
/// `chrono` feature), `ipv4` / `ipv6` → `std::net::Ipv4Addr` / `Ipv6Addr`.
///
/// The default, [`Typed`](FormatTypes::Typed), **deliberately diverges from drafts 2019-09 and
/// 2020-12**, where `format` is an annotation by default and any string satisfies
/// `"format": "uuid"`: a schema author who writes `uuid` means a UUID, so the field is a `Uuid`
/// and a string that is not one fails to deserialize. [`FollowDialect`](FormatTypes::FollowDialect)
/// is the spec-exact choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatTypes {
    /// The format's Rust type wherever the format appears, whatever the dialect's format mode.
    #[default]
    Typed,
    /// The format's Rust type only where the dialect asserts the format (`format_mode` is
    /// `Assertion` and the draft defines it); elsewhere the plain `String` the annotation allows.
    FollowDialect,
}

/// Changes to one generated type, keyed by its generated name in
/// [`CodeGenSettings::patches`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TypePatch {
    /// The name the type gets instead of its generated one: a Rust identifier no other type in
    /// its module has.
    pub rename: Option<String>,
    /// Derive macros the type gets on top of its own and
    /// [`CodeGenSettings::extra_derives`], as paths (`Hash`, `schemars::JsonSchema`).
    pub derives: Vec<String>,
    /// Attributes the type gets on top of [`CodeGenSettings::extra_attrs`], each the text
    /// inside `#[...]` (`serde(deny_unknown_fields)`).
    pub attrs: Vec<String>,
}

/// What codegen does with an `x-rust-type` extension whose crate is not in
/// [`CodeGenSettings::crates`].
///
/// `x-rust-type` (`{"crate": "my-types", "version": "^1.2", "path": "my_types::Thing"}`, with
/// optional `"parameters": [schema, ..]` for generic types) says a subschema is a type of an
/// existing crate. The JSON Schema specification ignores unknown keywords, so the default
/// generates the subschema as usual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownCrates {
    /// Generate the type from the subschema, as if the extension were absent.
    #[default]
    Generate,
    /// Use the extension's type, assuming its crate is a dependency at a matching version.
    Allow,
    /// Fail: the schema names a crate the output may not depend on.
    Deny,
}

/// The version of a crate in [`CodeGenSettings::crates`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrateVersion {
    /// The version the output depends on: an `x-rust-type` whose version requirement it meets
    /// is used, any other is generated.
    Version(semver::Version),
    /// Any version (`*`): every `x-rust-type` of the crate is used.
    Any,
    /// Never use the crate (`!`): its `x-rust-type`s are generated, even under
    /// [`UnknownCrates::Allow`] or [`UnknownCrates::Deny`].
    Never,
}

impl std::str::FromStr for CrateVersion {
    type Err = String;

    /// `*` for [`Any`](CrateVersion::Any), `!` for [`Never`](CrateVersion::Never), otherwise a
    /// version; missing minor and patch numbers are zero (`1.2` is `1.2.0`).
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "*" => return Ok(CrateVersion::Any),
            "!" => return Ok(CrateVersion::Never),
            _ => {}
        }
        let end: usize = text.find(['-', '+']).unwrap_or(text.len());
        let padding: &str = match text[..end].matches('.').count() {
            0 => ".0.0",
            1 => ".0",
            _ => "",
        };
        let full: String = format!("{}{padding}{}", &text[..end], &text[end..]);
        semver::Version::parse(&full)
            .map(CrateVersion::Version)
            .map_err(|e| format!("`{text}` is not a crate version (`1.2.3`, `*` or `!`): {e}"))
    }
}

/// One crate of [`CodeGenSettings::crates`]: its version and, when the output knows it under
/// another name, that name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateSpec {
    /// The crate's version (see [`CrateVersion`]).
    pub version: CrateVersion,
    /// The name the output depends on the crate under, replacing the first segment of each
    /// `x-rust-type` path (`-` read as `_`); `None` keeps the crate's own name.
    pub rename: Option<String>,
}

impl CrateSpec {
    /// Parses `[rename=]crate@version` (the CLI's `--cgs-crate`, typify's `--crate`) into the
    /// crate's name and its spec; the version is as [`CrateVersion`] parses it.
    ///
    /// # Errors
    ///
    /// A message when the text is not of that form, a name is not a crate name, or the version
    /// does not parse.
    pub fn parse_named(text: &str) -> Result<(String, CrateSpec), String> {
        let is_crate = |name: &str| {
            name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        };
        let (rename, rest): (Option<&str>, &str) = match text.split_once('=') {
            Some((rename, rest)) => (Some(rename), rest),
            None => (None, text),
        };
        let Some((name, version)) = rest.split_once('@') else {
            return Err(format!(
                "crate `{text}` is not of the form `[rename=]crate@version`"
            ));
        };
        for part in rename.into_iter().chain([name]) {
            if !is_crate(part) {
                return Err(format!("crate `{text}`: `{part}` is not a crate name"));
            }
        }
        let version: CrateVersion = version
            .parse()
            .map_err(|e: String| format!("crate `{text}`: {e}"))?;
        Ok((
            name.to_string(),
            CrateSpec {
                version,
                rename: rename.map(str::to_string),
            },
        ))
    }
}

/// One entry of [`CodeGenSettings::conversions`]: a subschema generated as a type of the
/// user's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeConversion {
    /// The subschema to match, as JSON. A subschema matches when it equals this one, ignoring
    /// the annotations of both (`title`, `description`, `$comment`, `default`, `examples`,
    /// `deprecated`, `readOnly`, `writeOnly`) at their top level; a property that only
    /// `$ref`s a matching definition matches too.
    pub schema: serde_json::Value,
    /// The Rust type generated in its place, as a path (`rust_decimal::Decimal`).
    pub rust_type: String,
}

/// Language-agnostic code generation settings.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent on/off setting, mirrored by a CLI flag and a macro argument"
)]
pub struct CodeGenSettings {
    /// Which source to prefer for struct/type names: title or property key.
    pub model_name_source: ModelNameSource,
    /// Whether and how to deduplicate identical object schemas (Disabled, Functional, Full).
    pub dedupe_mode: DedupeMode,
    /// Override for unknown object keys; `None` follows each schema's `additionalProperties`.
    pub unknown_properties: Option<UnknownProperties>,
    /// The Rust collection for `uniqueItems` arrays (default `Vec`).
    pub unique_items: UniqueItemsType,
    /// The Rust map type for objects' string-keyed entries (default `BTreeMap`).
    pub map_type: MapType,
    /// Generate an `anyOf` / `oneOf` with exactly one entry as that entry's type instead of a
    /// one-variant union (default `false`). The two are equivalent for validation, so the type
    /// accepts the same instances either way; a one-variant union keeps the combinator on a
    /// round trip through the derive, the flattened type does not. A one-entry `allOf` is
    /// always merged into its schema.
    pub flatten_single_entry_combinators: bool,
    /// Generate types that enforce their schema through the library's validator (default
    /// `true`). Each generated struct, enum and union gets `validate(&self)` and
    /// `TryFrom<serde_json::Value>` (validate, then deserialize), with the errors
    /// [`CompiledSchema::validate`](crate::CompiledSchema::validate) gives; an `anyOf` / `oneOf`
    /// union deserializes to a variant whose branch the instance satisfies (`oneOf`: exactly
    /// one). The generated code embeds the parts of the schema documents its types' validation
    /// reaches and compiles them once, on first use; it then depends on `json-schema-rs` and
    /// `serde_json`. Off, the types are plain data: no constraint beyond the Rust shape is
    /// checked, and a union deserializes to the first variant whose shape fits.
    pub self_validating: bool,
    /// Generate a constrained newtype for each string, integer, number or boolean whose schema
    /// asserts more than its Rust type holds (default `true`): `minLength`, `maxLength`,
    /// `pattern`, a checked `format`, bounds the integer type does not already hold, any bound on
    /// a float, `multipleOf`, `not`, and the other assertions the Rust type cannot show. The
    /// newtype (`pub struct Name(String);`) can only hold a value its schema accepts: its
    /// `TryFrom` and `Deserialize` validate the value through the library's validator (exactly
    /// [`CompiledSchema::validate`](crate::CompiledSchema::validate)'s semantics and errors), so
    /// an invalid value is rejected at the field during deserialization. The generated code
    /// embeds the schema parts the newtypes' validation reaches (as with
    /// [`self_validating`](CodeGenSettings::self_validating), which this does not need), and
    /// depends on `json-schema-rs` and `serde_json`. Off, such scalars are the plain Rust scalar
    /// (`String`, `u8`, `f64`), checked only by a self-validating parent's `validate` / `TryFrom`.
    pub constrained_newtypes: bool,
    /// Generate a builder for every struct (default `false`): `Name::builder()` returns
    /// `builder::Name`, whose setter per field takes any value that converts into the field's type
    /// (`TryInto`), and which builds the struct with `try_into()` (or `Name::try_from`). Building
    /// fails with a [`BuilderError`](crate::BuilderError) listing every required field left unset
    /// and every value that did not convert; a [`self_validating`](CodeGenSettings::self_validating)
    /// struct is then validated against its schema. A field left unset that is not required is
    /// absent. Each generated module gets a `builder` submodule holding its structs' builders.
    pub builders: bool,
    /// Whether every string enum gets `as_str`, `Display` and `FromStr` (default `true`), all
    /// through the enum's JSON names, via [`json_schema_rs::string_enum!`](crate::string_enum).
    /// They add no constraint and change no serialization; off, write your own.
    pub string_enum_impls: bool,
    /// Whether an item gets `impl Default` from the schema's `default` (default `false`): the
    /// item's own `default`, or for a struct, the object of its properties' `default`s when at
    /// least one property has one. The value is checked against the item's schema at generation
    /// time; one that does not satisfy it (or an item that has no schema location to check it
    /// against) gives no impl, and the item keeps its usual derives. The JSON spec makes
    /// `default` an annotation, so this is opt-in; the per-field `default` accessors are
    /// generated either way.
    pub schema_defaults: bool,
    /// Whether a typed `format` (`uuid`) gets its Rust type when the dialect only annotates
    /// formats (default [`FormatTypes::Typed`], a deliberate divergence from 2019-09 and
    /// 2020-12; see [`FormatTypes`]).
    pub format_types: FormatTypes,
    /// Derive macros every generated struct, enum and union gets on top of its own, as paths
    /// (`schemars::JsonSchema`); default none. A derive the type already has (matched by its
    /// last path segment, so `serde::Deserialize` matches the generated `Deserialize`) is not
    /// repeated. A plain type alias (`pub type`) takes none.
    pub extra_derives: Vec<String>,
    /// Attributes every generated struct, enum and union gets, each the text inside `#[...]`
    /// (`non_exhaustive`); default none. A plain type alias takes none.
    pub extra_attrs: Vec<String>,
    /// Changes to single generated types, keyed by the name the type would otherwise get: a new
    /// name, and derives and attributes for that type only (see [`TypePatch`]). Generating fails
    /// when a key names no generated type, or a new name is not a Rust identifier or clashes.
    pub patches: BTreeMap<String, TypePatch>,
    /// Subschemas generated as a type of the user's instead of a generated one, in order (the
    /// first match wins; see [`TypeConversion`]); default none. The subschema's type, and every
    /// type only it reaches, is not generated. The user's type must implement what the types
    /// holding it derive (`Debug`, `Clone`, `PartialEq`, `serde::Serialize`,
    /// `serde::Deserialize`). Self-validating parents still validate the JSON against the
    /// subschema before deserializing it into the user's type.
    pub conversions: Vec<TypeConversion>,
    /// Generated types replaced by a type of the user's, keyed by the name the type would
    /// otherwise get, valued by the user's type as a path (`my_crate::Ipv4`); default none.
    /// Every use of the type names the user's type instead, and the type, with every type only
    /// it reaches, is not generated. The user's type must implement what
    /// [`conversions`](CodeGenSettings::conversions) asks. Generating fails when a key names no
    /// generated type, names a schema's root type, or a value is not a Rust path.
    pub replacements: BTreeMap<String, String>,
    /// Crates the output depends on, by name (`my-types`), whose types an `x-rust-type`
    /// extension may name (see [`UnknownCrates`]); default none. A subschema whose
    /// `x-rust-type` names a listed crate at a version its requirement accepts is that type
    /// (`::my_types::Thing`, with its `parameters` generated as the type's generic arguments)
    /// instead of a generated one. The type must implement what the generated types derive, as
    /// a type generated by this library does. Generating fails when an extension the settings
    /// make count is malformed.
    pub crates: BTreeMap<String, CrateSpec>,
    /// What an `x-rust-type` naming a crate not in [`crates`](CodeGenSettings::crates) does
    /// (default [`UnknownCrates::Generate`]: ignored, as the specification ignores unknown
    /// keywords).
    pub unknown_crates: UnknownCrates,
}

impl CodeGenSettings {
    /// Whether generated types carry validation targets: self-validating types or constrained
    /// newtypes.
    pub(crate) fn validates(&self) -> bool {
        self.self_validating || self.constrained_newtypes
    }

    /// Whether `x-rust-type` extensions can change the output: a crate is listed or unknown
    /// crates do not generate.
    pub(crate) fn honours_rust_types(&self) -> bool {
        !self.crates.is_empty() || self.unknown_crates != UnknownCrates::Generate
    }
}

impl Default for CodeGenSettings {
    fn default() -> Self {
        Self {
            model_name_source: ModelNameSource::TitleFirst,
            dedupe_mode: DedupeMode::Full,
            unknown_properties: None,
            unique_items: UniqueItemsType::Vec,
            map_type: MapType::BTreeMap,
            flatten_single_entry_combinators: false,
            self_validating: true,
            constrained_newtypes: true,
            builders: false,
            string_enum_impls: true,
            schema_defaults: false,
            format_types: FormatTypes::Typed,
            extra_derives: Vec::new(),
            extra_attrs: Vec::new(),
            patches: BTreeMap::new(),
            conversions: Vec::new(),
            replacements: BTreeMap::new(),
            crates: BTreeMap::new(),
            unknown_crates: UnknownCrates::Generate,
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
    map_type: Option<MapType>,
    flatten_single_entry_combinators: Option<bool>,
    self_validating: Option<bool>,
    constrained_newtypes: Option<bool>,
    builders: Option<bool>,
    string_enum_impls: Option<bool>,
    schema_defaults: Option<bool>,
    format_types: Option<FormatTypes>,
    extra_derives: Vec<String>,
    extra_attrs: Vec<String>,
    patches: BTreeMap<String, TypePatch>,
    conversions: Vec<TypeConversion>,
    replacements: BTreeMap<String, String>,
    crates: BTreeMap<String, CrateSpec>,
    unknown_crates: Option<UnknownCrates>,
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

    /// Set the Rust map type for objects' string-keyed entries (see [`MapType`]).
    #[must_use]
    pub fn map_type(mut self, value: MapType) -> Self {
        self.map_type = Some(value);
        self
    }

    /// Generate a one-entry `anyOf` / `oneOf` as its entry's type (see
    /// [`CodeGenSettings::flatten_single_entry_combinators`]).
    #[must_use]
    pub fn flatten_single_entry_combinators(mut self, value: bool) -> Self {
        self.flatten_single_entry_combinators = Some(value);
        self
    }

    /// Generate types that validate themselves against their schema (see
    /// [`CodeGenSettings::self_validating`]).
    #[must_use]
    pub fn self_validating(mut self, value: bool) -> Self {
        self.self_validating = Some(value);
        self
    }

    /// Generate a builder for every struct (see [`CodeGenSettings::builders`]).
    #[must_use]
    pub fn builders(mut self, value: bool) -> Self {
        self.builders = Some(value);
        self
    }

    /// Give every string enum `as_str`, `Display` and `FromStr` (see
    /// [`CodeGenSettings::string_enum_impls`]).
    #[must_use]
    pub fn string_enum_impls(mut self, value: bool) -> Self {
        self.string_enum_impls = Some(value);
        self
    }

    /// Implement `Default` from the schema's `default` (see
    /// [`CodeGenSettings::schema_defaults`]).
    #[must_use]
    pub fn schema_defaults(mut self, value: bool) -> Self {
        self.schema_defaults = Some(value);
        self
    }

    /// Generate constrained newtypes for scalars whose schema asserts more than their Rust type
    /// holds (see [`CodeGenSettings::constrained_newtypes`]).
    #[must_use]
    pub fn constrained_newtypes(mut self, value: bool) -> Self {
        self.constrained_newtypes = Some(value);
        self
    }

    /// Choose when a typed `format` gets its Rust type (see [`FormatTypes`]).
    #[must_use]
    pub fn format_types(mut self, value: FormatTypes) -> Self {
        self.format_types = Some(value);
        self
    }

    /// Add derive macros to every generated type (see [`CodeGenSettings::extra_derives`]).
    #[must_use]
    pub fn extra_derives<I, S>(mut self, derives: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_derives
            .extend(derives.into_iter().map(Into::into));
        self
    }

    /// Add attributes to every generated type (see [`CodeGenSettings::extra_attrs`]).
    #[must_use]
    pub fn extra_attrs<I, S>(mut self, attrs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_attrs.extend(attrs.into_iter().map(Into::into));
        self
    }

    /// Patch the generated type named `type_name` (see [`CodeGenSettings::patches`]); a second
    /// patch for the same name replaces the first.
    #[must_use]
    pub fn patch(mut self, type_name: impl Into<String>, patch: TypePatch) -> Self {
        self.patches.insert(type_name.into(), patch);
        self
    }

    /// Generate subschemas equal to `schema` as `rust_type` (see
    /// [`CodeGenSettings::conversions`]); conversions added earlier match first.
    #[must_use]
    pub fn convert(mut self, schema: serde_json::Value, rust_type: impl Into<String>) -> Self {
        self.conversions.push(TypeConversion {
            schema,
            rust_type: rust_type.into(),
        });
        self
    }

    /// Replace the generated type named `type_name` with `rust_type` (see
    /// [`CodeGenSettings::replacements`]); a second replacement for the same name replaces the
    /// first.
    #[must_use]
    pub fn replace(mut self, type_name: impl Into<String>, rust_type: impl Into<String>) -> Self {
        self.replacements.insert(type_name.into(), rust_type.into());
        self
    }

    /// Let `x-rust-type` extensions naming the crate `name` use its types (see
    /// [`CodeGenSettings::crates`]); a second spec for the same crate replaces the first.
    #[must_use]
    pub fn external_crate(mut self, name: impl Into<String>, spec: CrateSpec) -> Self {
        self.crates.insert(name.into(), spec);
        self
    }

    /// Set what an `x-rust-type` naming a crate not listed does (see [`UnknownCrates`]).
    #[must_use]
    pub fn unknown_crates(mut self, value: UnknownCrates) -> Self {
        self.unknown_crates = Some(value);
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
            map_type: self.map_type.unwrap_or_default(),
            flatten_single_entry_combinators: self
                .flatten_single_entry_combinators
                .unwrap_or_default(),
            self_validating: self.self_validating.unwrap_or(true),
            constrained_newtypes: self.constrained_newtypes.unwrap_or(true),
            builders: self.builders.unwrap_or_default(),
            string_enum_impls: self.string_enum_impls.unwrap_or(true),
            schema_defaults: self.schema_defaults.unwrap_or_default(),
            format_types: self.format_types.unwrap_or_default(),
            extra_derives: self.extra_derives,
            extra_attrs: self.extra_attrs,
            patches: self.patches,
            conversions: self.conversions,
            replacements: self.replacements,
            crates: self.crates,
            unknown_crates: self.unknown_crates.unwrap_or_default(),
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

#[cfg(test)]
mod tests {
    use super::{CrateSpec, CrateVersion};

    #[test]
    fn crate_versions_parse_with_missing_numbers_as_zero() {
        let texts: [&str; 7] = ["1", "1.2", "1.2.3", "1.2-beta.1", "*", "!", "one"];
        let expected: Vec<Result<CrateVersion, String>> = vec![
            Ok(CrateVersion::Version(semver::Version::new(1, 0, 0))),
            Ok(CrateVersion::Version(semver::Version::new(1, 2, 0))),
            Ok(CrateVersion::Version(semver::Version::new(1, 2, 3))),
            Ok(CrateVersion::Version(
                semver::Version::parse("1.2.0-beta.1").unwrap(),
            )),
            Ok(CrateVersion::Any),
            Ok(CrateVersion::Never),
            Err("`one` is not a crate version (`1.2.3`, `*` or `!`): unexpected character 'o' while parsing major version number".to_string()),
        ];
        let actual: Vec<Result<CrateVersion, String>> =
            texts.iter().map(|text: &&str| text.parse()).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_specs_parse_with_and_without_a_rename() {
        let texts: [&str; 6] = [
            "my-types@1.2",
            "types_v1=my-types@*",
            "my-types",
            "1x@1",
            "a=b=c@1",
            "my-types@x",
        ];
        let spec = |version: CrateVersion, rename: Option<&str>| CrateSpec {
            version,
            rename: rename.map(str::to_string),
        };
        let expected: Vec<Result<(String, CrateSpec), String>> = vec![
            Ok((
                "my-types".to_string(),
                spec(CrateVersion::Version(semver::Version::new(1, 2, 0)), None),
            )),
            Ok((
                "my-types".to_string(),
                spec(CrateVersion::Any, Some("types_v1")),
            )),
            Err("crate `my-types` is not of the form `[rename=]crate@version`".to_string()),
            Err("crate `1x@1`: `1x` is not a crate name".to_string()),
            Err("crate `a=b=c@1`: `b=c` is not a crate name".to_string()),
            Err("crate `my-types@x`: `x` is not a crate version (`1.2.3`, `*` or `!`): unexpected character 'x' while parsing major version number".to_string()),
        ];
        let actual: Vec<Result<(String, CrateSpec), String>> = texts
            .iter()
            .map(|text: &&str| CrateSpec::parse_named(text))
            .collect();
        assert_eq!(expected, actual);
    }
}
