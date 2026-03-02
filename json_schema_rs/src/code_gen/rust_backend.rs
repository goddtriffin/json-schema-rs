//! Rust codegen backend: emits serde-compatible Rust structs from JSON Schema.

use super::CodeGenBackend;
use super::CodeGenError;
use super::CodeGenResult;
use super::GenerateRustOutput;
use super::settings::{CodeGenSettings, DedupeMode, ModelNameSource};
use crate::json_schema::JsonSchema;
use crate::json_schema::json_schema::AdditionalProperties;
use crate::json_schema::ref_resolver;
use crate::sanitizers::{
    enum_variant_names_with_collision_resolution, sanitize_field_name, sanitize_struct_name,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

/// Backend that emits Rust structs (serde-compatible).
#[derive(Debug, Clone, Default)]
pub struct RustBackend;

impl CodeGenBackend for RustBackend {
    fn generate(
        &self,
        schemas: &[JsonSchema],
        settings: &CodeGenSettings,
    ) -> CodeGenResult<GenerateRustOutput> {
        match settings.dedupe_mode {
            DedupeMode::Disabled => {
                let mut per_schema: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
                for (index, schema) in schemas.iter().enumerate() {
                    let mut out = Cursor::new(Vec::new());
                    emit_rust(schema, &mut out, settings).map_err(|e| CodeGenError::Batch {
                        index,
                        source: Box::new(e),
                    })?;
                    per_schema.push({
                        let result = maybe_prepend_btreemap_use(out.into_inner());
                        let result = maybe_prepend_hash_set_use(result);
                        #[cfg(feature = "uuid")]
                        let result = maybe_prepend_uuid_use(result);
                        result
                    });
                }
                Ok(GenerateRustOutput {
                    shared: None,
                    per_schema,
                })
            }
            DedupeMode::Functional | DedupeMode::Full => {
                generate_rust_with_dedupe(schemas, settings)
            }
        }
    }
}

/// One struct to emit: name and the object schema (root or nested).
struct StructToEmit {
    name: String,
    schema: JsonSchema,
}

/// Map from enum value list to (name, description) for dedupe path. Used to resolve enum type names and carry description from first occurrence.
type EnumValuesToNameMap = BTreeMap<Vec<String>, (String, Option<String>)>;

/// One enum to emit: name, sorted deduplicated string values, and optional description (from first property schema that contributed this enum).
struct EnumToEmit {
    name: String,
    values: Vec<String>,
    description: Option<String>,
}

/// One anyOf enum to emit: name and list of (`variant_name`, `rust_type_string`).
struct AnyOfEnumToEmit {
    name: String,
    variants: Vec<(String, String)>,
}

/// One oneOf enum to emit: name and list of (`variant_name`, `rust_type_string`).
struct OneOfEnumToEmit {
    name: String,
    variants: Vec<(String, String)>,
}

/// Returns doc comment lines for emission: empty if description is None or whitespace-only; else one line per non-empty trimmed line (no blank lines).
fn doc_lines(s: Option<&str>) -> Vec<String> {
    let Some(trimmed) = s.map(str::trim) else {
        return Vec::new();
    };
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(String::from)
        .collect()
}

/// Returns sorted, deduplicated string values if the schema is a string enum; otherwise None.
fn string_enum_values(schema: &JsonSchema) -> Option<Vec<String>> {
    if !schema.is_string_enum() {
        return None;
    }
    let v: Vec<String> = schema
        .enum_values
        .as_ref()
        .expect("string enum")
        .iter()
        .filter_map(|x| x.as_str().map(String::from))
        .collect();
    let set: BTreeSet<String> = v.into_iter().collect();
    let mut out: Vec<String> = set.iter().cloned().collect();
    out.sort();
    Some(out)
}

/// Returns effective string enum values: either from `enum` (string-only) or from `const` when it is a string (single-value).
fn string_enum_or_const_values(schema: &JsonSchema) -> Option<Vec<String>> {
    if let Some(values) = string_enum_values(schema) {
        return Some(values);
    }
    if schema.is_string_const() {
        let s: String = schema
            .const_value
            .as_ref()
            .and_then(|v| v.as_str().map(String::from))
            .expect("string const");
        return Some(vec![s]);
    }
    None
}

/// For dedupe key: additionalProperties as Forbid or Schema(sub key).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum AdditionalPropertiesDedupe {
    Forbid,
    Schema(Box<DedupeKey>),
}

/// Key for deduplication: canonical representation of an object schema for a given mode.
/// Implements `Ord` + `Eq` for use in `BTreeMap`. Full mode includes id, description, comment, and examples; Functional mode excludes them (key ignores $id).
#[derive(Debug, Clone)]
struct DedupeKey {
    id: Option<String>,
    type_: Option<String>,
    properties: BTreeMap<String, DedupeKey>,
    additional_properties: Option<AdditionalPropertiesDedupe>,
    required: Option<Vec<String>>,
    title: Option<String>,
    description: Option<String>,
    comment: Option<String>,
    examples: Option<Vec<serde_json::Value>>,
    items: Option<Box<DedupeKey>>,
    unique_items: Option<bool>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    pattern: Option<String>,
    format: Option<String>,
    default_value: Option<serde_json::Value>,
}

impl PartialEq for DedupeKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.type_ == other.type_
            && self.properties == other.properties
            && self.additional_properties == other.additional_properties
            && self.required == other.required
            && self.title == other.title
            && self.description == other.description
            && self.comment == other.comment
            && self.examples == other.examples
            && self.items == other.items
            && self.unique_items == other.unique_items
            && self.min_items == other.min_items
            && self.max_items == other.max_items
            && self.min_length == other.min_length
            && self.max_length == other.max_length
            && self.pattern == other.pattern
            && self.format == other.format
            && self.default_value == other.default_value
    }
}

impl Eq for DedupeKey {}

impl PartialOrd for DedupeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DedupeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id
            .cmp(&other.id)
            .then_with(|| self.type_.cmp(&other.type_))
            .then_with(|| {
                self.properties
                    .keys()
                    .cmp(other.properties.keys())
                    .then_with(|| {
                        for (k, v) in &self.properties {
                            if let Some(ov) = other.properties.get(k) {
                                let c = v.cmp(ov);
                                if c != Ordering::Equal {
                                    return c;
                                }
                            }
                        }
                        self.properties.len().cmp(&other.properties.len())
                    })
            })
            .then_with(|| self.additional_properties.cmp(&other.additional_properties))
            .then_with(|| compare_option_vec(self.required.as_ref(), other.required.as_ref()))
            .then_with(|| self.title.cmp(&other.title))
            .then_with(|| self.description.cmp(&other.description))
            .then_with(|| self.comment.cmp(&other.comment))
            .then_with(|| compare_option_vec_value(self.examples.as_ref(), other.examples.as_ref()))
            .then_with(|| self.items.cmp(&other.items))
            .then_with(|| self.unique_items.cmp(&other.unique_items))
            .then_with(|| self.min_items.cmp(&other.min_items))
            .then_with(|| self.max_items.cmp(&other.max_items))
            .then_with(|| self.min_length.cmp(&other.min_length))
            .then_with(|| self.max_length.cmp(&other.max_length))
            .then_with(|| self.pattern.cmp(&other.pattern))
            .then_with(|| self.format.cmp(&other.format))
            .then_with(|| {
                compare_option_value(self.default_value.as_ref(), other.default_value.as_ref())
            })
    }
}

fn compare_option_value(a: Option<&serde_json::Value>, b: Option<&serde_json::Value>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a_val), Some(b_val)) => {
            let a_str: String = serde_json::to_string(a_val).unwrap_or_default();
            let b_str: String = serde_json::to_string(b_val).unwrap_or_default();
            a_str.cmp(&b_str)
        }
    }
}

fn compare_option_vec_value(
    a: Option<&Vec<serde_json::Value>>,
    b: Option<&Vec<serde_json::Value>>,
) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(aa), Some(bb)) => {
            let len_cmp: Ordering = aa.len().cmp(&bb.len());
            if len_cmp != Ordering::Equal {
                return len_cmp;
            }
            for (a_val, b_val) in aa.iter().zip(bb.iter()) {
                let c: Ordering = compare_option_value(Some(a_val), Some(b_val));
                if c != Ordering::Equal {
                    return c;
                }
            }
            Ordering::Equal
        }
    }
}

/// Returns true when the JSON default value equals the Rust type default (so we can use `#[serde(default)]`).
fn json_value_equals_rust_type_default(
    value: &serde_json::Value,
    _type_str: &str,
    is_optional: bool,
) -> bool {
    if is_optional {
        return value.is_null();
    }
    match value {
        serde_json::Value::Null => true,
        serde_json::Value::Bool(b) => !*b,
        serde_json::Value::Number(n) => {
            if n.as_i64() == Some(0) {
                return true;
            }
            if n.as_f64() == Some(0.0) {
                return true;
            }
            false
        }
        serde_json::Value::String(s) => s.is_empty(),
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
    }
}

/// Returns the default function name for a struct field (e.g. `default_Root_my_field`).
fn default_function_name(struct_name: &str, field_name: &str) -> String {
    format!("default_{struct_name}_{field_name}")
}

/// Returns Rust expression for a JSON default value for the given type (e.g. `Some("foo".to_string())` for Option<String>).
fn json_default_to_rust_expr(
    value: &serde_json::Value,
    _ty: &str,
    is_optional: bool,
) -> Option<String> {
    let inner: String = match value {
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => {
            if n.is_i64() {
                n.as_i64().unwrap().to_string()
            } else {
                n.as_f64().unwrap().to_string()
            }
        }
        serde_json::Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\".to_string()")
        }
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            return None;
        }
    };
    let expr: String = if is_optional {
        format!("Some({inner})")
    } else {
        inner
    };
    Some(expr)
}

fn compare_option_vec(a: Option<&Vec<String>>, b: Option<&Vec<String>>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(aa), Some(bb)) => aa.cmp(bb),
    }
}

/// If the buffer contains "`HashSet`", insert `use std::collections::HashSet;` after the serde use line.
fn maybe_prepend_btreemap_use(mut buf: Vec<u8>) -> Vec<u8> {
    if !buf.windows(8).any(|w| w == b"BTreeMap") {
        return buf;
    }
    let needle = b"use serde::{Deserialize, Serialize};\n";
    let pos = buf
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + needle.len());
    if let Some(insert_at) = pos {
        let line = b"use std::collections::BTreeMap;\n";
        buf.splice(insert_at..insert_at, line.iter().copied());
    }
    buf
}

fn maybe_prepend_hash_set_use(mut buf: Vec<u8>) -> Vec<u8> {
    if !buf.windows(7).any(|w| w == b"HashSet") {
        return buf;
    }
    let needle = b"use serde::{Deserialize, Serialize};\n";
    let pos = buf
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|i| i + needle.len());
    if let Some(insert_at) = pos {
        let hash_set_use = b"use std::collections::HashSet;\n";
        buf.splice(insert_at..insert_at, hash_set_use.iter().copied());
    }
    buf
}

/// If the buffer contains "`Uuid`", insert `use uuid::Uuid;` after the HashSet use line (if present) or after the serde use line.
#[cfg(feature = "uuid")]
fn maybe_prepend_uuid_use(mut buf: Vec<u8>) -> Vec<u8> {
    if !buf.windows(4).any(|w| w == b"Uuid") {
        return buf;
    }
    let hash_set_needle = b"use std::collections::HashSet;\n";
    let serde_needle = b"use serde::{Deserialize, Serialize};\n";
    let insert_at = buf
        .windows(hash_set_needle.len())
        .position(|w| w == hash_set_needle)
        .map(|i| i + hash_set_needle.len())
        .or_else(|| {
            buf.windows(serde_needle.len())
                .position(|w| w == serde_needle)
                .map(|i| i + serde_needle.len())
        });
    if let Some(pos) = insert_at {
        let uuid_use = b"use uuid::Uuid;\n";
        buf.splice(pos..pos, uuid_use.iter().copied());
    }
    buf
}

/// Emits `#[derive(..., ToJsonSchema)]` and optional `#[json_schema(title = "...")]` for a struct.
/// Uses `json_schema_rs_macro::ToJsonSchema` so generated code compiles when the macro crate is a dependency.
/// Emits struct doc comment from schema.description when non-empty.
fn emit_struct_derive_and_attrs(
    out: &mut impl Write,
    name: &str,
    schema: &JsonSchema,
) -> CodeGenResult<()> {
    for line in doc_lines(schema.description.as_deref()) {
        writeln!(out, "/// {line}")?;
    }
    writeln!(
        out,
        "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]"
    )?;
    if let Some(ref t) = schema.title {
        let t = t.trim();
        if !t.is_empty() {
            let escaped = t.replace('\\', "\\\\").replace('"', "\\\"");
            writeln!(out, "#[json_schema(title = \"{escaped}\")]")?;
        }
    }
    if let Some(ref i) = schema.id {
        let escaped = i.replace('\\', "\\\\").replace('"', "\\\"");
        writeln!(out, "#[json_schema(id = \"{escaped}\")]")?;
    }
    if schema
        .additional_properties
        .as_ref()
        .is_some_and(|ap| matches!(ap, AdditionalProperties::Forbid))
    {
        writeln!(out, "#[serde(deny_unknown_fields)]")?;
    }
    writeln!(out, "pub struct {name} {{")?;
    Ok(())
}

impl DedupeKey {
    fn from_schema(schema: &JsonSchema, mode: DedupeMode) -> Self {
        let properties: BTreeMap<String, DedupeKey> = schema
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), DedupeKey::from_schema(v, mode)))
            .collect();
        let items: Option<Box<DedupeKey>> = schema
            .items
            .as_ref()
            .filter(|_| schema.type_.as_deref() == Some("array"))
            .map(|s| Box::new(DedupeKey::from_schema(s, mode)));
        let unique_items: Option<bool> = if schema.type_.as_deref() == Some("array") {
            schema.unique_items
        } else {
            None
        };
        let min_items: Option<u64> = if schema.type_.as_deref() == Some("array") {
            schema.min_items
        } else {
            None
        };
        let max_items: Option<u64> = if schema.type_.as_deref() == Some("array") {
            schema.max_items
        } else {
            None
        };
        let min_length: Option<u64> = if schema.type_.as_deref() == Some("string") {
            schema.min_length
        } else {
            None
        };
        let max_length: Option<u64> = if schema.type_.as_deref() == Some("string") {
            schema.max_length
        } else {
            None
        };
        let pattern: Option<String> = if schema.type_.as_deref() == Some("string") {
            schema.pattern.clone()
        } else {
            None
        };
        let format: Option<String> = if schema.type_.as_deref() == Some("string") {
            schema.format.clone()
        } else {
            None
        };
        let additional_properties: Option<AdditionalPropertiesDedupe> =
            match schema.additional_properties.as_ref() {
                None | Some(AdditionalProperties::Allow) => None,
                Some(AdditionalProperties::Forbid) => Some(AdditionalPropertiesDedupe::Forbid),
                Some(AdditionalProperties::Schema(s)) => Some(AdditionalPropertiesDedupe::Schema(
                    Box::new(DedupeKey::from_schema(s, mode)),
                )),
            };
        DedupeKey {
            id: match mode {
                DedupeMode::Full => schema.id.clone(),
                DedupeMode::Functional | DedupeMode::Disabled => None,
            },
            type_: schema.type_.clone(),
            properties,
            additional_properties,
            required: schema.required.clone(),
            title: schema.title.clone(),
            description: match mode {
                DedupeMode::Full => schema.description.clone(),
                DedupeMode::Functional | DedupeMode::Disabled => None,
            },
            comment: match mode {
                DedupeMode::Full => schema.comment.clone(),
                DedupeMode::Functional | DedupeMode::Disabled => None,
            },
            examples: match mode {
                DedupeMode::Full => schema.examples.clone(),
                DedupeMode::Functional | DedupeMode::Disabled => None,
            },
            items,
            unique_items,
            min_items,
            max_items,
            min_length,
            max_length,
            pattern,
            format,
            default_value: schema.default_value.clone(),
        }
    }
}

/// Compute struct/type name from title, property key, and root fallback per settings.
fn struct_name_from(
    title: Option<&str>,
    from_key: Option<&str>,
    is_root: bool,
    settings: &CodeGenSettings,
) -> String {
    let title_trimmed: Option<&str> = title.filter(|t| !t.trim().is_empty()).map(str::trim);
    let from_key_s: Option<&str> = from_key;

    let (first, second) = match settings.model_name_source {
        ModelNameSource::TitleFirst => (title_trimmed, from_key_s),
        ModelNameSource::PropertyKeyFirst => (from_key_s, title_trimmed),
    };

    first
        .map(sanitize_struct_name)
        .or_else(|| second.map(sanitize_struct_name))
        .unwrap_or_else(|| {
            if is_root {
                "Root".to_string()
            } else {
                "Unnamed".to_string()
            }
        })
}

const I64_MAX_AS_F64: f64 = 9_223_372_036_854_775_807.0_f64; // i64::MAX, exactly representable

/// Returns the Rust type string for an integer or number schema using `minimum` and `maximum` when both present and valid; otherwise fallback to `i64` or `f64`.
fn rust_numeric_type_for_schema(schema: &JsonSchema) -> String {
    if schema.is_integer() {
        let min: Option<f64> = schema.minimum;
        let max: Option<f64> = schema.maximum;
        #[expect(clippy::cast_precision_loss)]
        let i64_min_f64: f64 = i64::MIN as f64;
        let (min_i64, max_i64): (Option<i64>, Option<i64>) = match (min, max) {
            (Some(mi), Some(ma)) if mi <= ma => {
                let valid_min: bool =
                    mi.fract() == 0.0 && (i64_min_f64..=I64_MAX_AS_F64).contains(&mi);
                let valid_max: bool =
                    ma.fract() == 0.0 && (i64_min_f64..=I64_MAX_AS_F64).contains(&ma);
                if valid_min && valid_max {
                    #[expect(clippy::cast_possible_truncation)]
                    let min_i: i64 = mi as i64;
                    #[expect(clippy::cast_possible_truncation)]
                    let max_i: i64 = ma as i64;
                    (Some(min_i), Some(max_i))
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        };
        if let (Some(lo), Some(hi)) = (min_i64, max_i64) {
            if lo >= 0 {
                if hi <= i64::from(u8::MAX) {
                    return "u8".to_string();
                }
                if hi <= i64::from(u16::MAX) {
                    return "u16".to_string();
                }
                if hi <= i64::from(u32::MAX) {
                    return "u32".to_string();
                }
                return "u64".to_string();
            }
            if lo >= i64::from(i8::MIN) && hi <= i64::from(i8::MAX) {
                return "i8".to_string();
            }
            if lo >= i64::from(i16::MIN) && hi <= i64::from(i16::MAX) {
                return "i16".to_string();
            }
            if lo >= i64::from(i32::MIN) && hi <= i64::from(i32::MAX) {
                return "i32".to_string();
            }
        }
        return "i64".to_string();
    }
    if schema.is_number() {
        let min: Option<f64> = schema.minimum;
        let max: Option<f64> = schema.maximum;
        if let (Some(mi), Some(ma)) = (min, max)
            && mi <= ma
            && mi >= f64::from(f32::MIN)
            && ma <= f64::from(f32::MAX)
            && mi.is_finite()
            && ma.is_finite()
        {
            return "f32".to_string();
        }
        return "f64".to_string();
    }
    unreachable!("rust_numeric_type_for_schema only called for integer or number schema");
}

/// Returns true when the schema represents a type that is Hash + Eq (string, integer, number, or enum).
/// Used to decide whether to emit `HashSet<T>` for array with uniqueItems: true.
fn item_schema_is_hashable(schema: &JsonSchema) -> bool {
    !schema.is_object_with_properties() && !schema.is_array_with_items()
}

/// True if schema is object-like for allOf merge: type "object" or non-empty properties.
fn is_object_like_for_merge(schema: &JsonSchema) -> bool {
    schema.type_.as_deref() == Some("object") || !schema.properties.is_empty()
}

/// Merge an array of object-like schemas (allOf) into a single schema. Errors on empty array,
/// non-object-like subschema, or conflicting property types/bounds/enums.
pub(crate) fn merge_all_of(schemas: &[JsonSchema]) -> CodeGenResult<JsonSchema> {
    if schemas.is_empty() {
        return Err(CodeGenError::AllOfMergeEmpty);
    }
    for (index, s) in schemas.iter().enumerate() {
        if !is_object_like_for_merge(s) {
            return Err(CodeGenError::AllOfMergeNonObjectSubschema { index });
        }
    }
    let mut merged = JsonSchema::default();
    for s in schemas {
        merge_object_schema_into(&mut merged, s, "")?;
    }
    merged.type_ = Some("object".to_string());
    Ok(merged)
}

/// Merge one object schema into `target`. Used for allOf merge. `parent_key` is for error messages.
fn merge_object_schema_into(
    target: &mut JsonSchema,
    other: &JsonSchema,
    parent_key: &str,
) -> CodeGenResult<()> {
    for (k, other_prop) in &other.properties {
        let key_for_errors = if parent_key.is_empty() {
            k.clone()
        } else {
            format!("{parent_key}.{k}")
        };
        if let Some(target_prop) = target.properties.get_mut(k) {
            let merged_prop = merge_property_schemas(target_prop, other_prop, &key_for_errors)?;
            *target_prop = merged_prop;
        } else {
            target.properties.insert(k.clone(), other_prop.clone());
        }
    }
    // Required: union, dedupe, first-occurrence order
    let mut required: Vec<String> = target.required.clone().unwrap_or_default();
    for r in other.required.as_deref().unwrap_or(&[]) {
        if !required.contains(r) {
            required.push(r.clone());
        }
    }
    target.required = if required.is_empty() {
        None
    } else {
        Some(required)
    };
    if target.title.as_deref().map_or("", str::trim).is_empty() {
        target.title.clone_from(&other.title);
    }
    if target
        .description
        .as_deref()
        .map_or("", str::trim)
        .is_empty()
    {
        target.description.clone_from(&other.description);
    }
    if target.comment.is_none() {
        target.comment.clone_from(&other.comment);
    }
    if target.examples.is_none() {
        target.examples.clone_from(&other.examples);
    }
    Ok(())
}

/// Merge two property subschemas (same key in different allOf branches). Returns merged schema or error on conflict.
fn merge_property_schemas(
    a: &JsonSchema,
    b: &JsonSchema,
    property_key: &str,
) -> CodeGenResult<JsonSchema> {
    if a.is_object_with_properties() && b.is_object_with_properties() {
        let mut merged = a.clone();
        merge_object_schema_into(&mut merged, b, property_key)?;
        return Ok(merged);
    }
    if a.is_array_with_items() && b.is_array_with_items() {
        let a_items = a.items.as_ref().expect("array with items").as_ref();
        let b_items = b.items.as_ref().expect("array with items").as_ref();
        let merged_items = merge_property_schemas(a_items, b_items, &format!("{property_key}[]"))?;
        let mut out = a.clone();
        out.items = Some(Box::new(merged_items));
        return Ok(out);
    }
    let type_a = a.type_.as_deref();
    let type_b = b.type_.as_deref();
    if type_a != type_b {
        return Err(CodeGenError::AllOfMergeConflictingPropertyType {
            property_key: property_key.to_string(),
            subschema_indices: vec![], // we don't have indices in this context; message still clear
        });
    }
    if a.is_string() && b.is_string() {
        let mut out = a.clone();
        if out.min_length.is_none() {
            out.min_length = b.min_length;
        }
        if out.max_length.is_none() {
            out.max_length = b.max_length;
        }
        if let (Some(pa), Some(pb)) = (&a.pattern, &b.pattern) {
            if pa != pb {
                return Err(CodeGenError::AllOfMergeConflictingPattern {
                    property_key: property_key.to_string(),
                });
            }
        } else if out.pattern.is_none() {
            out.pattern.clone_from(&b.pattern);
        }
        if out.format.is_none() {
            out.format.clone_from(&b.format);
        }
        if let (Some(ea), Some(eb)) = (&a.enum_values, &b.enum_values) {
            if ea != eb {
                return Err(CodeGenError::AllOfMergeConflictingEnum {
                    property_key: property_key.to_string(),
                });
            }
        } else if b.enum_values.is_some() {
            out.enum_values.clone_from(&b.enum_values);
        }
        if let (Some(ca), Some(cb)) = (&a.const_value, &b.const_value) {
            if ca != cb {
                return Err(CodeGenError::AllOfMergeConflictingConst {
                    property_key: property_key.to_string(),
                });
            }
        } else if b.const_value.is_some() {
            out.const_value.clone_from(&b.const_value);
        }
        return Ok(out);
    }
    if a.is_integer() && b.is_integer() || a.is_number() && b.is_number() {
        let mut out = a.clone();
        merge_numeric_bounds(&mut out, b, property_key, "minimum", "maximum")?;
        return Ok(out);
    }
    if a.is_string_enum() && b.is_string_enum() {
        if a.enum_values != b.enum_values {
            return Err(CodeGenError::AllOfMergeConflictingEnum {
                property_key: property_key.to_string(),
            });
        }
        return Ok(a.clone());
    }
    if type_a.is_some() || type_b.is_some() {
        return Err(CodeGenError::AllOfMergeConflictingPropertyType {
            property_key: property_key.to_string(),
            subschema_indices: vec![],
        });
    }
    Ok(a.clone())
}

fn merge_numeric_bounds(
    target: &mut JsonSchema,
    other: &JsonSchema,
    property_key: &str,
    min_kw: &str,
    max_kw: &str,
) -> CodeGenResult<()> {
    let (t_min, t_max) = (target.minimum, target.maximum);
    let (o_min, o_max) = (other.minimum, other.maximum);
    let new_min = match (t_min, o_min) {
        (Some(t), Some(o)) => Some(t.max(o)),
        (a, None) | (None, a) => a,
    };
    let new_max = match (t_max, o_max) {
        (Some(t), Some(o)) => Some(t.min(o)),
        (a, None) | (None, a) => a,
    };
    if let (Some(mi), Some(ma)) = (new_min, new_max)
        && mi > ma
    {
        return Err(CodeGenError::AllOfMergeConflictingNumericBounds {
            property_key: property_key.to_string(),
            keyword: format!("{min_kw}/{max_kw}"),
        });
    }
    target.minimum = new_min;
    target.maximum = new_max;
    Ok(())
}

/// Resolve allOf for codegen: if schema has non-empty `all_of`, merge and return; otherwise return clone.
pub(crate) fn resolve_all_of_for_codegen(schema: &JsonSchema) -> CodeGenResult<JsonSchema> {
    match &schema.all_of {
        Some(all) if !all.is_empty() => merge_all_of(all),
        Some(_) => Err(CodeGenError::AllOfMergeEmpty),
        None => Ok(schema.clone()),
    }
}

/// Returns the Rust type string for a schema (used for array item type and nested types).
/// Unsupported types yield `serde_json::Value`.
fn rust_type_for_item_schema(
    root: &JsonSchema,
    schema: &JsonSchema,
    from_key: Option<&str>,
    enum_values_to_name: Option<&BTreeMap<Vec<String>, String>>,
    key_to_name: Option<&BTreeMap<DedupeKey, String>>,
    settings: &CodeGenSettings,
    mode: DedupeMode,
) -> CodeGenResult<String> {
    let mut def_key: Option<String> = None;
    let schema: &JsonSchema = if let Some(ref_str) = schema.ref_.as_deref() {
        match ref_resolver::parse_ref(ref_str) {
            Ok(
                ref_resolver::ParsedRef::Defs(name) | ref_resolver::ParsedRef::Definitions(name),
            ) => def_key = Some(name),
            Ok(ref_resolver::ParsedRef::Root) => {}
            Err(e) => {
                return Err(CodeGenError::RefResolution {
                    ref_str: ref_str.to_string(),
                    reason: format!("{e:?}"),
                });
            }
        }

        ref_resolver::resolve_schema_ref_transitive(root, schema).map_err(|e| {
            CodeGenError::RefResolution {
                ref_str: ref_str.to_string(),
                reason: format!("{e:?}"),
            }
        })?
    } else {
        schema
    };
    if let Some(values) = string_enum_or_const_values(schema)
        && let Some(m) = enum_values_to_name
        && let Some(name) = m.get(&values)
    {
        return Ok(name.clone());
    }
    if schema.is_string()
        || (schema.enum_values.as_ref().is_some_and(|v| !v.is_empty()) && !schema.is_string_enum())
        || (schema.const_value.is_some() && !schema.is_string_const())
    {
        #[cfg(feature = "uuid")]
        {
            if schema.format.as_deref() == Some("uuid") {
                return Ok("Uuid".to_string());
            }
        }
        return Ok("String".to_string());
    }
    if schema.is_integer() {
        return Ok(rust_numeric_type_for_schema(schema));
    }
    if schema.is_number() {
        return Ok(rust_numeric_type_for_schema(schema));
    }
    if schema.is_object_with_properties() {
        if let Some(key) = def_key.as_deref() {
            return Ok(sanitize_struct_name(key));
        }
        let name: String = if let Some(m) = key_to_name {
            let key = DedupeKey::from_schema(schema, mode);
            m.get(&key).cloned().unwrap_or_else(|| {
                struct_name_from(schema.title.as_deref(), from_key, false, settings)
            })
        } else {
            struct_name_from(schema.title.as_deref(), from_key, false, settings)
        };
        return Ok(name);
    }
    if schema.is_array_with_items() {
        let item_schema: &JsonSchema = schema.items.as_ref().expect("array with items").as_ref();
        let inner: String = rust_type_for_item_schema(
            root,
            item_schema,
            from_key,
            enum_values_to_name,
            key_to_name,
            settings,
            mode,
        )?;
        let use_hash_set: bool =
            schema.unique_items == Some(true) && item_schema_is_hashable(item_schema);
        return Ok(if use_hash_set {
            format!("HashSet<{inner}>")
        } else {
            format!("Vec<{inner}>")
        });
    }
    Ok("serde_json::Value".to_string())
}

/// Collect all string enums from a schema (and nested properties). Dedupe by value list; first occurrence wins the name and description.
fn collect_enums(
    root: &JsonSchema,
    schema: &JsonSchema,
    settings: &CodeGenSettings,
) -> CodeGenResult<Vec<EnumToEmit>> {
    let mut key_to_name_desc: BTreeMap<Vec<String>, (String, Option<String>)> = BTreeMap::new();
    let mut stack: Vec<JsonSchema> = vec![schema.clone()];
    while let Some(node) = stack.pop() {
        let (node, _) = resolve_ref_for_codegen(root, &node, None)?;
        for (key, prop_schema) in &node.properties {
            let (prop_effective, from_key) = resolve_ref_for_codegen(root, prop_schema, Some(key))?;
            if let Some(values) = string_enum_or_const_values(&prop_effective) {
                key_to_name_desc.entry(values.clone()).or_insert_with(|| {
                    let name: String = struct_name_from(
                        prop_effective.title.as_deref(),
                        from_key.as_deref(),
                        false,
                        settings,
                    );
                    let description: Option<String> = prop_effective
                        .description
                        .as_ref()
                        .filter(|s| !s.trim().is_empty())
                        .cloned();
                    (name, description)
                });
            }
            if prop_effective.is_object_with_properties() {
                stack.push(prop_effective.clone());
            }
            if prop_effective.is_array_with_items()
                && let Some(ref items) = prop_effective.items
            {
                let (items_effective, items_from_key) =
                    resolve_ref_for_codegen(root, items.as_ref(), Some(key))?;
                if let Some(values) = string_enum_or_const_values(&items_effective) {
                    key_to_name_desc.entry(values.clone()).or_insert_with(|| {
                        let name: String = struct_name_from(
                            items_effective.title.as_deref(),
                            items_from_key.as_deref(),
                            false,
                            settings,
                        );
                        let description: Option<String> = items_effective
                            .description
                            .as_ref()
                            .filter(|s| !s.trim().is_empty())
                            .cloned();
                        (name, description)
                    });
                }
                if items_effective.is_object_with_properties() {
                    stack.push(items_effective);
                }
            }
        }
        if let Some(ref any_of) = node.any_of {
            for sub in any_of {
                stack.push(sub.clone());
            }
        }
        if let Some(ref one_of) = node.one_of {
            for sub in one_of {
                stack.push(sub.clone());
            }
        }
    }
    Ok(key_to_name_desc
        .into_iter()
        .map(|(values, (name, description))| EnumToEmit {
            name,
            values,
            description,
        })
        .collect())
}

/// Collect all anyOf enums from a schema (root and nested). Each node with non-empty anyOf produces one enum.
fn collect_anyof_enums(
    root: &JsonSchema,
    schema: &JsonSchema,
    settings: &CodeGenSettings,
    enum_values_to_name: &BTreeMap<Vec<String>, String>,
) -> CodeGenResult<Vec<AnyOfEnumToEmit>> {
    let mut out: Vec<AnyOfEnumToEmit> = vec![];
    let mut stack: Vec<(JsonSchema, Option<String>)> = vec![(schema.clone(), None)];
    while let Some((node, from_key)) = stack.pop() {
        let (node, from_key) = resolve_ref_for_codegen(root, &node, from_key.as_deref())?;
        if let Some(ref any_of) = node.any_of {
            if any_of.is_empty() {
                return Err(CodeGenError::AnyOfEmpty);
            }
            let name = match &from_key {
                Some(k) => sanitize_struct_name(k) + "AnyOf",
                None => node.title.as_deref().map_or_else(
                    || "RootAnyOf".to_string(),
                    |t| sanitize_struct_name(t) + "AnyOf",
                ),
            };
            let mut variants = Vec::with_capacity(any_of.len());
            for (i, sub) in any_of.iter().enumerate() {
                let resolved = resolve_all_of_for_codegen(sub)?;
                let variant_from_key =
                    format!("{}_Variant{i}", from_key.as_deref().unwrap_or("Root"));
                let ty = rust_type_for_item_schema(
                    root,
                    &resolved,
                    Some(&variant_from_key),
                    Some(enum_values_to_name),
                    None,
                    settings,
                    DedupeMode::Full,
                )?;
                variants.push((format!("Variant{i}"), ty));
            }
            out.push(AnyOfEnumToEmit { name, variants });
            for sub in any_of {
                let resolved = resolve_all_of_for_codegen(sub)?;
                stack.push((resolved, None));
            }
        }
        for (key, prop_schema) in &node.properties {
            stack.push((prop_schema.clone(), Some(key.clone())));
        }
    }
    Ok(out)
}

/// Collect all oneOf enums from a schema (root and nested). Each node with non-empty oneOf produces one enum.
fn collect_oneof_enums(
    root: &JsonSchema,
    schema: &JsonSchema,
    settings: &CodeGenSettings,
    enum_values_to_name: &BTreeMap<Vec<String>, String>,
) -> CodeGenResult<Vec<OneOfEnumToEmit>> {
    let mut out: Vec<OneOfEnumToEmit> = vec![];
    let mut stack: Vec<(JsonSchema, Option<String>)> = vec![(schema.clone(), None)];
    while let Some((node, from_key)) = stack.pop() {
        let (node, from_key) = resolve_ref_for_codegen(root, &node, from_key.as_deref())?;
        if let Some(ref one_of) = node.one_of {
            if one_of.is_empty() {
                return Err(CodeGenError::OneOfEmpty);
            }
            let name = match &from_key {
                Some(k) => sanitize_struct_name(k) + "OneOf",
                None => node.title.as_deref().map_or_else(
                    || "RootOneOf".to_string(),
                    |t| sanitize_struct_name(t) + "OneOf",
                ),
            };
            let mut variants = Vec::with_capacity(one_of.len());
            for (i, sub) in one_of.iter().enumerate() {
                let resolved = resolve_all_of_for_codegen(sub)?;
                let variant_from_key =
                    format!("{}_Variant{i}", from_key.as_deref().unwrap_or("Root"));
                let ty = rust_type_for_item_schema(
                    root,
                    &resolved,
                    Some(&variant_from_key),
                    Some(enum_values_to_name),
                    None,
                    settings,
                    DedupeMode::Full,
                )?;
                variants.push((format!("Variant{i}"), ty));
            }
            out.push(OneOfEnumToEmit { name, variants });
            for sub in one_of {
                let resolved = resolve_all_of_for_codegen(sub)?;
                stack.push((resolved, None));
            }
        }
        for (key, prop_schema) in &node.properties {
            stack.push((prop_schema.clone(), Some(key.clone())));
        }
    }
    Ok(out)
}

/// Collect all object schemas that need a struct in topological order (children before parents).
/// Uses an explicit stack to avoid recursion and stack overflow on deep schemas.
/// Resolves allOf for each node before use (merge on-the-fly).
fn resolve_ref_for_codegen(
    root: &JsonSchema,
    schema: &JsonSchema,
    fallback_from_key: Option<&str>,
) -> CodeGenResult<(JsonSchema, Option<String>)> {
    let mut from_key: Option<String> = fallback_from_key.map(String::from);
    let Some(ref_str) = schema.ref_.as_deref() else {
        return Ok((schema.clone(), from_key));
    };

    match ref_resolver::parse_ref(ref_str) {
        Ok(ref_resolver::ParsedRef::Defs(name) | ref_resolver::ParsedRef::Definitions(name)) => {
            from_key = Some(name);
        }
        Ok(ref_resolver::ParsedRef::Root) => {}
        Err(e) => {
            return Err(CodeGenError::RefResolution {
                ref_str: ref_str.to_string(),
                reason: format!("{e:?}"),
            });
        }
    }

    let resolved: &JsonSchema =
        ref_resolver::resolve_schema_ref_transitive(root, schema).map_err(|e| {
            CodeGenError::RefResolution {
                ref_str: ref_str.to_string(),
                reason: format!("{e:?}"),
            }
        })?;
    Ok((resolved.clone(), from_key))
}

#[expect(clippy::too_many_lines)]
fn collect_structs(
    root: &JsonSchema,
    schema: &JsonSchema,
    from_key: Option<&str>,
    out: &mut Vec<StructToEmit>,
    seen: &mut BTreeSet<String>,
    settings: &CodeGenSettings,
) -> CodeGenResult<()> {
    let (schema, from_key_opt) = resolve_ref_for_codegen(root, schema, from_key)?;
    if !schema.is_object_with_properties() {
        return Ok(());
    }

    // Phase 1: iterative post-order DFS to collect (schema, from_key) so children come before parents.
    let mut post_order: Vec<(JsonSchema, Option<String>, bool)> = Vec::new();
    let mut stack: Vec<(JsonSchema, Option<String>, usize, bool)> = Vec::new();
    stack.push((
        schema.clone(),
        from_key_opt.clone(),
        0,
        from_key_opt.is_none(),
    ));

    while let Some((schema_node, from_key_opt, index, is_root)) = stack.pop() {
        let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
        if index < keys.len() {
            let key: String = keys.get(index).unwrap().clone();
            let child: JsonSchema = schema_node.properties.get(&key).unwrap().clone();
            let child_resolved = resolve_all_of_for_codegen(&child)?;
            stack.push((schema_node, from_key_opt, index + 1, is_root));
            if child_resolved
                .any_of
                .as_ref()
                .is_some_and(|v| !v.is_empty())
            {
                for (i, sub) in child_resolved.any_of.as_ref().unwrap().iter().enumerate() {
                    let sub_resolved = resolve_all_of_for_codegen(sub)?;
                    let variant_key = format!("{key}_Variant{i}");
                    let (sub_effective, sub_from_key) =
                        resolve_ref_for_codegen(root, &sub_resolved, Some(&variant_key))?;
                    if sub_effective.is_object_with_properties() {
                        stack.push((sub_effective, sub_from_key, 0, false));
                    } else if sub_resolved.is_array_with_items()
                        && let Some(ref items) = sub_resolved.items
                    {
                        let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                        let (items_effective, items_from_key) =
                            resolve_ref_for_codegen(root, &items_resolved, Some(&variant_key))?;
                        if items_effective.is_object_with_properties() {
                            stack.push((items_effective, items_from_key, 0, false));
                        }
                    }
                }
            } else if child_resolved
                .one_of
                .as_ref()
                .is_some_and(|v| !v.is_empty())
            {
                for (i, sub) in child_resolved.one_of.as_ref().unwrap().iter().enumerate() {
                    let sub_resolved = resolve_all_of_for_codegen(sub)?;
                    let variant_key = format!("{key}_Variant{i}");
                    let (sub_effective, sub_from_key) =
                        resolve_ref_for_codegen(root, &sub_resolved, Some(&variant_key))?;
                    if sub_effective.is_object_with_properties() {
                        stack.push((sub_effective, sub_from_key, 0, false));
                    } else if sub_resolved.is_array_with_items()
                        && let Some(ref items) = sub_resolved.items
                    {
                        let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                        let (items_effective, items_from_key) =
                            resolve_ref_for_codegen(root, &items_resolved, Some(&variant_key))?;
                        if items_effective.is_object_with_properties() {
                            stack.push((items_effective, items_from_key, 0, false));
                        }
                    }
                }
            } else {
                let (child_effective, child_from_key) =
                    resolve_ref_for_codegen(root, &child_resolved, Some(&key))?;
                if child_effective.is_object_with_properties() {
                    stack.push((child_effective, child_from_key, 0, false));
                } else if child_effective.is_array_with_items()
                    && let Some(ref items) = child_effective.items
                {
                    let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                    let (items_effective, items_from_key) =
                        resolve_ref_for_codegen(root, &items_resolved, Some(&key))?;
                    if items_effective.is_object_with_properties() {
                        stack.push((items_effective, items_from_key, 0, false));
                    }
                }
            }
        } else {
            post_order.push((schema_node, from_key_opt, is_root));
        }
    }

    // Phase 2: emit in post-order, dedupe by name (first occurrence wins).
    for (schema_node, from_key_opt, is_root) in post_order {
        let name: String = struct_name_from(
            schema_node.title.as_deref(),
            from_key_opt.as_deref(),
            is_root,
            settings,
        );

        if seen.contains(&name) {
            continue;
        }
        seen.insert(name.clone());

        out.push(StructToEmit {
            name,
            schema: schema_node,
        });
    }
    Ok(())
}

/// Collect (`schema_idx`, `candidate_name`, schema) for every struct from all schemas in post-order
/// (children before parents) per schema. No name dedupe.
#[expect(clippy::too_many_lines)]
fn collect_structs_all_schemas(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> CodeGenResult<Vec<(usize, String, JsonSchema)>> {
    let mut out: Vec<(usize, String, JsonSchema)> = Vec::new();
    for (schema_idx, schema_root) in schemas.iter().enumerate() {
        let (effective_root, root_from_key) =
            resolve_ref_for_codegen(schema_root, schema_root, None)?;
        if !effective_root.is_object_with_properties() {
            continue;
        }

        let mut post_order: Vec<(JsonSchema, Option<String>, bool)> = Vec::new();
        let mut stack: Vec<(JsonSchema, Option<String>, usize, bool)> = Vec::new();
        let is_root: bool = root_from_key.is_none();
        stack.push((effective_root.clone(), root_from_key, 0, is_root));

        while let Some((schema_node, from_key_opt, index, is_root)) = stack.pop() {
            let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
            if index < keys.len() {
                let key: String = keys[index].clone();
                let child: JsonSchema = schema_node.properties.get(&key).unwrap().clone();
                let child_resolved = resolve_all_of_for_codegen(&child)?;

                stack.push((schema_node, from_key_opt, index + 1, is_root));

                if child_resolved
                    .any_of
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
                {
                    for (i, sub) in child_resolved.any_of.as_ref().unwrap().iter().enumerate() {
                        let sub_resolved = resolve_all_of_for_codegen(sub)?;
                        let variant_key = format!("{key}_Variant{i}");
                        let (sub_effective, sub_from_key) = resolve_ref_for_codegen(
                            schema_root,
                            &sub_resolved,
                            Some(&variant_key),
                        )?;
                        if sub_effective.is_object_with_properties() {
                            stack.push((sub_effective, sub_from_key, 0, false));
                        } else if sub_effective.is_array_with_items()
                            && let Some(ref items) = sub_effective.items
                        {
                            let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                            let (items_effective, items_from_key) = resolve_ref_for_codegen(
                                schema_root,
                                &items_resolved,
                                Some(&variant_key),
                            )?;
                            if items_effective.is_object_with_properties() {
                                stack.push((items_effective, items_from_key, 0, false));
                            }
                        }
                    }
                } else if child_resolved
                    .one_of
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
                {
                    for (i, sub) in child_resolved.one_of.as_ref().unwrap().iter().enumerate() {
                        let sub_resolved = resolve_all_of_for_codegen(sub)?;
                        let variant_key = format!("{key}_Variant{i}");
                        let (sub_effective, sub_from_key) = resolve_ref_for_codegen(
                            schema_root,
                            &sub_resolved,
                            Some(&variant_key),
                        )?;
                        if sub_effective.is_object_with_properties() {
                            stack.push((sub_effective, sub_from_key, 0, false));
                        } else if sub_effective.is_array_with_items()
                            && let Some(ref items) = sub_effective.items
                        {
                            let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                            let (items_effective, items_from_key) = resolve_ref_for_codegen(
                                schema_root,
                                &items_resolved,
                                Some(&variant_key),
                            )?;
                            if items_effective.is_object_with_properties() {
                                stack.push((items_effective, items_from_key, 0, false));
                            }
                        }
                    }
                } else {
                    let (child_effective, child_from_key) =
                        resolve_ref_for_codegen(schema_root, &child_resolved, Some(&key))?;
                    if child_effective.is_object_with_properties() {
                        stack.push((child_effective, child_from_key, 0, false));
                    } else if child_effective.is_array_with_items()
                        && let Some(ref items) = child_effective.items
                    {
                        let items_resolved = resolve_all_of_for_codegen(items.as_ref())?;
                        let (items_effective, items_from_key) =
                            resolve_ref_for_codegen(schema_root, &items_resolved, Some(&key))?;
                        if items_effective.is_object_with_properties() {
                            stack.push((items_effective, items_from_key, 0, false));
                        }
                    }
                }
            } else {
                post_order.push((schema_node, from_key_opt, is_root));
            }
        }

        for (schema_node, from_key_opt, is_root) in post_order {
            let name: String = struct_name_from(
                schema_node.title.as_deref(),
                from_key_opt.as_deref(),
                is_root,
                settings,
            );
            out.push((schema_idx, name, schema_node));
        }
    }
    Ok(out)
}

/// Generate Rust with dedupe (Functional or Full mode). Returns shared buffer (if any) and per-schema buffers.
#[expect(clippy::too_many_lines)]
#[expect(clippy::type_complexity)]
fn generate_rust_with_dedupe(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> CodeGenResult<GenerateRustOutput> {
    let mode: DedupeMode = settings.dedupe_mode;

    let resolved_schemas: Vec<JsonSchema> = schemas
        .iter()
        .enumerate()
        .map(|(i, s)| {
            resolve_all_of_for_codegen(s).map_err(|e| CodeGenError::Batch {
                index: i,
                source: Box::new(e),
            })
        })
        .collect::<CodeGenResult<Vec<_>>>()?;

    let mut enum_values_to_name: EnumValuesToNameMap = BTreeMap::new();
    for schema in &resolved_schemas {
        for e in collect_enums(schema, schema, settings)? {
            enum_values_to_name
                .entry(e.values.clone())
                .or_insert_with(|| (e.name.clone(), e.description.clone()));
        }
    }
    let all_enums: Vec<EnumToEmit> = enum_values_to_name
        .iter()
        .map(|(values, (name, description))| EnumToEmit {
            name: name.clone(),
            values: values.clone(),
            description: description.clone(),
        })
        .collect();

    let collected: Vec<(usize, String, JsonSchema)> =
        collect_structs_all_schemas(&resolved_schemas, settings)?;

    // Build BTreeMap: DedupeKey -> (canonical_name, schema, occurrences)
    let mut map: BTreeMap<DedupeKey, (String, JsonSchema, Vec<(usize, String)>)> = BTreeMap::new();
    for (schema_idx, name, schema) in &collected {
        let key: DedupeKey = DedupeKey::from_schema(schema, mode);
        map.entry(key)
            .or_insert_with(|| (name.clone(), schema.clone(), Vec::new()))
            .2
            .push((*schema_idx, name.clone()));
    }

    let shared_names: BTreeSet<String> = map
        .iter()
        .filter(|(_, (_, _, occs))| occs.len() > 1)
        .map(|(_, (canonical_name, _, _))| canonical_name.clone())
        .collect();

    let canonical_name_to_first_schema_idx: BTreeMap<String, usize> = {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for (canonical_name, _, occs) in map.values() {
            let first_idx: usize = occs.iter().map(|(i, _)| *i).min().unwrap_or(0);
            out.entry(canonical_name.clone())
                .and_modify(|v| *v = (*v).min(first_idx))
                .or_insert(first_idx);
        }
        out
    };

    let key_to_canonical_name: BTreeMap<DedupeKey, String> = map
        .iter()
        .map(|(k, (canonical, _, _))| (k.clone(), canonical.clone()))
        .collect();

    let key_to_canonical: BTreeMap<DedupeKey, (String, JsonSchema)> = map
        .iter()
        .map(|(k, (cn, schema, _))| (k.clone(), (cn.clone(), schema.clone())))
        .collect();

    if shared_names.is_empty() {
        let mut per_schema: Vec<Vec<u8>> = Vec::with_capacity(resolved_schemas.len());
        for (index, schema) in resolved_schemas.iter().enumerate() {
            let mut out = Cursor::new(Vec::new());
            emit_rust(schema, &mut out, settings).map_err(|e| CodeGenError::Batch {
                index,
                source: Box::new(e),
            })?;
            per_schema.push({
                let result = maybe_prepend_btreemap_use(out.into_inner());
                let result = maybe_prepend_hash_set_use(result);
                #[cfg(feature = "uuid")]
                let result = maybe_prepend_uuid_use(result);
                result
            });
        }
        return Ok(GenerateRustOutput {
            shared: None,
            per_schema,
        });
    }

    // Shared structs: (canonical_name, schema) in dependency order
    let shared_structs: Vec<(String, JsonSchema)> = {
        let mut v: Vec<(String, JsonSchema)> = key_to_canonical
            .iter()
            .filter(|(_, (cn, _))| shared_names.contains(cn))
            .map(|(_, (cn, s))| (cn.clone(), s.clone()))
            .collect();
        let order: Vec<String> = v.iter().map(|(n, _)| n.clone()).collect();
        let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, schema) in &v {
            let mut set: BTreeSet<String> = BTreeSet::new();
            for prop_schema in schema.properties.values() {
                if prop_schema.is_object_with_properties() {
                    let prop_key = DedupeKey::from_schema(prop_schema, mode);
                    if let Some(cn) = key_to_canonical_name.get(&prop_key)
                        && shared_names.contains(cn)
                    {
                        set.insert(cn.clone());
                    }
                }
                if prop_schema.is_array_with_items()
                    && let Some(ref items) = prop_schema.items
                    && items.is_object_with_properties()
                {
                    let item_key = DedupeKey::from_schema(items, mode);
                    if let Some(cn) = key_to_canonical_name.get(&item_key)
                        && shared_names.contains(cn)
                    {
                        set.insert(cn.clone());
                    }
                }
            }
            deps.insert(name.clone(), set);
        }
        topo_sort_by_deps(&order, &deps, &mut v);
        v
    };

    let shared_buffer: Vec<u8> = {
        let mut out = Cursor::new(Vec::new());
        writeln!(
            out,
            "//! Generated by json-schema-rs. Do not edit manually."
        )?;
        writeln!(out)?;
        writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
        writeln!(out)?;
        for e in &all_enums {
            let pairs: Vec<(String, String)> =
                enum_variant_names_with_collision_resolution(&e.values);
            emit_enum_from_pairs(&mut out, &e.name, &pairs, e.description.as_deref())?;
        }
        for (name, schema) in &shared_structs {
            let root_idx: usize = *canonical_name_to_first_schema_idx
                .get(name)
                .expect("root schema index for shared struct");
            let root_schema: &JsonSchema = resolved_schemas.get(root_idx).expect("root schema");
            emit_default_functions_for_struct(&mut out, name, schema)?;
            emit_struct_derive_and_attrs(&mut out, name, schema)?;
            emit_struct_fields_with_resolver(
                root_schema,
                name,
                schema,
                &mut out,
                settings,
                Some(&key_to_canonical_name),
                mode,
                Some(&enum_values_to_name),
            )?;
            writeln!(out, "}}")?;
            writeln!(out)?;
        }
        {
            let result = maybe_prepend_btreemap_use(out.into_inner());
            let result = maybe_prepend_hash_set_use(result);
            #[cfg(feature = "uuid")]
            let result = maybe_prepend_uuid_use(result);
            result
        }
    };

    let per_schema: Vec<Vec<u8>> = (0..schemas.len())
        .map(|schema_idx| {
            let mut local_structs: Vec<(String, JsonSchema)> = map
                .iter()
                .filter(|(_, (canonical_name, _, occs))| {
                    occs.len() == 1
                        && occs[0].0 == schema_idx
                        && !shared_names.contains(canonical_name)
                })
                .map(|(_, (name, schema, _))| (name.clone(), schema.clone()))
                .collect();
            let local_names: BTreeSet<String> =
                local_structs.iter().map(|(n, _)| n.clone()).collect();
            let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (name, schema) in &local_structs {
                let mut set: BTreeSet<String> = BTreeSet::new();
                for prop_schema in schema.properties.values() {
                    if prop_schema.is_object_with_properties() {
                        let prop_key = DedupeKey::from_schema(prop_schema, mode);
                        if let Some(cn) = key_to_canonical_name.get(&prop_key)
                            && (local_names.contains(cn) || shared_names.contains(cn))
                        {
                            set.insert(cn.clone());
                        }
                    }
                    if prop_schema.is_array_with_items()
                        && let Some(ref items) = prop_schema.items
                        && items.is_object_with_properties()
                    {
                        let item_key = DedupeKey::from_schema(items, mode);
                        if let Some(cn) = key_to_canonical_name.get(&item_key)
                            && (local_names.contains(cn) || shared_names.contains(cn))
                        {
                            set.insert(cn.clone());
                        }
                    }
                }
                deps.insert(name.clone(), set);
            }
            let order: Vec<String> = local_structs.iter().map(|(n, _)| n.clone()).collect();
            topo_sort_by_deps(&order, &deps, &mut local_structs);

            let mut used_shared: BTreeSet<String> = BTreeSet::new();
            for (_, schema) in &local_structs {
                for prop_schema in schema.properties.values() {
                    if prop_schema.is_object_with_properties() {
                        let prop_key = DedupeKey::from_schema(prop_schema, mode);
                        if let Some(cn) = key_to_canonical_name.get(&prop_key)
                            && shared_names.contains(cn)
                        {
                            used_shared.insert(cn.clone());
                        }
                    }
                    if prop_schema.is_array_with_items()
                        && let Some(ref items) = prop_schema.items
                        && items.is_object_with_properties()
                    {
                        let item_key = DedupeKey::from_schema(items, mode);
                        if let Some(cn) = key_to_canonical_name.get(&item_key)
                            && shared_names.contains(cn)
                        {
                            used_shared.insert(cn.clone());
                        }
                    }
                    if let Some(values) = string_enum_or_const_values(prop_schema)
                        && let Some((enum_name, _)) = enum_values_to_name.get(&values)
                    {
                        used_shared.insert(enum_name.clone());
                    }
                }
            }
            let root_for_schema = collected
                .iter()
                .rev()
                .find(|(idx, _, _)| *idx == schema_idx)
                .map(|(_, _, s)| DedupeKey::from_schema(s, mode));
            if let Some(root_key) = root_for_schema
                && let Some(cn) = key_to_canonical_name.get(&root_key)
                && shared_names.contains(cn)
            {
                used_shared.insert(cn.clone());
            }

            let mut buf = Cursor::new(Vec::new());
            writeln!(
                buf,
                "//! Generated by json-schema-rs. Do not edit manually."
            )
            .ok();
            writeln!(buf).ok();
            writeln!(buf, "use serde::{{Deserialize, Serialize}};").ok();
            for u in &used_shared {
                writeln!(buf, "pub use crate::{u};").ok();
            }
            if !used_shared.is_empty() {
                writeln!(buf).ok();
            }
            let root_schema: &JsonSchema = resolved_schemas
                .get(schema_idx)
                .expect("root schema for local emission");
            for (name, schema) in &local_structs {
                emit_default_functions_for_struct(&mut buf, name, schema).ok();
                emit_struct_derive_and_attrs(&mut buf, name, schema).ok();
                emit_struct_fields_with_resolver(
                    root_schema,
                    name,
                    schema,
                    &mut buf,
                    settings,
                    Some(&key_to_canonical_name),
                    mode,
                    Some(&enum_values_to_name),
                )
                .ok();
                writeln!(buf, "}}").ok();
                writeln!(buf).ok();
            }
            {
                let result = maybe_prepend_btreemap_use(buf.into_inner());
                let result = maybe_prepend_hash_set_use(result);
                #[cfg(feature = "uuid")]
                let result = maybe_prepend_uuid_use(result);
                result
            }
        })
        .collect();

    Ok(GenerateRustOutput {
        shared: Some(shared_buffer),
        per_schema,
    })
}

fn topo_sort_by_deps(
    order: &[String],
    deps: &BTreeMap<String, BTreeSet<String>>,
    v: &mut Vec<(String, JsonSchema)>,
) {
    let mut sorted: Vec<String> = Vec::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    for name in order {
        visit_topo(name, deps, &mut visited, &mut sorted);
    }
    let name_to_pair: BTreeMap<String, (String, JsonSchema)> =
        v.drain(..).map(|(n, s)| (n.clone(), (n, s))).collect();
    for n in &sorted {
        if let Some(pair) = name_to_pair.get(n) {
            v.push(pair.clone());
        }
    }
}

fn visit_topo(
    name: &str,
    deps: &BTreeMap<String, BTreeSet<String>>,
    visited: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) {
    if visited.contains(name) {
        return;
    }
    visited.insert(name.to_string());
    if let Some(d) = deps.get(name) {
        for dep in d {
            visit_topo(dep, deps, visited, out);
        }
    }
    out.push(name.to_string());
}

/// Emit a single Rust enum (unit variants). Pairs are (`json_value`, `variant_name`); serde rename when they differ.
/// Emits enum doc comment from description when present.
fn emit_enum_from_pairs(
    out: &mut impl Write,
    name: &str,
    pairs: &[(String, String)],
    description: Option<&str>,
) -> CodeGenResult<()> {
    for line in doc_lines(description) {
        writeln!(out, "/// {line}")?;
    }
    writeln!(
        out,
        "#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]"
    )?;
    writeln!(out, "pub enum {name} {{")?;
    for (value, variant_name) in pairs {
        if value != variant_name {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            writeln!(out, "    #[serde(rename = \"{escaped}\")]")?;
        }
        writeln!(out, "    {variant_name},")?;
    }
    writeln!(out, "}}")?;
    writeln!(out)?;
    Ok(())
}

/// Emit a single anyOf enum (union) to `out`.
/// We do not derive `ToJsonSchema` (macro supports only unit enums) or PartialEq/Eq (variant structs may not implement them).
fn emit_anyof_enum(out: &mut impl Write, a: &AnyOfEnumToEmit) -> CodeGenResult<()> {
    writeln!(out, "#[derive(Debug, Clone, Serialize, Deserialize)]")?;
    writeln!(out, "pub enum {} {{", a.name)?;
    for (variant_name, ty) in &a.variants {
        writeln!(out, "    {variant_name}({ty}),")?;
    }
    writeln!(out, "}}")?;
    writeln!(out)?;
    Ok(())
}

/// Emit a single oneOf enum (union) to `out`.
/// We do not derive `ToJsonSchema` (macro supports only unit enums) or PartialEq/Eq (variant structs may not implement them).
fn emit_oneof_enum(out: &mut impl Write, a: &OneOfEnumToEmit) -> CodeGenResult<()> {
    writeln!(out, "#[derive(Debug, Clone, Serialize, Deserialize)]")?;
    writeln!(out, "pub enum {} {{", a.name)?;
    for (variant_name, ty) in &a.variants {
        writeln!(out, "    {variant_name}({ty}),")?;
    }
    writeln!(out, "}}")?;
    writeln!(out)?;
    Ok(())
}

/// Emits `#[serde(default)]` or `#[serde(default = "fn")]` when the property has a default value.
/// Default functions are emitted at module level by `emit_default_functions_for_struct`.
fn emit_default_attr(
    out: &mut impl Write,
    struct_name: &str,
    field_name: &str,
    prop_schema: &JsonSchema,
    ty: &str,
    is_required: bool,
) -> CodeGenResult<()> {
    let Some(ref value) = prop_schema.default_value else {
        return Ok(());
    };
    let is_optional: bool = !is_required;
    if json_value_equals_rust_type_default(value, ty, is_optional) {
        writeln!(out, "    #[serde(default)]")?;
        return Ok(());
    }
    let Some(_expr) = json_default_to_rust_expr(value, ty, is_optional) else {
        return Ok(());
    };
    let fn_name: String = default_function_name(struct_name, field_name);
    writeln!(out, "    #[serde(default = \"{fn_name}\")]")?;
    Ok(())
}

/// Emits module-level default functions for all properties of the struct that have a custom default value.
fn emit_default_functions_for_struct(
    out: &mut impl Write,
    struct_name: &str,
    schema: &JsonSchema,
) -> CodeGenResult<()> {
    for (key, prop_schema) in &schema.properties {
        let Some(ref value) = prop_schema.default_value else {
            continue;
        };
        let field_name = sanitize_field_name(key);
        let is_required = schema.is_required(key);
        let is_optional = !is_required;
        let ty: String = if prop_schema.is_string() {
            if is_required {
                "String".to_string()
            } else {
                "Option<String>".to_string()
            }
        } else if prop_schema.is_integer() || prop_schema.is_number() {
            let inner = rust_numeric_type_for_schema(prop_schema);
            if is_required {
                inner
            } else {
                format!("Option<{inner}>")
            }
        } else {
            continue;
        };
        if json_value_equals_rust_type_default(value, &ty, is_optional) {
            continue;
        }
        let Some(expr) = json_default_to_rust_expr(value, &ty, is_optional) else {
            continue;
        };
        let fn_name = default_function_name(struct_name, &field_name);
        writeln!(out, "fn {fn_name}() -> {ty} {{ {expr} }}")?;
        writeln!(out)?;
    }
    Ok(())
}

/// Emit struct fields; when resolver is Some (dedupe mode), use canonical type names for nested objects.
#[expect(clippy::too_many_lines, clippy::too_many_arguments)]
fn emit_struct_fields_with_resolver(
    root: &JsonSchema,
    struct_name: &str,
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
    key_to_name: Option<&BTreeMap<DedupeKey, String>>,
    mode: DedupeMode,
    enum_values_to_name: Option<&EnumValuesToNameMap>,
) -> CodeGenResult<()> {
    let enum_names_simple: Option<BTreeMap<Vec<String>, String>> =
        enum_values_to_name.map(|m| m.iter().map(|(k, (n, _))| (k.clone(), n.clone())).collect());
    for (key, prop_schema) in &schema.properties {
        let (prop_schema_effective, _) = resolve_ref_for_codegen(root, prop_schema, Some(key))?;
        let prop_schema: &JsonSchema = &prop_schema_effective;

        for line in doc_lines(prop_schema.description.as_deref()) {
            writeln!(out, "    /// {line}")?;
        }
        let field_name = sanitize_field_name(key);
        let needs_rename = field_name != *key;

        if let Some(values) = string_enum_or_const_values(prop_schema) {
            let enum_name: &String = enum_values_to_name
                .and_then(|m| m.get(&values).map(|(n, _)| n))
                .expect("enum name for string enum");
            let ty = if schema.is_required(key) {
                enum_name.clone()
            } else {
                format!("Option<{enum_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_string()
            || (prop_schema
                .enum_values
                .as_ref()
                .is_some_and(|v| !v.is_empty())
                && !prop_schema.is_string_enum())
        {
            #[cfg(feature = "uuid")]
            if prop_schema.is_string() && prop_schema.format.as_deref() == Some("uuid") {
                let ty = if schema.is_required(key) {
                    "Uuid".to_string()
                } else {
                    "Option<Uuid>".to_string()
                };
                if needs_rename {
                    writeln!(out, "    #[serde(rename = \"{key}\")]")?;
                }
                writeln!(out, "    pub {field_name}: {ty},")?;
                continue;
            }
            // String type, or non-string/mixed enum fallback per design.
            let ty = if schema.is_required(key) {
                "String".to_string()
            } else {
                "Option<String>".to_string()
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            if prop_schema.min_length.is_some()
                || prop_schema.max_length.is_some()
                || prop_schema.pattern.is_some()
            {
                let mut attrs: Vec<String> = Vec::new();
                if let Some(n) = prop_schema.min_length {
                    attrs.push(format!("min_length = {n}"));
                }
                if let Some(n) = prop_schema.max_length {
                    attrs.push(format!("max_length = {n}"));
                }
                if let Some(ref p) = prop_schema.pattern {
                    let escaped = p.replace('\\', "\\\\").replace('"', "\\\"");
                    attrs.push(format!("pattern = \"{escaped}\""));
                }
                writeln!(out, "    #[json_schema({})]", attrs.join(", "))?;
            }
            emit_default_attr(
                out,
                struct_name,
                &field_name,
                prop_schema,
                &ty,
                schema.is_required(key),
            )?;
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_integer() || prop_schema.is_number() {
            let inner: String = rust_numeric_type_for_schema(prop_schema);
            let ty = if schema.is_required(key) {
                inner
            } else {
                format!("Option<{inner}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            emit_default_attr(
                out,
                struct_name,
                &field_name,
                prop_schema,
                &ty,
                schema.is_required(key),
            )?;
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_array_with_items() {
            let item_schema: &JsonSchema = prop_schema
                .items
                .as_ref()
                .expect("array with items")
                .as_ref();
            let inner: String = rust_type_for_item_schema(
                root,
                item_schema,
                Some(key),
                enum_names_simple.as_ref(),
                key_to_name,
                settings,
                mode,
            )?;
            let use_hash_set: bool =
                prop_schema.unique_items == Some(true) && item_schema_is_hashable(item_schema);
            let container: &str = if use_hash_set { "HashSet" } else { "Vec" };
            let ty = if schema.is_required(key) {
                format!("{container}<{inner}>")
            } else {
                format!("Option<{container}<{inner}>>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            if prop_schema.min_items.is_some() || prop_schema.max_items.is_some() {
                let mut attrs: Vec<String> = Vec::new();
                if let Some(n) = prop_schema.min_items {
                    attrs.push(format!("min_items = {n}"));
                }
                if let Some(n) = prop_schema.max_items {
                    attrs.push(format!("max_items = {n}"));
                }
                writeln!(out, "    #[json_schema({})]", attrs.join(", "))?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_object_with_properties() {
            let nested_name: String = if let Some(m) = key_to_name {
                let prop_key = DedupeKey::from_schema(prop_schema, mode);
                m.get(&prop_key).cloned().unwrap_or_else(|| {
                    struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings)
                })
            } else {
                struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings)
            };
            let ty = if schema.is_required(key) {
                nested_name.clone()
            } else {
                format!("Option<{nested_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        }
    }
    if let Some(AdditionalProperties::Schema(sub)) = &schema.additional_properties {
        let value_ty: String = rust_type_for_item_schema(
            root,
            sub,
            Some("additional"),
            enum_names_simple.as_ref(),
            key_to_name,
            settings,
            mode,
        )?;
        writeln!(out, "    #[serde(default)]")?;
        writeln!(out, "    pub additional: BTreeMap<String, {value_ty}>,")?;
    }
    Ok(())
}

/// Emit a single struct's fields to `out`.
#[expect(clippy::too_many_lines, clippy::too_many_arguments)]
fn emit_struct_fields(
    root: &JsonSchema,
    struct_name: &str,
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
    enum_values_to_name: Option<&BTreeMap<Vec<String>, String>>,
    _anyof_enums: Option<&[AnyOfEnumToEmit]>,
    _oneof_enums: Option<&[OneOfEnumToEmit]>,
) -> CodeGenResult<()> {
    for (key, prop_schema) in &schema.properties {
        let (prop_schema_effective, _) = resolve_ref_for_codegen(root, prop_schema, Some(key))?;
        let prop_schema: &JsonSchema = &prop_schema_effective;

        for line in doc_lines(prop_schema.description.as_deref()) {
            writeln!(out, "    /// {line}")?;
        }
        let field_name = sanitize_field_name(key);
        let needs_rename = field_name != *key;

        if prop_schema.any_of.as_ref().is_some_and(|v| !v.is_empty()) {
            let enum_name = sanitize_struct_name(key) + "AnyOf";
            let ty = if schema.is_required(key) {
                enum_name.clone()
            } else {
                format!("Option<{enum_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.one_of.as_ref().is_some_and(|v| !v.is_empty()) {
            let enum_name = sanitize_struct_name(key) + "OneOf";
            let ty = if schema.is_required(key) {
                enum_name.clone()
            } else {
                format!("Option<{enum_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if let Some(values) = string_enum_or_const_values(prop_schema) {
            let enum_name: &String = enum_values_to_name
                .and_then(|m| m.get(&values))
                .expect("enum name for string enum");
            let ty = if schema.is_required(key) {
                enum_name.clone()
            } else {
                format!("Option<{enum_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_string()
            || (prop_schema
                .enum_values
                .as_ref()
                .is_some_and(|v| !v.is_empty())
                && !prop_schema.is_string_enum())
        {
            #[cfg(feature = "uuid")]
            if prop_schema.is_string() && prop_schema.format.as_deref() == Some("uuid") {
                let ty = if schema.is_required(key) {
                    "Uuid".to_string()
                } else {
                    "Option<Uuid>".to_string()
                };
                if needs_rename {
                    writeln!(out, "    #[serde(rename = \"{key}\")]")?;
                }
                writeln!(out, "    pub {field_name}: {ty},")?;
                continue;
            }
            // String type, or non-string/mixed enum fallback per design.
            let ty = if schema.is_required(key) {
                "String".to_string()
            } else {
                "Option<String>".to_string()
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            if prop_schema.min_length.is_some()
                || prop_schema.max_length.is_some()
                || prop_schema.pattern.is_some()
            {
                let mut attrs: Vec<String> = Vec::new();
                if let Some(n) = prop_schema.min_length {
                    attrs.push(format!("min_length = {n}"));
                }
                if let Some(n) = prop_schema.max_length {
                    attrs.push(format!("max_length = {n}"));
                }
                if let Some(ref p) = prop_schema.pattern {
                    let escaped = p.replace('\\', "\\\\").replace('"', "\\\"");
                    attrs.push(format!("pattern = \"{escaped}\""));
                }
                writeln!(out, "    #[json_schema({})]", attrs.join(", "))?;
            }
            emit_default_attr(
                out,
                struct_name,
                &field_name,
                prop_schema,
                &ty,
                schema.is_required(key),
            )?;
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_integer() || prop_schema.is_number() {
            let inner: String = rust_numeric_type_for_schema(prop_schema);
            let ty = if schema.is_required(key) {
                inner
            } else {
                format!("Option<{inner}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            emit_default_attr(
                out,
                struct_name,
                &field_name,
                prop_schema,
                &ty,
                schema.is_required(key),
            )?;
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_array_with_items() {
            let item_schema: &JsonSchema = prop_schema
                .items
                .as_ref()
                .expect("array with items")
                .as_ref();
            let inner: String = rust_type_for_item_schema(
                root,
                item_schema,
                Some(key),
                enum_values_to_name,
                None,
                settings,
                DedupeMode::Full,
            )?;
            let use_hash_set: bool =
                prop_schema.unique_items == Some(true) && item_schema_is_hashable(item_schema);
            let container: &str = if use_hash_set { "HashSet" } else { "Vec" };
            let ty = if schema.is_required(key) {
                format!("{container}<{inner}>")
            } else {
                format!("Option<{container}<{inner}>>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            if prop_schema.min_items.is_some() || prop_schema.max_items.is_some() {
                let mut attrs: Vec<String> = Vec::new();
                if let Some(n) = prop_schema.min_items {
                    attrs.push(format!("min_items = {n}"));
                }
                if let Some(n) = prop_schema.max_items {
                    attrs.push(format!("max_items = {n}"));
                }
                writeln!(out, "    #[json_schema({})]", attrs.join(", "))?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_object_with_properties() {
            let nested_name: String =
                struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings);
            let ty = if schema.is_required(key) {
                nested_name.clone()
            } else {
                format!("Option<{nested_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        }
    }
    if let Some(AdditionalProperties::Schema(sub)) = &schema.additional_properties {
        let value_ty: String = rust_type_for_item_schema(
            root,
            sub,
            Some("additional"),
            enum_values_to_name,
            None,
            settings,
            settings.dedupe_mode,
        )?;
        writeln!(out, "    #[serde(default)]")?;
        writeln!(out, "    pub additional: BTreeMap<String, {value_ty}>,")?;
    }
    Ok(())
}

/// Emit Rust source from a parsed schema to `out`. Used by [`RustBackend::generate`].
fn emit_rust(
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
) -> CodeGenResult<()> {
    let root_unresolved = resolve_all_of_for_codegen(schema)?;
    let (root, root_from_key) = resolve_ref_for_codegen(schema, &root_unresolved, None)?;
    if root.any_of.as_ref().is_some_and(std::vec::Vec::is_empty) {
        return Err(CodeGenError::AnyOfEmpty);
    }
    if root.one_of.as_ref().is_some_and(std::vec::Vec::is_empty) {
        return Err(CodeGenError::OneOfEmpty);
    }
    let roots_for_structs: Vec<JsonSchema> = if root.one_of.as_ref().is_some_and(|v| !v.is_empty())
    {
        root.one_of
            .as_ref()
            .unwrap()
            .iter()
            .map(resolve_all_of_for_codegen)
            .collect::<CodeGenResult<Vec<_>>>()?
    } else if root.any_of.as_ref().is_some_and(|v| !v.is_empty()) {
        root.any_of
            .as_ref()
            .unwrap()
            .iter()
            .map(resolve_all_of_for_codegen)
            .collect::<CodeGenResult<Vec<_>>>()?
    } else {
        if !root.is_object_with_properties() {
            return Err(CodeGenError::RootNotObject);
        }
        vec![root.clone()]
    };

    let enums: Vec<EnumToEmit> = collect_enums(schema, &root, settings)?;
    let enum_values_to_name: BTreeMap<Vec<String>, String> = enums
        .iter()
        .map(|e| (e.values.clone(), e.name.clone()))
        .collect();

    let anyof_enums: Vec<AnyOfEnumToEmit> =
        collect_anyof_enums(schema, &root, settings, &enum_values_to_name)?;
    let oneof_enums: Vec<OneOfEnumToEmit> =
        collect_oneof_enums(schema, &root, settings, &enum_values_to_name)?;

    let mut structs: Vec<StructToEmit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let root_is_anyof = root.any_of.as_ref().is_some_and(|v| !v.is_empty());
    let root_is_oneof = root.one_of.as_ref().is_some_and(|v| !v.is_empty());
    for (i, r) in roots_for_structs.iter().enumerate() {
        let from_key: Option<String> = if root_is_anyof || root_is_oneof {
            Some(format!("Root_Variant{i}"))
        } else {
            root_from_key.clone()
        };
        collect_structs(
            schema,
            r,
            from_key.as_deref(),
            &mut structs,
            &mut seen,
            settings,
        )?;
    }

    writeln!(
        out,
        "//! Generated by json-schema-rs. Do not edit manually."
    )?;
    writeln!(out)?;
    writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
    writeln!(out)?;

    for e in &enums {
        let pairs: Vec<(String, String)> = enum_variant_names_with_collision_resolution(&e.values);
        emit_enum_from_pairs(out, &e.name, &pairs, e.description.as_deref())?;
    }

    for a in &anyof_enums {
        emit_anyof_enum(out, a)?;
    }

    for o in &oneof_enums {
        emit_oneof_enum(out, o)?;
    }

    for st in &structs {
        emit_default_functions_for_struct(out, &st.name, &st.schema)?;
        emit_struct_derive_and_attrs(out, &st.name, &st.schema)?;
        emit_struct_fields(
            schema,
            &st.name,
            &st.schema,
            out,
            settings,
            Some(&enum_values_to_name),
            Some(&anyof_enums),
            Some(&oneof_enums),
        )?;
        writeln!(out, "}}")?;
        writeln!(out)?;
    }

    Ok(())
}

/// Generate Rust source from one or more parsed schemas.
///
/// Callers must pass `settings` (use [`CodeGenSettings::builder`] and call [`CodeGenSettingsBuilder::build`]
/// for all-default settings). Returns [`GenerateRustOutput`] with optional shared buffer and one buffer per schema.
/// Root of each schema must have `type: "object"` and non-empty `properties`.
///
/// # Errors
///
/// Returns [`CodeGenError::RootNotObject`] if a root schema is not an object with properties.
/// Returns [`CodeGenError::Io`] on write failure.
/// Returns [`CodeGenError::Batch`] with index when one schema in the batch fails.
pub fn generate_rust(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> CodeGenResult<GenerateRustOutput> {
    RustBackend.generate(schemas, settings)
}

#[cfg(test)]
mod tests {
    use super::CodeGenError;
    use super::{CodeGenBackend, RustBackend, generate_rust, merge_all_of};
    use crate::code_gen::settings::{CodeGenSettings, DedupeMode, ModelNameSource};
    use crate::json_schema::JsonSchema;

    fn default_settings() -> CodeGenSettings {
        CodeGenSettings::builder().build()
    }

    #[test]
    fn root_not_object_errors() {
        let schema: JsonSchema = JsonSchema::default();
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::RootNotObject));
        }
    }

    #[test]
    fn root_object_empty_properties_errors() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            ..Default::default()
        };
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::RootNotObject));
        }
    }

    #[test]
    fn schema_with_id_emits_id_attribute() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = concat!(
            "//! Generated by json-schema-rs. Do not edit manually.\n\n",
            "use serde::{Deserialize, Serialize};\n\n",
            "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]\n",
            "#[json_schema(id = \"http://example.com/schema\")]\n",
            "pub struct Root {\n    pub name: Option<String>,\n}\n\n"
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_emits_deny_unknown_fields() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":false}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = concat!(
            "//! Generated by json-schema-rs. Do not edit manually.\n\n",
            "use serde::{Deserialize, Serialize};\n\n",
            "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]\n",
            "#[serde(deny_unknown_fields)]\n",
            "pub struct Root {\n    pub name: Option<String>,\n}\n\n"
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_emits_map_field() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":{"type":"string"}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = concat!(
            "//! Generated by json-schema-rs. Do not edit manually.\n\n",
            "use serde::{Deserialize, Serialize};\n",
            "use std::collections::BTreeMap;\n\n",
            "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]\n",
            "pub struct Root {\n",
            "    pub name: Option<String>,\n",
            "    #[serde(default)]\n",
            "    pub additional: BTreeMap<String, String>,\n",
            "}\n\n"
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_string_property() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = concat!(
            "//! Generated by json-schema-rs. Do not edit manually.\n\n",
            "use serde::{Deserialize, Serialize};\n\n",
            "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]\n",
            "pub struct Root {\n    pub name: Option<String>,\n}\n\n"
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn string_property_with_pattern_emits_attribute() {
        let json =
            r#"{"type":"object","properties":{"name":{"type":"string","pattern":"^[a-z]+$"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(pattern = "^[a-z]+$")]
    pub name: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_field_emits_without_option() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_defs_object_emits_named_struct_and_field_type() {
        let json = r##"{
  "$defs": {
    "Address": {
      "type": "object",
      "properties": { "city": { "type": "string" } },
      "required": ["city"]
    }
  },
  "type": "object",
  "properties": { "address": { "$ref": "#/$defs/Address" } },
  "required": ["address"]
}"##;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub address: Address,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_missing_defs_returns_ref_resolution_error() {
        let json = r##"{
  "type": "object",
  "properties": { "x": { "$ref": "#/$defs/Missing" } }
}"##;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual: super::CodeGenResult<super::GenerateRustOutput> =
            generate_rust(&[schema], &settings);
        assert!(
            matches!(
                actual,
                Err(CodeGenError::RefResolution { ref ref_str, ref reason })
                if ref_str == "#/$defs/Missing" && reason.contains("DefsMissing")
            ),
            "expected RefResolution with DefsMissing, got: {actual:?}"
        );
    }

    #[test]
    fn ref_cycle_in_defs_returns_ref_resolution_error() {
        let json = r##"{
  "$defs": {
    "A": { "$ref": "#/$defs/B" },
    "B": { "$ref": "#/$defs/A" }
  },
  "type": "object",
  "properties": { "x": { "$ref": "#/$defs/A" } }
}"##;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual: super::CodeGenResult<super::GenerateRustOutput> =
            generate_rust(&[schema], &settings);
        assert!(
            matches!(
                actual,
                Err(CodeGenError::RefResolution { ref ref_str, ref reason })
                if ref_str == "#/$defs/A" && reason.contains("RefCycle")
            ),
            "expected RefResolution with RefCycle, got: {actual:?}"
        );
    }

    #[test]
    fn required_enum_property_emits_enum_and_struct() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}},"required":["status"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Status {
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub status: Status,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_string_property_emits_single_variant_enum() {
        let json = r#"{"type":"object","properties":{"key":{"const":"only"}},"required":["key"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Key {
    #[serde(rename = "only")]
    Only,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub key: Key,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_merge_same_const_ok() {
        let s1: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","const":"same"}}}"#,
        )
        .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","const":"same"}}}"#,
        )
        .unwrap();
        let actual: Result<JsonSchema, _> = merge_all_of(&[s1, s2]);
        assert!(actual.is_ok());
        let merged: JsonSchema = actual.unwrap();
        let x_schema: &JsonSchema = merged.properties.get("x").expect("property x");
        assert_eq!(
            x_schema.const_value.as_ref(),
            Some(&serde_json::Value::String("same".to_string()))
        );
    }

    #[test]
    fn all_of_merge_conflicting_const_err() {
        let s1: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","const":"a"}}}"#,
        )
        .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","const":"b"}}}"#,
        )
        .unwrap();
        let actual: Result<JsonSchema, CodeGenError> = merge_all_of(&[s1, s2]);
        let err = actual.expect_err("expected AllOfMergeConflictingConst");
        assert!(matches!(
            err,
            CodeGenError::AllOfMergeConflictingConst { .. }
        ));
    }

    #[test]
    fn all_of_merge_same_pattern_ok() {
        let s1: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}}"#,
        )
        .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}}"#,
        )
        .unwrap();
        let merged: JsonSchema = merge_all_of(&[s1, s2]).expect("merge ok");
        let actual: JsonSchema = merged.properties.get("x").cloned().expect("property x");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            pattern: Some("^[a-z]+$".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_merge_conflicting_pattern_err() {
        let s1: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}}"#,
        )
        .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string","pattern":"^[0-9]+$"}}}"#,
        )
        .unwrap();
        let actual: Result<JsonSchema, CodeGenError> = merge_all_of(&[s1, s2]);
        assert!(matches!(
            actual,
            Err(CodeGenError::AllOfMergeConflictingPattern { .. })
        ));
    }

    #[test]
    fn all_of_merged_object_golden() {
        let json = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}},"required":["b"]}]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub a: Option<String>,
    pub b: i64,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn anyof_property_golden() {
        let json = r#"{"type":"object","properties":{"foo":{"anyOf":[{"type":"string"},{"type":"object","properties":{"x":{"type":"integer"}}}]}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FooAnyOf {
    Variant0(String),
    Variant1(FooVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooVariant1 {
    pub x: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub foo: Option<FooAnyOf>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn anyof_root_golden() {
        let json = r#"{"anyOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RootAnyOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    pub a: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    pub b: Option<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn anyof_empty_errors() {
        let schema: JsonSchema = JsonSchema {
            any_of: Some(vec![]),
            ..Default::default()
        };
        let settings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::AnyOfEmpty));
        }
    }

    #[test]
    fn oneof_property_golden() {
        let json = r#"{"type":"object","properties":{"foo":{"oneOf":[{"type":"string"},{"type":"object","properties":{"x":{"type":"integer"}}}]}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FooOneOf {
    Variant0(String),
    Variant1(FooVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooVariant1 {
    pub x: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub foo: Option<FooOneOf>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn oneof_root_golden() {
        let json = r#"{"oneOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RootOneOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    pub a: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    pub b: Option<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn oneof_empty_errors() {
        let schema: JsonSchema = JsonSchema {
            one_of: Some(vec![]),
            ..Default::default()
        };
        let settings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::OneOfEmpty));
        }
    }

    #[test]
    fn merge_all_of_success_two_object_subschemas() {
        let s1: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"a":{"type":"string"}}}"#)
                .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"b":{"type":"integer"}},"required":["b"]}"#,
        )
        .unwrap();
        let actual = merge_all_of(&[s1, s2]).unwrap();
        let expected: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["b"]}"#,
        )
        .unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn merge_all_of_empty_errors() {
        let actual = merge_all_of(&[]);
        assert!(matches!(actual, Err(CodeGenError::AllOfMergeEmpty)));
    }

    #[test]
    fn merge_all_of_single_schema() {
        let s: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"x":{"type":"string"}}}"#)
                .unwrap();
        let expected = s.clone();
        let actual = merge_all_of(std::slice::from_ref(&s)).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn merge_all_of_conflicting_property_type_errors() {
        let s1: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"k":{"type":"string"}}}"#)
                .unwrap();
        let s2: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"k":{"type":"integer"}}}"#)
                .unwrap();
        let actual = merge_all_of(&[s1, s2]);
        assert!(matches!(
            actual,
            Err(CodeGenError::AllOfMergeConflictingPropertyType {
                property_key: ref k,
                ..
            }) if k == "k"
        ));
    }

    #[test]
    fn merge_all_of_non_object_subschema_errors() {
        let s1: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"a":{"type":"string"}}}"#)
                .unwrap();
        let s2: JsonSchema = serde_json::from_str(r#"{"type":"string"}"#).unwrap();
        let actual = merge_all_of(&[s1, s2]);
        assert!(matches!(
            actual,
            Err(CodeGenError::AllOfMergeNonObjectSubschema { index: 1 })
        ));
    }

    #[test]
    fn merge_all_of_conflicting_enum_errors() {
        let s1: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"s":{"enum":["a","b"]}}}"#)
                .unwrap();
        let s2: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"s":{"enum":["x","y"]}}}"#)
                .unwrap();
        let actual = merge_all_of(&[s1, s2]);
        assert!(matches!(
            actual,
            Err(CodeGenError::AllOfMergeConflictingEnum { property_key: ref k }) if k == "s"
        ));
    }

    #[test]
    fn merge_all_of_conflicting_numeric_bounds_errors() {
        let s1: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":10}}}"#,
        )
        .unwrap();
        let s2: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"n":{"type":"integer","minimum":20,"maximum":30}}}"#,
        )
        .unwrap();
        let actual = merge_all_of(&[s1, s2]);
        assert!(matches!(
            actual,
            Err(CodeGenError::AllOfMergeConflictingNumericBounds {
                property_key: ref k,
                keyword: ref w
            }) if k == "n" && w == "minimum/maximum"
        ));
    }

    #[test]
    fn batch_error_when_allof_merge_fails_in_second_schema() {
        let s0: JsonSchema =
            serde_json::from_str(r#"{"type":"object","properties":{"a":{"type":"string"}}}"#)
                .unwrap();
        let s1_bad: JsonSchema = serde_json::from_str(
            r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string"}}},{"type":"object","properties":{"x":{"type":"integer"}}}]}"#,
        )
        .unwrap();
        let settings = default_settings();
        let actual = generate_rust(&[s0.clone(), s1_bad], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 1, .. }));
    }

    #[test]
    fn root_all_of_merges_to_empty_object_errors_with_root_not_object() {
        let schema: JsonSchema =
            serde_json::from_str(r#"{"allOf":[{"type":"object"},{"type":"object"}]}"#).unwrap();
        let settings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(
            matches!(actual, CodeGenError::Batch { index: 0, source: ref s } if matches!(**s, CodeGenError::RootNotObject)),
            "expected Batch {{ index: 0, source: RootNotObject }}, got {actual:?}"
        );
    }

    #[test]
    fn optional_enum_property_emits_enum_and_struct() {
        let json = r#"{"type":"object","properties":{"level":{"enum":["low","medium","high"]}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Level {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub level: Option<Level>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_dedupe_two_properties_same_enum_emits_one_enum() {
        let json = r#"{"type":"object","properties":{"a":{"enum":["x","y"]},"b":{"enum":["x","y"]}},"required":["a"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum A {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "y")]
    Y,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub a: A,
    pub b: Option<A>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_collision_emits_suffixed_variants() {
        let json = r#"{"type":"object","properties":{"t":{"enum":["a","A"]}},"required":["t"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum T {
    #[serde(rename = "A")]
    A_0,
    #[serde(rename = "a")]
    A_1,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub t: T,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_duplicate_values_in_schema_emits_single_variant_per_value() {
        let json = r#"{"type":"object","properties":{"t":{"enum":["A","A","A","a","a","a","a","a","a","a"]}},"required":["t"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum T {
    #[serde(rename = "A")]
    A_0,
    #[serde(rename = "a")]
    A_1,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub t: T,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_string_enum_fallback_emits_string() {
        let json =
            r#"{"type":"object","properties":{"tag":{"enum":["foo",1,true]}},"required":["tag"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tag: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_required_integer_property() {
        let json =
            r#"{"type":"object","properties":{"count":{"type":"integer"}},"required":["count"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: i64,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_optional_integer_property() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: Option<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_required_float_property() {
        let json =
            r#"{"type":"object","properties":{"value":{"type":"number"}},"required":["value"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_optional_float_property() {
        let json = r#"{"type":"object","properties":{"value":{"type":"number"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: Option<f64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_minimum_maximum_u8_range_emits_u8() {
        let json = r#"{"type":"object","properties":{"byte":{"type":"integer","minimum":0,"maximum":255}},"required":["byte"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub byte: u8,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_only_minimum_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer","minimum":0}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: Option<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_only_maximum_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer","maximum":100}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: Option<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_without_min_max_emits_f64_fallback() {
        let json =
            r#"{"type":"object","properties":{"value":{"type":"number"}},"required":["value"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_f32_range_emits_f32() {
        let json = r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f32,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_required_string_property() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}},"required":["tags"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tags: Vec<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_unique_items_true_emits_hash_set_string() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            actual.contains("pub tags: HashSet<String>"),
            "expected HashSet<String>: {actual}"
        );
        assert!(
            actual.contains(concat!("use std::collections::", "HashSet")),
            "expected HashSet use: {actual}"
        );
    }

    #[test]
    fn array_with_unique_items_false_emits_vec_string() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":false}},"required":["tags"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tags: Vec<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_unique_items_true_object_items_emits_vec() {
        let json = r#"{"type":"object","properties":{"items":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"}}},"uniqueItems":true}},"required":["items"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            actual.contains("pub items: Vec<") && actual.contains(">,"),
            "uniqueItems true with object items should emit Vec: {actual}"
        );
        assert!(
            !actual.contains("HashSet"),
            "should not use HashSet for object items: {actual}"
        );
    }

    #[test]
    fn array_with_min_items_max_items_emits_attribute() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}},"required":["tags"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(min_items = 2, max_items = 5)]
    pub tags: Vec<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_min_items_only_emits_attribute() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["tags"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            actual.contains("#[json_schema(min_items = 1)]"),
            "expected min_items attribute: {actual}"
        );
        assert!(
            !actual.contains("max_items"),
            "should not emit max_items when absent: {actual}"
        );
    }

    #[test]
    fn array_optional_string_property() {
        let json =
            r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tags: Option<Vec<String>>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_integers_property() {
        let json = r#"{"type":"object","properties":{"counts":{"type":"array","items":{"type":"integer"}}},"required":["counts"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub counts: Vec<i64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_objects_property() {
        let json = r#"{"type":"object","properties":{"items":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"}}}}},"required":["items"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Items {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub items: Vec<Items>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_arrays_property() {
        let json = r#"{"type":"object","properties":{"matrix":{"type":"array","items":{"type":"array","items":{"type":"string"}}}},"required":["matrix"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub matrix: Vec<Vec<String>>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_string_integer_float_properties() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"},"count":{"type":"integer"},"value":{"type":"number"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: Option<i64>,
    pub id: String,
    pub value: Option<f64>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_string_and_integer_properties() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"},"count":{"type":"integer"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: Option<i64>,
    pub id: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_object_and_rename() {
        let json = r#"{
          "type": "object",
          "properties": {
            "first_name": { "type": "string" },
            "address": {
              "type": "object",
              "properties": {
                "street_address": { "type": "string" },
                "city": { "type": "string" }
              }
            }
          }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: Option<String>,
    pub street_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub address: Option<Address>,
    pub first_name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn full_example_from_plan() {
        let json = r#"{
          "type": "object",
          "properties": {
            "first_name": { "type": "string" },
            "last_name": { "type": "string" },
            "birthday": { "type": "string" },
            "address": {
              "type": "object",
              "properties": {
                "street_address": { "type": "string" },
                "city": { "type": "string" },
                "state": { "type": "string" },
                "country": { "type": "string" }
              }
            }
          }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub street_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub address: Option<Address>,
    pub birthday: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn deeply_nested_schema_does_not_stack_overflow() {
        const DEPTH: usize = 150;
        let mut inner: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            title: Some("Leaf".to_string()),
            ..Default::default()
        };
        for i in (0..DEPTH).rev() {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some("object".to_string()),
                title: Some(format!("Level{i}")),
                ..Default::default()
            };
            wrap.properties.insert("child".to_string(), inner);
            inner = wrap;
        }
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[inner], &settings);
        assert!(actual.is_ok(), concat!("deep schema must not ", "overflow"));
        let out = actual.unwrap();
        let output: String = String::from_utf8(out.per_schema[0].clone()).unwrap();
        assert!(
            output.contains(concat!("pub struct ", "Level0")),
            concat!("output must contain root ", "struct")
        );
        assert!(
            output.contains(concat!("pub struct ", "Leaf")),
            concat!("output must contain leaf ", "struct")
        );
    }

    #[test]
    fn field_rename_when_key_differs_from_identifier() {
        let json = r#"{"type":"object","properties":{"foo-bar":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "foo-bar")]
    pub foo_bar: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn generate_rust_one_schema_returns_one_buffer() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput =
            generate_rust(std::slice::from_ref(&schema), &settings).unwrap();
        let expected: super::GenerateRustOutput =
            RustBackend.generate(&[schema], &settings).unwrap();
        assert_eq!(expected.per_schema, output.per_schema);
        assert_eq!(1, output.per_schema.len());
    }

    #[test]
    fn property_key_first_uses_key_over_title_for_nested_struct() {
        let json = r#"{"type":"object","properties":{"address":{"type":"object","title":"FooBar","properties":{"city":{"type":"string"}}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .model_name_source(ModelNameSource::PropertyKeyFirst)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            actual.contains(concat!("pub struct ", "Address")),
            "with PropertyKeyFirst nested struct should be named from key address -> Address; got: {actual}"
        );
        assert!(
            !actual.contains(concat!("struct ", "FooBar")),
            "with PropertyKeyFirst title FooBar should not be used for nested name; got: {actual}"
        );
    }

    #[test]
    fn generate_rust_two_schemas_returns_two_buffers() {
        let json1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let json2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json1).unwrap();
        let s2: JsonSchema = serde_json::from_str(json2).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput =
            generate_rust(&[s1.clone(), s2.clone()], &settings).unwrap();
        let expected: super::GenerateRustOutput =
            RustBackend.generate(&[s1, s2], &settings).unwrap();
        assert_eq!(expected.per_schema, output.per_schema);
        assert_eq!(2, output.per_schema.len());
        let out1 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let out2 = String::from_utf8(output.per_schema[1].clone()).unwrap();
        assert!(out1.contains("pub a: Option<String>") || out1.contains("pub a:"));
        assert!(out2.contains("pub b: Option<String>") || out2.contains("pub b:"));
    }

    #[test]
    fn batch_error_includes_index() {
        let valid = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let invalid: JsonSchema = JsonSchema::default();
        let s1: JsonSchema = serde_json::from_str(valid).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[s1, invalid], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 1, .. }));
    }

    #[test]
    fn dedupe_disabled_returns_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema.clone(), schema], &settings).unwrap();
        let expected: Option<Vec<u8>> = None;
        let actual = output.shared.clone();
        assert_eq!(expected, actual);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn dedupe_disabled_two_schemas_same_shape_two_buffers_no_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        assert_eq!(None, output.shared);
        assert_eq!(2, output.per_schema.len());
        let out0 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let out1 = String::from_utf8(output.per_schema[1].clone()).unwrap();
        let root_struct: &str = concat!("pub struct ", "Root");
        assert!(out0.contains(root_struct));
        assert!(out1.contains(root_struct));
    }

    #[test]
    fn dedupe_full_two_identical_schemas_produces_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
        assert_eq!(2, output.per_schema.len());
        let shared_str = String::from_utf8(output.shared.unwrap()).unwrap();
        let root_struct: &str = concat!("pub struct ", "Root");
        assert!(shared_str.contains(root_struct));
        let per0 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let root_use: &str = concat!("pub use crate::", "Root");
        let root_only: &str = "Root";
        assert!(per0.contains(root_use) || per0.contains(root_only));
    }

    #[test]
    fn dedupe_full_single_schema_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let expected: Option<Vec<u8>> = None;
        let actual = output.shared.clone();
        assert_eq!(expected, actual);
        assert_eq!(1, output.per_schema.len());
    }

    #[test]
    fn dedupe_full_two_different_schemas_no_shared() {
        let j1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let j2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        assert_eq!(None, output.shared);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn dedupe_full_single_schema_two_identical_nested_objects_deduped() {
        let json = r#"{
            "type": "object",
            "properties": {
                "addr1": { "type": "object", "properties": { "street": { "type": "string" } } },
                "addr2": { "type": "object", "properties": { "street": { "type": "string" } } }
            }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let per_str = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            per_str.contains("addr1") && per_str.contains("addr2"),
            "per_schema should reference both fields; got: {per_str}"
        );
        let shared_count = output
            .shared
            .as_ref()
            .map_or(0, |b| b.windows(11).filter(|w| w == b"pub struct ").count());
        let per_count = per_str.matches("pub struct ").count();
        assert!(
            shared_count + per_count >= 1,
            "at least one struct (Root in per_schema, nested in shared when deduped)"
        );
    }

    #[test]
    fn description_root_struct_single_line() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"A root type"}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

/// A root type
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_root_struct_multi_line() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}},"description":"Line one\nLine two"}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

/// Line one
/// Line two
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub x: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_empty_or_whitespace_emits_no_doc() {
        let json_empty =
            r#"{"type":"object","properties":{"a":{"type":"string"}},"description":""}"#;
        let schema_empty: JsonSchema = serde_json::from_str(json_empty).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema_empty], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub a: Option<String>,
}

";
        assert_eq!(expected, actual);

        let json_ws =
            r#"{"type":"object","properties":{"a":{"type":"string"}},"description":"   \n  "}"#;
        let schema_ws: JsonSchema = serde_json::from_str(json_ws).unwrap();
        let output_ws: super::GenerateRustOutput = generate_rust(&[schema_ws], &settings).unwrap();
        let actual_ws = String::from_utf8(output_ws.per_schema[0].clone()).unwrap();
        assert_eq!(expected, actual_ws);
    }

    #[test]
    fn description_nested_object_struct_doc() {
        let json = r#"{"type":"object","properties":{"nested":{"type":"object","description":"Inner type","properties":{"v":{"type":"string"}}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

/// Inner type
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Nested {
    pub v: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Inner type
    pub nested: Option<Nested>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_property_field_doc() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string","description":"User full name"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// User full name
    pub name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_enum_property_emits_enum_doc() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"],"description":"Issue status"}},"required":["status"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

/// Issue status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Status {
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Issue status
    pub status: Status,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_description_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"First"}"#;
        let j2 =
            r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"Second"}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
    }

    #[test]
    fn dedupe_full_same_shape_different_description_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"First"}"#;
        let j2 =
            r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"Second"}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = false;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn dedupe_functional_same_shape_different_comment_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"First"}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"Second"}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
    }

    #[test]
    fn dedupe_full_same_shape_different_comment_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"First"}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"Second"}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = false;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn examples_golden_same_rust_with_or_without() {
        let json_without: &str =
            r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"]}"#;
        let json_with: &str = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":["foo"]}"#;
        let schema_without: JsonSchema = serde_json::from_str(json_without).unwrap();
        let schema_with: JsonSchema = serde_json::from_str(json_with).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::default();
        let output_without: super::GenerateRustOutput =
            generate_rust(&[schema_without], &settings).unwrap();
        let output_with: super::GenerateRustOutput =
            generate_rust(&[schema_with], &settings).unwrap();
        let expected: String = String::from_utf8(output_without.per_schema[0].clone()).unwrap();
        let actual: String = String::from_utf8(output_with.per_schema[0].clone()).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_same_shape_different_examples_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[1]}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[2]}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected: (bool, usize) = (false, 2);
        let actual: (bool, usize) = (output.shared.is_some(), output.per_schema.len());
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_examples_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[1]}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[2]}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
    }

    #[test]
    fn dedupe_full_same_shape_different_id_two_structs() {
        let j1 = r#"{"$id":"http://example.com/a","type":"object","properties":{"x":{"type":"string"}}}"#;
        let j2 = r#"{"$id":"http://example.com/b","type":"object","properties":{"x":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = false;
        let actual_shared_some = output.shared.is_some();
        let msg: &str = concat!(
            "Full dedupe: same shape different id yields two ",
            "structs"
        );
        assert_eq!(expected_shared_some, actual_shared_some, "{msg}");
    }

    #[test]
    fn dedupe_functional_same_shape_different_id_one_struct() {
        let j1 = r#"{"$id":"http://example.com/a","type":"object","properties":{"x":{"type":"string"}}}"#;
        let j2 = r#"{"$id":"http://example.com/b","type":"object","properties":{"x":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        let msg: &str = concat!(
            "Functional dedupe: same shape different id yields one shared ",
            "struct"
        );
        assert_eq!(expected_shared_some, actual_shared_some, "{msg}");
    }

    #[test]
    fn dedupe_functional_same_as_full_when_no_description() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings_full: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let settings_func: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output_full: super::GenerateRustOutput =
            generate_rust(&[s1.clone(), s2.clone()], &settings_full).unwrap();
        let output_func: super::GenerateRustOutput =
            generate_rust(&[s1, s2], &settings_func).unwrap();
        assert_eq!(output_full.shared.is_some(), output_func.shared.is_some());
        assert_eq!(output_full.per_schema.len(), output_func.per_schema.len());
    }

    #[test]
    fn default_settings_use_full_dedupe() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected = DedupeMode::Full;
        let actual = settings.dedupe_mode;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_required_property() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: Uuid,
}

";
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_optional_property() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: Option<Uuid>,
}

";
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_array_items() {
        let json = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"type":"string","format":"uuid"}}},"required":["ids"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected = "//! Generated by json-schema-rs. Do not edit manually.\n\nuse serde::{\"Deserialize\", \"Serialize\"};\nuse uuid::Uuid;\n\n#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]\npub struct Root {\n    pub ids: Vec<Uuid>,\n}\n\n";
        assert_eq!(expected, actual);
    }
}
