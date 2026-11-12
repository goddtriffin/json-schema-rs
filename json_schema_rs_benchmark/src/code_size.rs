//! Generated code size (#113): how much Rust our codegen generates for each fixture, against the
//! schema it came from.
//!
//! Generated size drives the user's compile time and binary size, so the benchmark tracks it
//! next to time and memory. It is deterministic (the same schema and codegen give the same
//! output), so every change counts: there is no noise to filter.
//!
//! The driver generates each fixture with the CLI (`jsonschemars generate rust`) twice, with
//! default settings and with constrained newtypes off ([`Settings`]), and the `code_size` binary
//! measures every generated `.rs` file with [`measure_source`]. Both sides of a quick run are
//! measured by the working tree's `code_size`, so a change to this classification never shows up
//! as a change in generated size.
//!
//! [`measure_source`] classifies each line of our generated code ([`Category`]): doc comments and
//! attribute lines by their own syntax, every other line by the top-level item it belongs to (a
//! line starting in column 0 opens an item; indented lines and closing braces continue it).

use crate::{TsvParse, escape_cell, exact_fields, parse_tsv};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// The codegen settings a fixture is generated with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Settings {
    /// Default settings.
    Default,
    /// Default settings with `constrained_newtypes(false)` (`--cgs-constrained-newtypes false`).
    NoNewtypes,
}

impl Settings {
    /// Every setting, in report order.
    pub const ALL: [Self; 2] = [Self::Default, Self::NoNewtypes];

    /// The setting's token in captures and directory names.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::NoNewtypes => "no-newtypes",
        }
    }

    /// The setting as reports name it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::NoNewtypes => "no constrained newtypes",
        }
    }

    /// The setting a token names.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|settings: &Self| settings.token() == token)
    }
}

/// What a line of generated code is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// Struct and enum definitions (`pub struct`, `pub enum`, `pub type`), their fields and
    /// variants.
    TypeDefinitions,
    /// Serde implementations: `impl Serialize` / `impl Deserialize` blocks,
    /// `deserialize_untagged!`, `deserialize_union!`, and serde helper functions.
    SerdeImpls,
    /// Validation implementations: `impl TryFrom<serde_json::Value>`, `self_validating!` and
    /// `constrained_newtype!`.
    ValidationImpls,
    /// Inherent `impl Type { … }` blocks: default accessors (and `validate()`, before
    /// `self_validating!`).
    InherentImpls,
    /// The schema embedded for self-validation (the `TYPE_VALIDATORS` static).
    EmbeddedSchema,
    /// Doc comments (`///`, `//!`).
    Docs,
    /// Attribute lines (`#[…]`): derives, serde and schema attributes.
    Attributes,
    /// Everything else: `use` and `mod` lines, blank lines, any other item.
    Other,
}

impl Category {
    /// Every category, in report order.
    pub const ALL: [Self; 8] = [
        Self::TypeDefinitions,
        Self::SerdeImpls,
        Self::ValidationImpls,
        Self::InherentImpls,
        Self::EmbeddedSchema,
        Self::Docs,
        Self::Attributes,
        Self::Other,
    ];

    /// The category's token in captures.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::TypeDefinitions => "type-definitions",
            Self::SerdeImpls => "serde-impls",
            Self::ValidationImpls => "validation-impls",
            Self::InherentImpls => "inherent-impls",
            Self::EmbeddedSchema => "embedded-schema",
            Self::Docs => "docs",
            Self::Attributes => "attributes",
            Self::Other => "other",
        }
    }

    /// The category as reports name it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TypeDefinitions => "type definitions",
            Self::SerdeImpls => "serde impls",
            Self::ValidationImpls => "validation impls",
            Self::InherentImpls => "inherent impls",
            Self::EmbeddedSchema => "embedded schema",
            Self::Docs => "docs",
            Self::Attributes => "attributes",
            Self::Other => "other",
        }
    }
}

/// The kind of a generated type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeKind {
    /// A struct with named fields.
    Struct,
    /// A tuple struct wrapping one value (a constrained newtype).
    Newtype,
    /// An enum.
    Enum,
    /// A type alias (`pub type`).
    Alias,
}

impl TypeKind {
    /// Every kind, in report order.
    pub const ALL: [Self; 4] = [Self::Struct, Self::Newtype, Self::Enum, Self::Alias];

    /// The kind's token in captures.
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Struct => "struct",
            Self::Newtype => "newtype",
            Self::Enum => "enum",
            Self::Alias => "alias",
        }
    }

    /// The kind as reports name it (plural).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Struct => "structs",
            Self::Newtype => "newtypes",
            Self::Enum => "enums",
            Self::Alias => "aliases",
        }
    }
}

/// One measured quantity of a fixture's generated code (or of its schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SizeMetric {
    /// Lines of generated code.
    Lines,
    /// Bytes of generated code.
    Bytes,
    /// Generated types of one kind.
    Types(TypeKind),
    /// Lines of generated code of one category.
    CategoryLines(Category),
    /// Bytes of generated code of one category.
    CategoryBytes(Category),
    /// Lines of the schema and its resources, pretty-printed (so formatting does not count).
    SchemaLines,
    /// Bytes of the schema and its resource files, as stored.
    SchemaBytes,
}

impl SizeMetric {
    /// The metric's token in captures (`lines`, `types.struct`, `lines.docs`, `schema.bytes`).
    #[must_use]
    pub fn token(self) -> String {
        match self {
            Self::Lines => "lines".to_owned(),
            Self::Bytes => "bytes".to_owned(),
            Self::Types(kind) => format!("types.{}", kind.token()),
            Self::CategoryLines(category) => format!("lines.{}", category.token()),
            Self::CategoryBytes(category) => format!("bytes.{}", category.token()),
            Self::SchemaLines => "schema.lines".to_owned(),
            Self::SchemaBytes => "schema.bytes".to_owned(),
        }
    }

    /// The metric as reports name it.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Lines => "lines".to_owned(),
            Self::Bytes => "bytes".to_owned(),
            Self::Types(kind) => kind.label().to_owned(),
            Self::CategoryLines(category) => format!("lines: {}", category.label()),
            Self::CategoryBytes(category) => format!("bytes: {}", category.label()),
            Self::SchemaLines => "schema lines".to_owned(),
            Self::SchemaBytes => "schema bytes".to_owned(),
        }
    }

    /// The metric a token names.
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "lines" => return Some(Self::Lines),
            "bytes" => return Some(Self::Bytes),
            "schema.lines" => return Some(Self::SchemaLines),
            "schema.bytes" => return Some(Self::SchemaBytes),
            _ => {}
        }
        let (prefix, name) = token.split_once('.')?;
        let category = || -> Option<Category> {
            Category::ALL
                .into_iter()
                .find(|category: &Category| category.token() == name)
        };
        match prefix {
            "types" => TypeKind::ALL
                .into_iter()
                .find(|kind: &TypeKind| kind.token() == name)
                .map(Self::Types),
            "lines" => category().map(Self::CategoryLines),
            "bytes" => category().map(Self::CategoryBytes),
            _ => None,
        }
    }
}

impl Serialize for SizeMetric {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.token())
    }
}

impl<'de> Deserialize<'de> for SizeMetric {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token: String = String::deserialize(deserializer)?;
        Self::from_token(&token)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown size metric `{token}`")))
    }
}

/// Every metric of one fixture's generated code, with its value.
pub type CodeSize = BTreeMap<SizeMetric, u64>;

/// Add `amount` to `metric` of `size`.
fn add(size: &mut CodeSize, metric: SizeMetric, amount: u64) {
    *size.entry(metric).or_insert(0) += amount;
}

/// A count as the `u64` the metrics hold.
fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Measure one generated source file into `size`: its lines and bytes, in total and by
/// [`Category`], and its types by [`TypeKind`]. A line's bytes include its newline.
pub fn measure_source(source: &str, size: &mut CodeSize) {
    let mut item: Category = Category::Other;
    for line in source.lines() {
        let trimmed: &str = line.trim_start();
        let category: Category = if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            Category::Docs
        } else if trimmed.starts_with("#[") || trimmed.starts_with("#![") {
            Category::Attributes
        } else if trimmed.is_empty() {
            Category::Other
        } else if opens_item(line) {
            let (category, kind): (Category, Option<TypeKind>) = classify_item(line);
            if let Some(kind) = kind {
                add(size, SizeMetric::Types(kind), 1);
            }
            item = category;
            category
        } else {
            item
        };
        let bytes: u64 = count(line.len() + 1);
        add(size, SizeMetric::Lines, 1);
        add(size, SizeMetric::Bytes, bytes);
        add(size, SizeMetric::CategoryLines(category), 1);
        add(size, SizeMetric::CategoryBytes(category), bytes);
    }
}

/// Whether a (non-blank, non-doc, non-attribute) line opens a top-level item: it starts in
/// column 0 and is not the continuation of one (a closing bracket or a `where` clause).
fn opens_item(line: &str) -> bool {
    !line.starts_with(char::is_whitespace)
        && !line.starts_with(['}', ')', ']', '{'])
        && !line.starts_with("where")
}

/// The category of a top-level item from its first line, and the kind of type it defines.
fn classify_item(line: &str) -> (Category, Option<TypeKind>) {
    if let Some(rest) = line.strip_prefix("pub struct ") {
        let after_name: &str = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
        let kind: TypeKind = if after_name.starts_with('(') {
            TypeKind::Newtype
        } else {
            TypeKind::Struct
        };
        return (Category::TypeDefinitions, Some(kind));
    }
    if line.starts_with("pub enum ") {
        return (Category::TypeDefinitions, Some(TypeKind::Enum));
    }
    if line.starts_with("pub type ") {
        return (Category::TypeDefinitions, Some(TypeKind::Alias));
    }
    let category: Category = if line.starts_with("impl") {
        if line.contains("TryFrom<serde_json::Value>") {
            Category::ValidationImpls
        } else if line.contains("Deserialize<") || line.contains("Serialize for ") {
            Category::SerdeImpls
        } else if line.contains(" for ") {
            Category::Other
        } else {
            Category::InherentImpls
        }
    } else if line.starts_with("json_schema_rs::constrained_newtype!")
        || line.starts_with("json_schema_rs::self_validating!")
    {
        Category::ValidationImpls
    } else if line.starts_with("json_schema_rs::deserialize_untagged!")
        || line.starts_with("json_schema_rs::deserialize_union!")
        || line.starts_with("fn deserialize")
        || line.starts_with("fn serialize")
    {
        Category::SerdeImpls
    } else if line.starts_with("static TYPE_VALIDATORS") {
        Category::EmbeddedSchema
    } else {
        Category::Other
    };
    (category, None)
}

/// Measure a fixture's schema and resource documents into `size`: their bytes as stored, and
/// their lines pretty-printed (a document that is not JSON counts its lines as stored).
pub fn measure_schema<'a>(documents: impl IntoIterator<Item = &'a str>, size: &mut CodeSize) {
    for document in documents {
        let lines: usize = match serde_json::from_str::<serde_json::Value>(document) {
            Ok(value) => serde_json::to_string_pretty(&value).map_or_else(
                |_| document.lines().count(),
                |pretty| pretty.lines().count(),
            ),
            Err(_) => document.lines().count(),
        };
        add(size, SizeMetric::SchemaLines, count(lines));
        add(size, SizeMetric::SchemaBytes, count(document.len()));
    }
}

/// One metric of one fixture's generated code, as captured (`code_size.tsv`) and committed
/// (`results.json`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CodeSizeRow {
    /// The fixture's id.
    pub fixture: String,
    /// The settings it was generated with.
    pub settings: Settings,
    /// The metric.
    pub metric: SizeMetric,
    /// The metric's value.
    pub value: u64,
}

/// The rows of a capture, one fixture and setting's [`CodeSize`] per line
/// (`fixture\tsettings\tmetric\tvalue`), sorted.
#[must_use]
pub fn code_size_rows(fixture: &str, settings: Settings, size: &CodeSize) -> Vec<CodeSizeRow> {
    size.iter()
        .map(|(metric, value)| CodeSizeRow {
            fixture: fixture.to_owned(),
            settings,
            metric: *metric,
            value: *value,
        })
        .collect()
}

/// Render rows as `code_size.tsv` lines.
#[must_use]
pub fn code_size_tsv(rows: &[CodeSizeRow]) -> String {
    let mut out: String = String::new();
    for row in rows {
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}",
            row.fixture,
            row.settings.token(),
            row.metric.token(),
            row.value
        );
    }
    out
}

/// Parse `code_size.tsv` (`fixture\tsettings\tmetric\tvalue`).
#[must_use]
pub fn parse_code_size_tsv(text: &str) -> TsvParse<CodeSizeRow> {
    parse_tsv(text, |line: &str| {
        let fields: Vec<&str> = exact_fields(line, 4)?;
        let settings: Settings = Settings::from_token(fields[1])
            .ok_or_else(|| format!("unknown settings `{}`", fields[1]))?;
        let metric: SizeMetric = SizeMetric::from_token(fields[2])
            .ok_or_else(|| format!("unknown size metric `{}`", fields[2]))?;
        let value: u64 = fields[3]
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("value `{}` is not a non-negative integer", fields[3]))?;
        Ok(CodeSizeRow {
            fixture: fields[0].to_owned(),
            settings,
            metric,
            value,
        })
    })
}

/// Each fixture and setting's [`CodeSize`].
pub type CodeSizes = BTreeMap<(String, Settings), CodeSize>;

/// Group rows by fixture and setting.
#[must_use]
pub fn code_sizes(rows: &[CodeSizeRow]) -> CodeSizes {
    let mut sizes: CodeSizes = CodeSizes::new();
    for row in rows {
        sizes
            .entry((row.fixture.clone(), row.settings))
            .or_default()
            .insert(row.metric, row.value);
    }
    sizes
}

/// One metric of one fixture and setting that differs between the two sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSizeChange {
    /// The fixture's id.
    pub fixture: String,
    /// The settings.
    pub settings: Settings,
    /// The metric.
    pub metric: SizeMetric,
    /// The base side's value (zero when the base has none).
    pub base: u64,
    /// The head side's value (zero when the head has none).
    pub head: u64,
}

/// How the two sides' generated code compares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeSizeComparison {
    /// Every generated-code metric that differs, by fixture, setting and metric. The schema's
    /// own metrics are left out (both sides read the same fixtures).
    pub changes: Vec<CodeSizeChange>,
    /// Fixture and setting pairs measured on both sides.
    pub compared: usize,
    /// Pairs measured only on the base side (the head's codegen failed on them).
    pub base_only: Vec<(String, Settings)>,
    /// Pairs measured only on the head side.
    pub head_only: Vec<(String, Settings)>,
}

/// Compare both sides' generated code: every change counts, since generated code is
/// deterministic.
#[must_use]
pub fn compare_code_sizes(base: &CodeSizes, head: &CodeSizes) -> CodeSizeComparison {
    let mut comparison: CodeSizeComparison = CodeSizeComparison::default();
    let keys: BTreeSet<&(String, Settings)> = base.keys().chain(head.keys()).collect();
    for key in keys {
        let (Some(base_size), Some(head_size)) = (base.get(key), head.get(key)) else {
            if base.contains_key(key) {
                comparison.base_only.push(key.clone());
            } else {
                comparison.head_only.push(key.clone());
            }
            continue;
        };
        comparison.compared += 1;
        let metrics: BTreeSet<&SizeMetric> = base_size.keys().chain(head_size.keys()).collect();
        for metric in metrics {
            if matches!(metric, SizeMetric::SchemaLines | SizeMetric::SchemaBytes) {
                continue;
            }
            let base_value: u64 = base_size.get(metric).copied().unwrap_or(0);
            let head_value: u64 = head_size.get(metric).copied().unwrap_or(0);
            if base_value != head_value {
                comparison.changes.push(CodeSizeChange {
                    fixture: key.0.clone(),
                    settings: key.1,
                    metric: *metric,
                    base: base_value,
                    head: head_value,
                });
            }
        }
    }
    comparison
}

/// `value` minus `base`, signed (`+4`, `-120`).
fn delta(base: u64, value: u64) -> String {
    format!("{:+}", i128::from(value) - i128::from(base))
}

/// `value` relative to `base` as a signed percentage (`+12.5%`), or `new` when `base` is zero.
fn change(base: u64, value: u64) -> String {
    if base == 0 {
        return "new".to_owned();
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "generated sizes are far below 2^52, and the change is shown to 0.1%"
    )]
    let relative: f64 = (value as f64 - base as f64) / base as f64 * 100.0;
    format!("{relative:+.1}%")
}

impl CodeSizeComparison {
    /// The PR body's *Generated code* part of the *Performance* section: a one-line summary
    /// (total lines and bytes over every fixture, default settings), then every change in a
    /// collapsed table, and the pairs measured on one side only.
    #[must_use]
    pub fn to_markdown(&self, base: &CodeSizes, head: &CodeSizes, base_label: &str) -> String {
        let mut out: String = String::new();
        if self.changes.is_empty() {
            let _ = writeln!(
                out,
                "Generated code: unchanged ({} fixture and settings pairs compared against \
                 `{base_label}`).",
                self.compared
            );
        } else {
            let total = |sizes: &CodeSizes, metric: SizeMetric| -> u64 {
                sizes
                    .iter()
                    .filter(|((fixture, settings), _)| {
                        *settings == Settings::Default
                            && base.contains_key(&(fixture.clone(), *settings))
                            && head.contains_key(&(fixture.clone(), *settings))
                    })
                    .filter_map(|(_, size)| size.get(&metric))
                    .sum()
            };
            let changed: BTreeSet<(&str, Settings)> = self
                .changes
                .iter()
                .map(|change: &CodeSizeChange| (change.fixture.as_str(), change.settings))
                .collect();
            let (base_lines, head_lines): (u64, u64) = (
                total(base, SizeMetric::Lines),
                total(head, SizeMetric::Lines),
            );
            let (base_bytes, head_bytes): (u64, u64) = (
                total(base, SizeMetric::Bytes),
                total(head, SizeMetric::Bytes),
            );
            let _ = write!(
                out,
                "Generated code changed on {} of {} fixture and settings pairs against \
                 `{base_label}`. Every fixture, default settings: {base_lines} → {head_lines} \
                 lines ({}, {}), {base_bytes} → {head_bytes} bytes ({}, {}).\n\n\
                 <details><summary>Generated code ({} changes)</summary>\n\n\
                 | Fixture | Settings | Metric | `{base_label}` | This PR | Change |\n\
                 |---|---|---|---|---|---|\n",
                changed.len(),
                self.compared,
                delta(base_lines, head_lines),
                change(base_lines, head_lines),
                delta(base_bytes, head_bytes),
                change(base_bytes, head_bytes),
                self.changes.len()
            );
            for change_row in &self.changes {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_cell(&change_row.fixture),
                    change_row.settings.label(),
                    change_row.metric.label(),
                    change_row.base,
                    change_row.head,
                    change(change_row.base, change_row.head)
                );
            }
            out.push_str("\n</details>\n");
        }
        for (pairs, side) in [
            (&self.base_only, format!("only on `{base_label}`")),
            (&self.head_only, "only on this branch".to_owned()),
        ] {
            if pairs.is_empty() {
                continue;
            }
            let _ = write!(out, "\nGenerated code measured {side} ({}):", pairs.len());
            for (fixture, settings) in pairs {
                let _ = write!(out, " {fixture} ({});", settings.label());
            }
            out.pop();
            out.push_str(".\n");
        }
        out
    }
}

/// `text` with its first letter in upper case, for a table header.
fn capitalized(text: &str) -> String {
    let mut chars: std::str::Chars<'_> = text.chars();
    chars.next().map_or_else(String::new, |first: char| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A size in KiB with one decimal.
fn kib(bytes: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "generated sizes are far below 2^52, and the size is shown to 0.1 KiB"
    )]
    let kib: f64 = bytes as f64 / 1024.0;
    format!("{kib:.1}")
}

/// `value` over `base` as a ratio with one decimal (`2.4×`), or `–` when `base` is zero.
fn ratio(value: u64, base: u64) -> String {
    if base == 0 {
        return "–".to_owned();
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "generated sizes are far below 2^52, and the ratio is shown to 0.1"
    )]
    let ratio: f64 = value as f64 / base as f64;
    format!("{ratio:.1}×")
}

/// The committed report's *Generated code* section (`results.md`): per setting, one table of
/// sizes and types against the schema, and one of lines (KiB) by category.
#[must_use]
pub fn code_size_markdown(sizes: &CodeSizes) -> String {
    if sizes.is_empty() {
        return "\nNot measured.\n".to_owned();
    }
    let mut out: String = String::new();
    for settings in Settings::ALL {
        let rows: Vec<(&String, &CodeSize)> = sizes
            .iter()
            .filter(|((_, row_settings), _)| *row_settings == settings)
            .map(|((fixture, _), size)| (fixture, size))
            .collect();
        if rows.is_empty() {
            continue;
        }
        let value = |size: &CodeSize, metric: SizeMetric| -> u64 {
            size.get(&metric).copied().unwrap_or(0)
        };
        let _ = write!(
            out,
            "\n### Size, {} settings\n\n\
             | Fixture | Schema lines | Lines | × schema | KiB | × schema |",
            settings.label()
        );
        for kind in TypeKind::ALL {
            let _ = write!(out, " {} |", capitalized(kind.label()));
        }
        out.push_str("\n|---|---|---|---|---|---|---|---|---|---|\n");
        for (fixture, size) in &rows {
            let lines: u64 = value(size, SizeMetric::Lines);
            let bytes: u64 = value(size, SizeMetric::Bytes);
            let _ = write!(
                out,
                "| {} | {} | {lines} | {} | {} | {} |",
                escape_cell(fixture),
                value(size, SizeMetric::SchemaLines),
                ratio(lines, value(size, SizeMetric::SchemaLines)),
                kib(bytes),
                ratio(bytes, value(size, SizeMetric::SchemaBytes))
            );
            for kind in TypeKind::ALL {
                let _ = write!(out, " {} |", value(size, SizeMetric::Types(kind)));
            }
            out.push('\n');
        }
        let _ = write!(
            out,
            "\n### Lines (KiB) by category, {} settings\n\n| Fixture |",
            settings.label()
        );
        for category in Category::ALL {
            let _ = write!(out, " {} |", capitalized(category.label()));
        }
        out.push_str("\n|---|");
        for _ in Category::ALL {
            out.push_str("---|");
        }
        out.push('\n');
        for (fixture, size) in &rows {
            let _ = write!(out, "| {} |", escape_cell(fixture));
            for category in Category::ALL {
                let _ = write!(
                    out,
                    " {} ({}) |",
                    value(size, SizeMetric::CategoryLines(category)),
                    kib(value(size, SizeMetric::CategoryBytes(category)))
                );
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        Category, CodeSize, CodeSizeChange, CodeSizeComparison, CodeSizeRow, CodeSizes, Settings,
        SizeMetric, TypeKind, code_size_markdown, compare_code_sizes, measure_schema,
        measure_source, parse_code_size_tsv,
    };
    use crate::{SkippedLine, TsvParse};

    fn size(metrics: &[(SizeMetric, u64)]) -> CodeSize {
        metrics.iter().copied().collect()
    }

    fn measured(source: &str) -> CodeSize {
        let mut actual: CodeSize = CodeSize::new();
        measure_source(source, &mut actual);
        actual
    }

    /// Lines and bytes of `lines` lines of `bytes` bytes (newlines included) in `category`.
    fn lines_in(category: Category, lines: u64, bytes: u64) -> [(SizeMetric, u64); 2] {
        [
            (SizeMetric::CategoryLines(category), lines),
            (SizeMetric::CategoryBytes(category), bytes),
        ]
    }

    #[test]
    fn measure_source_splits_a_struct_into_definition_docs_and_attributes() {
        let source: &str = "/// A.\n#[derive(Debug)]\npub struct A {\n    /// F.\n    #[serde(default)]\n    pub f: u8,\n}\n";
        let mut expected: CodeSize = size(&[
            (SizeMetric::Lines, 7),
            (SizeMetric::Bytes, 89),
            (SizeMetric::Types(TypeKind::Struct), 1),
        ]);
        expected.extend(lines_in(Category::TypeDefinitions, 3, 32));
        expected.extend(lines_in(Category::Docs, 2, 18));
        expected.extend(lines_in(Category::Attributes, 2, 39));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_source_counts_each_type_kind() {
        let source: &str =
            "pub struct S {\n}\npub struct N(String);\npub enum E {\n}\npub type T = u8;\n";
        let mut expected: CodeSize = size(&[
            (SizeMetric::Lines, 6),
            (SizeMetric::Bytes, 71),
            (SizeMetric::Types(TypeKind::Struct), 1),
            (SizeMetric::Types(TypeKind::Newtype), 1),
            (SizeMetric::Types(TypeKind::Enum), 1),
            (SizeMetric::Types(TypeKind::Alias), 1),
        ]);
        expected.extend(lines_in(Category::TypeDefinitions, 6, 71));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_source_classifies_impl_blocks_by_trait() {
        let source: &str = "impl A {\n}\nimpl TryFrom<serde_json::Value> for A {\n}\nimpl<'de> Deserialize<'de> for A {\n}\nimpl Serialize for A {\n}\nimpl Default for A {\n}\n";
        let mut expected: CodeSize = size(&[(SizeMetric::Lines, 10), (SizeMetric::Bytes, 138)]);
        expected.extend(lines_in(Category::InherentImpls, 2, 11));
        expected.extend(lines_in(Category::ValidationImpls, 2, 42));
        expected.extend(lines_in(Category::SerdeImpls, 4, 62));
        expected.extend(lines_in(Category::Other, 2, 23));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_source_classifies_deserialize_union_as_serde_impls() {
        let source: &str = "json_schema_rs::deserialize_union!(U, V, OneOf, Some(0), [1], |value| {\n    _ => A::deserialize(value).map(Self::A),\n});\n";
        let mut expected: CodeSize = size(&[(SizeMetric::Lines, 3), (SizeMetric::Bytes, 121)]);
        expected.extend(lines_in(Category::SerdeImpls, 3, 121));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_source_classifies_self_validating_as_validation_impls() {
        let source: &str = "json_schema_rs::self_validating!(A, V, 0);\n";
        let mut expected: CodeSize = size(&[(SizeMetric::Lines, 1), (SizeMetric::Bytes, 43)]);
        expected.extend(lines_in(Category::ValidationImpls, 1, 43));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_source_classifies_macros_statics_helpers_and_blank_lines() {
        let source: &str = "use serde::Deserialize;\n\njson_schema_rs::constrained_newtype!(string, N, V, 0);\njson_schema_rs::deserialize_untagged!(U);\nfn deserialize_some<D>(d: D)\nwhere\n    D: X,\n{\n}\nstatic TYPE_VALIDATORS: V = V::new(\n    &[],\n);\n";
        let mut expected: CodeSize = size(&[(SizeMetric::Lines, 12), (SizeMetric::Bytes, 219)]);
        expected.extend(lines_in(Category::Other, 2, 25));
        expected.extend(lines_in(Category::ValidationImpls, 1, 55));
        expected.extend(lines_in(Category::SerdeImpls, 6, 91));
        expected.extend(lines_in(Category::EmbeddedSchema, 3, 48));
        let actual: CodeSize = measured(source);
        assert_eq!(expected, actual);
    }

    #[test]
    fn measure_schema_counts_pretty_printed_lines_and_stored_bytes() {
        let mut actual: CodeSize = CodeSize::new();
        measure_schema(
            [r#"{"type":"object","required":["a"]}"#, "not json\n"],
            &mut actual,
        );
        let expected: CodeSize =
            size(&[(SizeMetric::SchemaLines, 7), (SizeMetric::SchemaBytes, 43)]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn size_metric_tokens_round_trip() {
        let mut expected: Vec<SizeMetric> = vec![
            SizeMetric::Lines,
            SizeMetric::Bytes,
            SizeMetric::SchemaLines,
            SizeMetric::SchemaBytes,
        ];
        expected.extend(TypeKind::ALL.map(SizeMetric::Types));
        expected.extend(Category::ALL.map(SizeMetric::CategoryLines));
        expected.extend(Category::ALL.map(SizeMetric::CategoryBytes));
        let actual: Vec<SizeMetric> = expected
            .iter()
            .filter_map(|metric: &SizeMetric| SizeMetric::from_token(&metric.token()))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_code_size_tsv_reads_rows_and_skips_malformed_lines() {
        let text: &str = "small/a\tdefault\tlines.docs\t12\nsmall/a\tother\tlines\t1\nsmall/a\tno-newtypes\tlines\tx\n";
        let expected: TsvParse<CodeSizeRow> = TsvParse {
            rows: vec![CodeSizeRow {
                fixture: "small/a".to_owned(),
                settings: Settings::Default,
                metric: SizeMetric::CategoryLines(Category::Docs),
                value: 12,
            }],
            skipped: vec![
                SkippedLine {
                    line: 2,
                    reason: "unknown settings `other`".to_owned(),
                },
                SkippedLine {
                    line: 3,
                    reason: "value `x` is not a non-negative integer".to_owned(),
                },
            ],
        };
        let actual: TsvParse<CodeSizeRow> = parse_code_size_tsv(text);
        assert_eq!(expected, actual);
    }

    /// One fixture and setting's metrics, as a test writes them.
    type Entry<'a> = (&'a str, Settings, &'a [(SizeMetric, u64)]);

    fn sizes(entries: &[Entry<'_>]) -> CodeSizes {
        entries
            .iter()
            .map(|(fixture, settings, metrics)| (((*fixture).to_owned(), *settings), size(metrics)))
            .collect()
    }

    #[test]
    fn compare_code_sizes_lists_every_changed_metric_and_one_sided_pairs() {
        let base: CodeSizes = sizes(&[
            (
                "small/a",
                Settings::Default,
                &[
                    (SizeMetric::Lines, 10),
                    (SizeMetric::Bytes, 100),
                    (SizeMetric::SchemaLines, 5),
                ],
            ),
            ("small/b", Settings::Default, &[(SizeMetric::Lines, 3)]),
        ]);
        let head: CodeSizes = sizes(&[
            (
                "small/a",
                Settings::Default,
                &[
                    (SizeMetric::Lines, 12),
                    (SizeMetric::Bytes, 100),
                    (SizeMetric::Types(TypeKind::Enum), 1),
                    (SizeMetric::SchemaLines, 6),
                ],
            ),
            ("small/a", Settings::NoNewtypes, &[(SizeMetric::Lines, 9)]),
        ]);
        let expected: CodeSizeComparison = CodeSizeComparison {
            changes: vec![
                CodeSizeChange {
                    fixture: "small/a".to_owned(),
                    settings: Settings::Default,
                    metric: SizeMetric::Lines,
                    base: 10,
                    head: 12,
                },
                CodeSizeChange {
                    fixture: "small/a".to_owned(),
                    settings: Settings::Default,
                    metric: SizeMetric::Types(TypeKind::Enum),
                    base: 0,
                    head: 1,
                },
            ],
            compared: 1,
            base_only: vec![("small/b".to_owned(), Settings::Default)],
            head_only: vec![("small/a".to_owned(), Settings::NoNewtypes)],
        };
        let actual: CodeSizeComparison = compare_code_sizes(&base, &head);
        assert_eq!(expected, actual);
    }

    #[test]
    fn code_size_comparison_markdown_says_unchanged() {
        let both: CodeSizes = sizes(&[("small/a", Settings::Default, &[(SizeMetric::Lines, 10)])]);
        let expected: &str =
            "Generated code: unchanged (1 fixture and settings pairs compared against `main`).\n";
        let actual: String = compare_code_sizes(&both, &both).to_markdown(&both, &both, "main");
        assert_eq!(expected, actual);
    }

    #[test]
    fn code_size_comparison_markdown_totals_and_tables_the_changes() {
        let base: CodeSizes = sizes(&[
            (
                "small/a",
                Settings::Default,
                &[(SizeMetric::Lines, 10), (SizeMetric::Bytes, 400)],
            ),
            (
                "small/b",
                Settings::Default,
                &[(SizeMetric::Lines, 30), (SizeMetric::Bytes, 600)],
            ),
            ("small/b", Settings::NoNewtypes, &[(SizeMetric::Lines, 20)]),
        ]);
        let head: CodeSizes = sizes(&[
            (
                "small/a",
                Settings::Default,
                &[(SizeMetric::Lines, 14), (SizeMetric::Bytes, 400)],
            ),
            (
                "small/b",
                Settings::Default,
                &[(SizeMetric::Lines, 30), (SizeMetric::Bytes, 600)],
            ),
        ]);
        let expected: &str = "Generated code changed on 1 of 2 fixture and settings pairs \
            against `main`. Every fixture, default settings: 40 → 44 lines (+4, +10.0%), \
            1000 → 1000 bytes (+0, +0.0%).\n\n\
            <details><summary>Generated code (1 changes)</summary>\n\n\
            | Fixture | Settings | Metric | `main` | This PR | Change |\n\
            |---|---|---|---|---|---|\n\
            | small/a | default | lines | 10 | 14 | +40.0% |\n\n\
            </details>\n\n\
            Generated code measured only on `main` (1): small/b (no constrained newtypes).\n";
        let actual: String = compare_code_sizes(&base, &head).to_markdown(&base, &head, "main");
        assert_eq!(expected, actual);
    }

    #[test]
    fn code_size_markdown_without_rows_says_not_measured() {
        let expected: &str = "\nNot measured.\n";
        let actual: String = code_size_markdown(&CodeSizes::new());
        assert_eq!(expected, actual);
    }
}
