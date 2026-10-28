//! In-memory representation of JSON Schema for codegen.

use super::dialect::{Dialect, ExclusiveBounds, ResolvedDialect, SubschemaPosition};
use super::error::{JsonSchemaParseError, JsonSchemaParseResult};
use super::settings::{JsonSchemaSettings, resolve_dialect, resolve_dialect_for_uri};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;

/// Returns true when `required` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_required(v: &Option<Vec<String>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when a defs/definitions map should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_defs_map(v: &Option<BTreeMap<String, JsonSchema>>) -> bool {
    v.as_ref().is_none_or(BTreeMap::is_empty)
}

/// Returns true when `enum_values` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_enum_values(v: &Option<Vec<serde_json::Value>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `all_of` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_all_of(v: &Option<Vec<JsonSchema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `any_of` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_any_of(v: &Option<Vec<JsonSchema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `one_of` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_one_of(v: &Option<Vec<JsonSchema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when `additional_properties` should be omitted from serialized output (None).
#[expect(clippy::ref_option)]
fn skip_additional_properties(v: &Option<AdditionalProperties>) -> bool {
    v.is_none()
}

/// Value of the JSON Schema `additionalProperties` keyword: allow (true), forbid (false), or a schema for additional property values.
#[derive(Debug, Clone, PartialEq)]
pub enum AdditionalProperties {
    /// Additional properties are allowed (keyword `true` or absent).
    Allow,
    /// No additional properties allowed (keyword `false`).
    Forbid,
    /// Each additional property's value must validate against this schema.
    Schema(Box<JsonSchema>),
}

impl Serialize for AdditionalProperties {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            AdditionalProperties::Allow => serializer.serialize_bool(true),
            AdditionalProperties::Forbid => serializer.serialize_bool(false),
            AdditionalProperties::Schema(s) => s.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for AdditionalProperties {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;
        match value {
            serde_json::Value::Bool(true) => Ok(AdditionalProperties::Allow),
            serde_json::Value::Bool(false) => Ok(AdditionalProperties::Forbid),
            serde_json::Value::Object(_) => {
                let schema: JsonSchema =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(AdditionalProperties::Schema(Box::new(schema)))
            }
            _ => Err(serde::de::Error::custom(
                "additionalProperties must be a boolean or a schema object",
            )),
        }
    }
}

/// Deserializes a keyword whose presence matters even when its value is `null` (`"const": null`):
/// a present key is always `Some`, so `null` becomes `Some(Value::Null)` rather than `None`.
fn deserialize_present<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
}

/// JSON Schema type keyword: either a single type string or an array of types (draft 2020-12).
/// First type in the array is used; codegen uses `object` and `string`.
fn deserialize_type_optional<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TypeOrArray {
        Single(String),
        Array(Vec<String>),
    }
    let value: TypeOrArray = Deserialize::deserialize(deserializer)?;
    let first = match value {
        TypeOrArray::Single(s) => Some(s),
        TypeOrArray::Array(a) => a.into_iter().next(),
    };
    Ok(first)
}

/// Type keyword deserializer for [`DenyUnknownFieldsJsonSchema`]: single string or array (takes first).
pub(crate) fn deserialize_type_optional_deny_unknown_fields<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TypeOrArray {
        Single(String),
        Array(Vec<String>),
    }
    let value: TypeOrArray = Deserialize::deserialize(deserializer)?;
    let first = match value {
        TypeOrArray::Single(s) => Some(s),
        TypeOrArray::Array(a) => a.into_iter().next(),
    };
    Ok(first)
}

/// Schema helper with `deny_unknown_fields`: same shape as our schema model but with `#[serde(deny_unknown_fields)]`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DenyUnknownFieldsJsonSchema {
    #[serde(default, rename = "$schema")]
    pub(crate) schema: Option<String>,
    #[serde(default, rename = "$id")]
    pub(crate) id: Option<String>,
    #[serde(default, rename = "$ref")]
    pub(crate) ref_: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_type_optional_deny_unknown_fields",
        rename = "type"
    )]
    pub(crate) type_: Option<String>,
    #[serde(default, rename = "$defs")]
    pub(crate) defs: Option<BTreeMap<String, DenyUnknownFieldsJsonSchema>>,
    #[serde(default)]
    pub(crate) definitions: Option<BTreeMap<String, DenyUnknownFieldsJsonSchema>>,
    #[serde(default)]
    pub(crate) properties: Option<BTreeMap<String, DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "additionalProperties")]
    pub(crate) additional_properties: Option<AdditionalProperties>,
    #[serde(default)]
    pub(crate) required: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) description: Option<String>,
    #[serde(default, rename = "$comment")]
    pub(crate) comment: Option<String>,
    #[serde(default, rename = "enum")]
    pub(crate) enum_values: Option<Vec<serde_json::Value>>,
    #[serde(default, rename = "const", deserialize_with = "deserialize_present")]
    pub(crate) const_value: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) items: Option<Box<DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "uniqueItems")]
    pub(crate) unique_items: Option<bool>,
    #[serde(default, rename = "minItems")]
    pub(crate) min_items: Option<u64>,
    #[serde(default, rename = "maxItems")]
    pub(crate) max_items: Option<u64>,
    #[serde(default)]
    pub(crate) minimum: Option<serde_json::Number>,
    #[serde(default)]
    pub(crate) maximum: Option<serde_json::Number>,
    #[serde(default, rename = "exclusiveMinimum")]
    pub(crate) exclusive_minimum: Option<serde_json::Number>,
    #[serde(default, rename = "exclusiveMaximum")]
    pub(crate) exclusive_maximum: Option<serde_json::Number>,
    #[serde(default, rename = "multipleOf")]
    pub(crate) multiple_of: Option<serde_json::Number>,
    #[serde(default, rename = "minLength")]
    pub(crate) min_length: Option<u64>,
    #[serde(default, rename = "maxLength")]
    pub(crate) max_length: Option<u64>,
    #[serde(default)]
    pub(crate) pattern: Option<String>,
    #[serde(default)]
    pub(crate) format: Option<String>,
    #[serde(default, rename = "default")]
    pub(crate) default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) deprecated: Option<bool>,
    #[serde(default)]
    pub(crate) examples: Option<Vec<serde_json::Value>>,
    #[serde(default, rename = "allOf")]
    pub(crate) all_of: Option<Vec<DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "anyOf")]
    pub(crate) any_of: Option<Vec<DenyUnknownFieldsJsonSchema>>,
    #[serde(default, rename = "oneOf")]
    pub(crate) one_of: Option<Vec<DenyUnknownFieldsJsonSchema>>,
}

/// Converts a strict (deny-unknown-fields) deserialized helper into the public [`JsonSchema`] model.
pub(crate) fn deny_unknown_fields_helper_to_schema(h: DenyUnknownFieldsJsonSchema) -> JsonSchema {
    let defs: Option<BTreeMap<String, JsonSchema>> = h.defs.map(|m| {
        m.into_iter()
            .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
            .collect()
    });
    let definitions: Option<BTreeMap<String, JsonSchema>> = h.definitions.map(|m| {
        m.into_iter()
            .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
            .collect()
    });
    let properties: BTreeMap<String, JsonSchema> = h
        .properties
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, deny_unknown_fields_helper_to_schema(v)))
        .collect();
    let items: Option<Box<JsonSchema>> = h
        .items
        .map(|b| Box::new(deny_unknown_fields_helper_to_schema(*b)));
    let all_of: Option<Vec<JsonSchema>> = h.all_of.map(|v| {
        v.into_iter()
            .map(deny_unknown_fields_helper_to_schema)
            .collect()
    });
    let any_of: Option<Vec<JsonSchema>> = h.any_of.map(|v| {
        v.into_iter()
            .map(deny_unknown_fields_helper_to_schema)
            .collect()
    });
    let one_of: Option<Vec<JsonSchema>> = h.one_of.map(|v| {
        v.into_iter()
            .map(deny_unknown_fields_helper_to_schema)
            .collect()
    });
    JsonSchema {
        schema: h.schema,
        id: h.id,
        ref_: h.ref_,
        type_: h.type_,
        defs,
        definitions,
        properties,
        additional_properties: h.additional_properties,
        required: h.required,
        title: h.title,
        description: h.description,
        comment: h.comment,
        enum_values: h.enum_values,
        const_value: h.const_value,
        items,
        unique_items: h.unique_items,
        min_items: h.min_items,
        max_items: h.max_items,
        minimum: h.minimum,
        maximum: h.maximum,
        exclusive_minimum: h.exclusive_minimum,
        exclusive_maximum: h.exclusive_maximum,
        multiple_of: h.multiple_of,
        min_length: h.min_length,
        max_length: h.max_length,
        pattern: h.pattern,
        format: h.format,
        default_value: h.default_value,
        deprecated: h.deprecated,
        examples: h.examples,
        all_of,
        any_of,
        one_of,
        dialect: None,
    }
}

/// Schema model used for code generation.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JsonSchema {
    /// Declares the JSON Schema dialect (meta-schema URI). When present, stored and round-tripped; used for draft inference when no explicit spec version is set.
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    /// Unique identifier for the schema (typically a URI). Stored and round-tripped as `$id`. In a draft-4 document, `id` is read into this field at parse time. Reserved for future use as base URI when `$ref` resolution is implemented.
    #[serde(rename = "$id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// Direct schema reference (applicator). Stored and round-tripped. Only fragment-only `#...` refs are resolved by this crate.
    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,

    /// Schema type; `object`, `string`, `integer`, and `number` drive codegen; others are ignored.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,

    /// Reusable subschemas (draft 2019-09+). Stored and round-tripped.
    #[serde(rename = "$defs", skip_serializing_if = "skip_defs_map")]
    pub defs: Option<BTreeMap<String, JsonSchema>>,

    /// Reusable subschemas (legacy drafts). Stored and round-tripped.
    #[serde(skip_serializing_if = "skip_defs_map")]
    pub definitions: Option<BTreeMap<String, JsonSchema>>,

    /// Object properties (only when type is "object"). Default empty; use `BTreeMap` for stable ordering.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, JsonSchema>,

    /// When set, controls whether properties not in `properties` are allowed (Allow/absent), forbidden (Forbid), or must validate against a schema (Schema). We do not implement `patternProperties` yet, so "additional" means any key not in `properties`.
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "skip_additional_properties"
    )]
    pub additional_properties: Option<AdditionalProperties>,

    /// Required property names at this object level. When absent, all properties are optional.
    #[serde(skip_serializing_if = "skip_required")]
    pub required: Option<Vec<String>>,

    /// Used for struct naming when present (`PascalCase`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Human-readable description. Codegen emits it as Rust doc comments (struct, enum, or field).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Schema-only comment (draft-07+). Stored and round-tripped; not used for validation or user-facing docs.
    #[serde(rename = "$comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    /// Allowed values for the instance (JSON Schema `enum`). When present and non-empty, instance must equal one of these. Codegen uses only string-only enums.
    #[serde(rename = "enum", skip_serializing_if = "skip_enum_values")]
    pub enum_values: Option<Vec<serde_json::Value>>,

    /// Single allowed value for the instance (JSON Schema `const`, draft-06+). When present, instance must equal this value (numbers by value: `1` equals `1.0`). `"const": null` is `Some(Value::Null)`. Codegen uses only string const (single-variant enum).
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<serde_json::Value>,

    /// Schema for all array elements (when type is "array"). Single-schema form only; tuple-typing (array of schemas) not supported.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<JsonSchema>>,

    /// When true, all array elements must be unique (JSON equality). Array-only; used by validator and codegen (`HashSet` when applicable).
    #[serde(rename = "uniqueItems", skip_serializing_if = "Option::is_none")]
    pub unique_items: Option<bool>,

    /// Minimum number of array elements. Array-only; used by validator and codegen (emitted as field attribute).
    #[serde(rename = "minItems", skip_serializing_if = "Option::is_none")]
    pub min_items: Option<u64>,

    /// Maximum number of array elements. Array-only; used by validator and codegen (emitted as field attribute).
    #[serde(rename = "maxItems", skip_serializing_if = "Option::is_none")]
    pub max_items: Option<u64>,

    /// Inclusive lower bound for numeric instances, held exactly as written (an integer stays an
    /// integer). Used for validation and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<serde_json::Number>,

    /// Inclusive upper bound for numeric instances, held exactly as written. Used for validation
    /// and for codegen type selection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<serde_json::Number>,

    /// Exclusive lower bound for numeric instances (draft 6+ form). A draft-4 document's boolean
    /// `exclusiveMinimum: true` is read at parse time as this bound with `minimum`'s value (see
    /// [`ExclusiveBounds`](super::ExclusiveBounds)). Used for validation and codegen type
    /// selection.
    #[serde(rename = "exclusiveMinimum", skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum: Option<serde_json::Number>,

    /// Exclusive upper bound for numeric instances; the counterpart of
    /// [`exclusive_minimum`](Self::exclusive_minimum).
    #[serde(rename = "exclusiveMaximum", skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum: Option<serde_json::Number>,

    /// A numeric instance must be an integer multiple of this value (decided exactly; see
    /// design.md, *multipleOf*). Used for validation.
    #[serde(rename = "multipleOf", skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<serde_json::Number>,

    /// Minimum string length in Unicode code points. String-only; used by validator and codegen.
    #[serde(rename = "minLength", skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u64>,

    /// Maximum string length in Unicode code points. String-only; used by validator and codegen.
    #[serde(rename = "maxLength", skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u64>,

    /// ECMA 262 regex pattern; string instance is valid if the pattern matches anywhere in the string. String-only; used by validator and codegen.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// Format annotation for string instances (e.g. "uuid"). When the `uuid` feature is enabled,
    /// `"uuid"` drives type selection in codegen and format validation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Default value (meta-data/annotation). Does not affect validation. Codegen uses it for `#[serde(default)]` or default functions.
    #[serde(rename = "default", skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,

    /// Deprecation flag (meta-data/annotation, draft 2019-09+). When true, indicates the instance or property should not be used. Does not affect validation. Codegen emits `#[deprecated]` on the corresponding Rust field or type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,

    /// Example values (meta-data/annotation, draft-06+). Does not affect validation. Stored and round-tripped; not emitted as Rust.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<serde_json::Value>>,

    /// allOf: instance must validate against every subschema. Stored as-is at ingestion; validator validates each; codegen merges on-the-fly. Not preserved on round-trip or reverse codegen.
    #[serde(rename = "allOf", skip_serializing_if = "skip_all_of")]
    pub all_of: Option<Vec<JsonSchema>>,

    /// anyOf: instance must validate against at least one subschema. Stored as-is at ingestion; validator and codegen (union enum) use it. Not emitted by reverse codegen.
    #[serde(rename = "anyOf", skip_serializing_if = "skip_any_of")]
    pub any_of: Option<Vec<JsonSchema>>,

    /// oneOf: instance must validate against exactly one subschema. Stored as-is at ingestion; validator and codegen (union enum) use it. Not emitted by reverse codegen.
    #[serde(rename = "oneOf", skip_serializing_if = "skip_one_of")]
    pub one_of: Option<Vec<JsonSchema>>,

    /// The effective dialect when the parsing settings chose one other than what `$schema` alone
    /// selects (a pinned spec version or a per-flag override). `None` follows `$schema` (Draft
    /// 2020-12 when absent). Read the effective dialect with [`JsonSchema::dialect`]. Not
    /// serialized.
    #[serde(skip)]
    pub dialect: Option<Dialect>,
}

impl<'de> Deserialize<'de> for JsonSchema {
    #[expect(clippy::too_many_lines)]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct JsonSchemaHelper {
            #[serde(default, rename = "$schema")]
            schema: Option<String>,
            #[serde(default, rename = "$id")]
            id: Option<String>,
            #[serde(default, rename = "$ref")]
            ref_: Option<String>,
            #[serde(default, deserialize_with = "deserialize_type_optional")]
            #[serde(rename = "type")]
            type_: Option<String>,
            #[serde(default, rename = "$defs")]
            defs: Option<BTreeMap<String, JsonSchema>>,
            #[serde(default)]
            definitions: Option<BTreeMap<String, JsonSchema>>,
            #[serde(default)]
            properties: Option<BTreeMap<String, JsonSchema>>,
            #[serde(default, rename = "additionalProperties")]
            additional_properties: Option<AdditionalProperties>,
            #[serde(default)]
            required: Option<Vec<String>>,
            #[serde(default)]
            title: Option<String>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default, rename = "$comment")]
            comment: Option<String>,
            #[serde(default, rename = "enum")]
            enum_values: Option<Vec<serde_json::Value>>,
            #[serde(default, rename = "const", deserialize_with = "deserialize_present")]
            const_value: Option<serde_json::Value>,
            #[serde(default)]
            items: Option<Box<JsonSchema>>,
            #[serde(default, rename = "uniqueItems")]
            unique_items: Option<bool>,
            #[serde(default, rename = "minItems")]
            min_items: Option<u64>,
            #[serde(default, rename = "maxItems")]
            max_items: Option<u64>,
            #[serde(default)]
            minimum: Option<serde_json::Number>,
            #[serde(default)]
            maximum: Option<serde_json::Number>,
            #[serde(default, rename = "exclusiveMinimum")]
            exclusive_minimum: Option<serde_json::Number>,
            #[serde(default, rename = "exclusiveMaximum")]
            exclusive_maximum: Option<serde_json::Number>,
            #[serde(default, rename = "multipleOf")]
            multiple_of: Option<serde_json::Number>,
            #[serde(default, rename = "minLength")]
            min_length: Option<u64>,
            #[serde(default, rename = "maxLength")]
            max_length: Option<u64>,
            #[serde(default)]
            pattern: Option<String>,
            #[serde(default)]
            format: Option<String>,
            #[serde(default, rename = "default")]
            default_value: Option<serde_json::Value>,
            #[serde(default)]
            deprecated: Option<bool>,
            #[serde(default)]
            examples: Option<Vec<serde_json::Value>>,
            #[serde(default, rename = "allOf")]
            all_of: Option<Vec<JsonSchema>>,
            #[serde(default, rename = "anyOf")]
            any_of: Option<Vec<JsonSchema>>,
            #[serde(default, rename = "oneOf")]
            one_of: Option<Vec<JsonSchema>>,
        }
        let h: JsonSchemaHelper = JsonSchemaHelper::deserialize(deserializer)?;
        Ok(JsonSchema {
            schema: h.schema,
            id: h.id,
            ref_: h.ref_,
            type_: h.type_,
            defs: h.defs,
            definitions: h.definitions,
            properties: h.properties.unwrap_or_default(),
            additional_properties: h.additional_properties,
            required: h.required,
            title: h.title,
            description: h.description,
            comment: h.comment,
            enum_values: h.enum_values,
            const_value: h.const_value,
            items: h.items,
            unique_items: h.unique_items,
            min_items: h.min_items,
            max_items: h.max_items,
            minimum: h.minimum,
            maximum: h.maximum,
            exclusive_minimum: h.exclusive_minimum,
            exclusive_maximum: h.exclusive_maximum,
            multiple_of: h.multiple_of,
            min_length: h.min_length,
            max_length: h.max_length,
            pattern: h.pattern,
            format: h.format,
            default_value: h.default_value,
            deprecated: h.deprecated,
            examples: h.examples,
            all_of: h.all_of,
            any_of: h.any_of,
            one_of: h.one_of,
            dialect: None,
        })
    }
}

/// Raw schema text, parsed with the matching `serde_json` entry point.
#[derive(Clone, Copy)]
enum RawSchema<'a> {
    Str(&'a str),
    Slice(&'a [u8]),
}

impl RawSchema<'_> {
    fn parse<T: DeserializeOwned>(&self) -> serde_json::Result<T> {
        match self {
            RawSchema::Str(s) => serde_json::from_str(s),
            RawSchema::Slice(b) => serde_json::from_slice(b),
        }
    }
}

/// Parses raw schema text. Lenient parsing goes straight to the model unless the document
/// needs a draft-4 shape normalised (an `id` key, or a boolean `exclusiveMinimum` /
/// `exclusiveMaximum`, which the model cannot hold); strict parsing, and those cases, go through
/// a [`Value`] so the dialect is known before the model is built.
fn parse_raw(
    raw: RawSchema<'_>,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<JsonSchema> {
    if !settings.disallow_unknown_fields {
        match raw.parse::<JsonSchema>() {
            Ok(schema) => {
                let resolved: ResolvedDialect = resolve_dialect(&schema, settings)?;
                if !uses_legacy_id(resolved.dialect) || !may_contain_key(raw, "\"id\"") {
                    return Ok(finish_parse(schema, &resolved));
                }
            }
            Err(error) => {
                // A draft-4 boolean bound fails the direct parse; retry through a `Value` only
                // when that can be the cause, and otherwise keep the direct error (it carries
                // the line and column).
                if !may_contain_key(raw, "\"exclusiveM") {
                    return Err(error.into());
                }
                let value: Value = raw.parse()?;
                let resolved: ResolvedDialect = resolve_dialect_for_value(&value, settings)?;
                if resolved.dialect.exclusive_bounds != ExclusiveBounds::BooleanModifier {
                    return Err(error.into());
                }
                return parse_value_in_dialect(value, &resolved, settings);
            }
        }
    }
    let value: Value = raw.parse()?;
    parse_value(value, settings)
}

/// Parses a schema document held as a [`Value`].
fn parse_value(value: Value, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<JsonSchema> {
    let resolved: ResolvedDialect = resolve_dialect_for_value(&value, settings)?;
    parse_value_in_dialect(value, &resolved, settings)
}

/// Resolves the dialect of a document held as a [`Value`] from its `$schema` and `settings`.
fn resolve_dialect_for_value(
    value: &Value,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<ResolvedDialect> {
    let schema_uri: Option<&str> = value.get("$schema").and_then(Value::as_str);
    resolve_dialect_for_uri(schema_uri, settings)
}

/// Normalises the document's draft-4 shapes for `resolved`'s dialect and builds the model.
fn parse_value_in_dialect(
    mut value: Value,
    resolved: &ResolvedDialect,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<JsonSchema> {
    normalize_draft4_shapes(&mut value, resolved.dialect);
    let schema: JsonSchema = if settings.disallow_unknown_fields {
        let helper: DenyUnknownFieldsJsonSchema = serde_json::from_value(value)?;
        deny_unknown_fields_helper_to_schema(helper)
    } else {
        serde_json::from_value(value)?
    };
    Ok(finish_parse(schema, resolved))
}

/// Logs the dialect warnings and records the dialect on the root when the settings changed it.
fn finish_parse(mut schema: JsonSchema, resolved: &ResolvedDialect) -> JsonSchema {
    for warning in &resolved.warnings {
        log::debug!("json-schema-rs: {warning}");
    }
    if resolved.dialect != schema.dialect() {
        schema.dialect = Some(resolved.dialect);
    }
    schema
}

/// Whether the dialect names a schema's URI with draft 4's `id` rather than `$id`.
fn uses_legacy_id(dialect: Dialect) -> bool {
    dialect.id_keyword() == "id"
}

/// Whether raw JSON text can contain `needle` (a quoted key or key prefix): it is either written
/// literally or the text has a `\u` escape that could spell it. Uses `str::contains`, which is
/// several times faster than a byte-window scan on large documents; text that is not UTF-8
/// answers `true` and is left to the parser to reject.
fn may_contain_key(raw: RawSchema<'_>, needle: &str) -> bool {
    let text: Result<&str, std::str::Utf8Error> = match raw {
        RawSchema::Str(s) => Ok(s),
        RawSchema::Slice(b) => std::str::from_utf8(b),
    };
    text.map_or(true, |s| s.contains(needle) || s.contains("\\u"))
}

/// Rewrites draft-4 shapes in every schema object of `root` into the one shape the model holds,
/// as `dialect` says:
///
/// - when the id keyword is `id`, a string `id` becomes `$id` (an object that already has `$id`
///   keeps both keys as written);
/// - under [`ExclusiveBounds::BooleanModifier`], `exclusiveMinimum: true` next to `minimum`
///   becomes `exclusiveMinimum: <minimum>` with `minimum` removed, and `exclusiveMinimum: false`
///   (or `true` without `minimum`, which has no effect) is removed; likewise for
///   `exclusiveMaximum` / `maximum`. A numeric `exclusiveMinimum` is kept as the bound it states.
///
/// Only schema positions are visited (from the dialect's keyword table), so a property named
/// `id` is left alone. Iterative.
fn normalize_draft4_shapes(root: &mut Value, dialect: Dialect) {
    let legacy_id: bool = uses_legacy_id(dialect);
    let boolean_bounds: bool = dialect.exclusive_bounds == ExclusiveBounds::BooleanModifier;
    if !legacy_id && !boolean_bounds {
        return;
    }
    let mut stack: Vec<&mut Value> = vec![root];
    while let Some(node) = stack.pop() {
        let Value::Object(map) = node else {
            continue;
        };
        if legacy_id
            && !map.contains_key("$id")
            && map.get("id").is_some_and(Value::is_string)
            && let Some(id) = map.remove("id")
        {
            map.insert("$id".to_string(), id);
        }
        if boolean_bounds {
            for (modifier, bound) in [
                ("exclusiveMinimum", "minimum"),
                ("exclusiveMaximum", "maximum"),
            ] {
                let Some(&Value::Bool(exclusive)) = map.get(modifier) else {
                    continue;
                };
                map.remove(modifier);
                if exclusive && let Some(value) = map.remove(bound) {
                    map.insert(modifier.to_string(), value);
                }
            }
        }
        for (key, child) in map.iter_mut() {
            let Some(position) = dialect.keyword(key).and_then(|k| k.subschemas) else {
                continue;
            };
            match (position, child) {
                (
                    SubschemaPosition::Array | SubschemaPosition::SingleOrArray,
                    Value::Array(items),
                ) => {
                    stack.extend(items.iter_mut());
                }
                (SubschemaPosition::Map, Value::Object(entries)) => {
                    stack.extend(entries.values_mut());
                }
                (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, child) => {
                    stack.push(child);
                }
                (SubschemaPosition::Array | SubschemaPosition::Map, _) => {}
            }
        }
    }
}

impl JsonSchema {
    /// The effective [`Dialect`] of this schema as a document root: the one recorded at parse
    /// time when the settings chose it, otherwise the one its `$schema` selects (Draft 2020-12
    /// when absent or unrecognised).
    #[must_use]
    pub fn dialect(&self) -> Dialect {
        self.dialect.unwrap_or_else(|| {
            resolve_dialect_for_uri(self.schema.as_deref(), &JsonSchemaSettings::default())
                .map(|resolved| resolved.dialect)
                .unwrap_or_default()
        })
    }

    /// Returns true if this schema is an object with properties (for codegen).
    #[must_use]
    pub(crate) fn is_object_with_properties(&self) -> bool {
        self.type_.as_deref() == Some("object") && !self.properties.is_empty()
    }

    /// Returns true if this schema is type "string".
    #[must_use]
    pub(crate) fn is_string(&self) -> bool {
        self.type_.as_deref() == Some("string")
    }

    /// Returns true if this schema is type "integer".
    #[must_use]
    pub(crate) fn is_integer(&self) -> bool {
        self.type_.as_deref() == Some("integer")
    }

    /// Returns true if this schema is type "number".
    #[must_use]
    pub(crate) fn is_number(&self) -> bool {
        self.type_.as_deref() == Some("number")
    }

    /// Returns true if this schema is type "boolean".
    #[must_use]
    pub(crate) fn is_boolean(&self) -> bool {
        self.type_.as_deref() == Some("boolean")
    }

    /// Returns true if this schema is type "array".
    #[must_use]
    pub(crate) fn is_array(&self) -> bool {
        self.type_.as_deref() == Some("array")
    }

    /// Returns true if this schema is type "array" and has an items schema (single-schema form).
    #[must_use]
    pub(crate) fn is_array_with_items(&self) -> bool {
        self.is_array() && self.items.is_some()
    }

    /// Returns true if the given property name is required at this object level.
    #[must_use]
    pub(crate) fn is_required(&self, name: &str) -> bool {
        self.required
            .as_ref()
            .is_some_and(|r| r.iter().any(|s| s == name))
    }

    /// Returns true if this schema has a string-only enum (non-empty, all elements are strings). Used for codegen to emit a Rust enum.
    #[must_use]
    pub(crate) fn is_string_enum(&self) -> bool {
        self.enum_values
            .as_ref()
            .is_some_and(|v| !v.is_empty() && v.iter().all(serde_json::Value::is_string))
    }

    /// Returns true when this schema has a string const (single allowed string value). Used for codegen to treat as single-value enum.
    #[must_use]
    pub(crate) fn is_string_const(&self) -> bool {
        self.const_value
            .as_ref()
            .is_some_and(serde_json::Value::is_string)
    }

    /// Returns mutable references to this schema's direct subschemas, in a fixed order:
    /// `$defs`, `definitions`, `properties`, `additionalProperties`, `items`, `allOf`, `anyOf`,
    /// `oneOf`. Walk the whole tree with an explicit stack over this.
    pub(crate) fn subschemas_mut(&mut self) -> Vec<&mut JsonSchema> {
        let mut children: Vec<&mut JsonSchema> = Vec::new();
        if let Some(defs) = self.defs.as_mut() {
            children.extend(defs.values_mut());
        }
        if let Some(definitions) = self.definitions.as_mut() {
            children.extend(definitions.values_mut());
        }
        children.extend(self.properties.values_mut());
        if let Some(AdditionalProperties::Schema(schema)) = self.additional_properties.as_mut() {
            children.push(schema.as_mut());
        }
        if let Some(items) = self.items.as_mut() {
            children.push(items.as_mut());
        }
        for list in [
            self.all_of.as_mut(),
            self.any_of.as_mut(),
            self.one_of.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            children.extend(list.iter_mut());
        }
        children
    }

    /// Parse a JSON Schema from a string with the given settings.
    ///
    /// When [`JsonSchemaSettings::disallow_unknown_fields`] is `false`, unknown keys
    /// are ignored (lenient). When `true`, any unknown key causes an error (strict).
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Serde`] on invalid JSON or type mismatch.
    /// Returns [`JsonSchemaParseError::UnknownField`] when strict and an unknown key is present.
    pub fn new_from_str(json: &str, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<Self> {
        parse_raw(RawSchema::Str(json), settings)
    }

    /// Parse a JSON Schema from a byte slice with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but takes bytes (e.g. from a file).
    ///
    /// # Errors
    ///
    /// Same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_slice(
        slice: &[u8],
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        parse_raw(RawSchema::Slice(slice), settings)
    }

    /// Parse a JSON Schema from an already-parsed [`serde_json::Value`] with the given settings.
    ///
    /// Same semantics as [`new_from_str`](Self::new_from_str); takes a value to avoid string round-trips
    /// when the schema is already loaded as JSON (e.g. from a test case or API response).
    ///
    /// # Errors
    ///
    /// Same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_serde_value(
        value: &serde_json::Value,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        parse_value(value.clone(), settings)
    }

    /// Parse a JSON Schema from a reader with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but reads from a reader. I/O errors are
    /// returned as [`JsonSchemaParseError::Io`].
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on read failure.
    /// Otherwise same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_reader<R: Read>(
        reader: R,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let mut buf: Vec<u8> = Vec::new();
        let mut reader = reader;
        reader
            .read_to_end(&mut buf)
            .map_err(JsonSchemaParseError::Io)?;
        Self::new_from_slice(&buf, settings)
    }

    /// Parse a JSON Schema from a file path with the given settings.
    ///
    /// Same as [`new_from_str`](Self::new_from_str) but reads from a file. I/O errors (e.g. file not found)
    /// are returned as [`JsonSchemaParseError::Io`].
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on open or read failure.
    /// Otherwise same as [`new_from_str`](Self::new_from_str).
    pub fn new_from_path<P: AsRef<std::path::Path>>(
        path: P,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let f: std::fs::File = std::fs::File::open(path.as_ref())?;
        Self::new_from_reader(f, settings)
    }
}

// TryFrom: parse into JsonSchema with default settings

/// Parses a JSON Schema from a string using default [`JsonSchemaSettings`](super::settings::JsonSchemaSettings).
/// For custom settings, use [`JsonSchema::new_from_str`](Self::new_from_str).
impl TryFrom<&str> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value, &settings)
    }
}

/// Parses a JSON Schema from an owned string using default settings.
/// For custom settings, use [`JsonSchema::new_from_str`](Self::new_from_str).
impl TryFrom<String> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value.as_str(), &settings)
    }
}

/// Parses a JSON Schema from a byte slice using default settings.
/// For custom settings, use [`JsonSchema::new_from_slice`](Self::new_from_slice).
impl TryFrom<&[u8]> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_slice(value, &settings)
    }
}

/// Parses a JSON Schema from an already-parsed [`serde_json::Value`] using default settings.
/// For custom settings, use [`JsonSchema::new_from_serde_value`](Self::new_from_serde_value).
impl TryFrom<&serde_json::Value> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_serde_value(value, &settings)
    }
}

/// Parses a JSON Schema from an open file using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_reader`](Self::new_from_reader).
impl TryFrom<std::fs::File> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: std::fs::File) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_reader(value, &settings)
    }
}

/// Parses a JSON Schema from a file path using default settings. I/O errors (e.g. file not found) are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<&std::path::Path> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(value, &settings)
    }
}

/// Parses a JSON Schema from an owned path using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<std::path::PathBuf> for JsonSchema {
    type Error = JsonSchemaParseError;

    fn try_from(value: std::path::PathBuf) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(&value, &settings)
    }
}

impl TryFrom<&JsonSchema> for String {
    type Error = JsonSchemaParseError;

    fn try_from(schema: &JsonSchema) -> Result<String, Self::Error> {
        serde_json::to_string(schema).map_err(Into::into)
    }
}

impl TryFrom<&JsonSchema> for Vec<u8> {
    type Error = JsonSchemaParseError;

    fn try_from(schema: &JsonSchema) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(schema).map_err(Into::into)
    }
}

impl TryFrom<JsonSchema> for String {
    type Error = JsonSchemaParseError;

    fn try_from(schema: JsonSchema) -> Result<String, Self::Error> {
        serde_json::to_string(&schema).map_err(Into::into)
    }
}

impl TryFrom<JsonSchema> for Vec<u8> {
    type Error = JsonSchemaParseError;

    fn try_from(schema: JsonSchema) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(&schema).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::JsonSchema;
    use crate::json_schema::{
        Dialect, ExclusiveBounds, FormatMode, JsonSchemaParseError, JsonSchemaSettings,
        SpecVersion, UnknownSchemaUri,
    };
    use std::collections::BTreeMap;
    use std::io;

    #[test]
    fn try_from_schema_to_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            title: Some("Root".to_string()),
            ..Default::default()
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"object","title":"Root"}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_with_comment_serializes_dollar_comment() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            comment: Some("Created by John Doe".to_string()),
            ..Default::default()
        };
        let actual: String = (&schema).try_into().expect("serialize");
        let expected_contains = r#""$comment":"Created by John Doe""#;
        assert!(
            actual.contains(expected_contains),
            "serialized schema should contain $comment; got: {actual}"
        );
    }

    #[test]
    fn try_from_schema_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"string\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn round_trip_via_vec_u8() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let bytes: Vec<u8> = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(bytes.as_slice()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    // examples keyword (meta-data, draft-06+)

    #[test]
    fn parse_examples_present() {
        let json = r#"{"type":"string","examples":[1,"a",{"b":true}]}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            examples: Some(vec![
                serde_json::json!(1),
                serde_json::json!("a"),
                serde_json::json!({"b": true}),
            ]),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_examples_absent() {
        let json = r#"{"type":"string"}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_examples_empty_array() {
        let json = r#"{"type":"string","examples":[]}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            examples: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_examples_unchanged() {
        let json = r#"{"type":"object","examples":[1,2,3]}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        let expected: JsonSchema = parsed;
        let actual: JsonSchema = reparsed;
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_parse_schema_with_examples_succeeds() {
        let json = r#"{"type":"object","properties":{},"examples":[1]}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: JsonSchema =
            JsonSchema::new_from_str(json, &settings).expect("parse with strict");
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            examples: Some(vec![serde_json::json!(1)]),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    // deprecated keyword (meta-data, draft 2019-09+)

    #[test]
    fn parse_deprecated_true() {
        let json = r#"{"type":"string","deprecated":true}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            deprecated: Some(true),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_deprecated_false() {
        let json = r#"{"type":"string","deprecated":false}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            deprecated: Some(false),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_deprecated_absent() {
        let json = r#"{"type":"string"}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_deprecated_unchanged() {
        let json = r#"{"type":"object","properties":{"old":{"type":"string","deprecated":true}}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        let expected: JsonSchema = parsed;
        let actual: JsonSchema = reparsed;
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_parse_schema_with_deprecated_succeeds() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string","deprecated":true}}}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: JsonSchema =
            JsonSchema::new_from_str(json, &settings).expect("parse with strict");
        let mut properties: BTreeMap<String, JsonSchema> = BTreeMap::new();
        properties.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                deprecated: Some(true),
                ..Default::default()
            },
        );
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties,
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    // TryFrom: parse into JsonSchema with default settings

    #[test]
    fn try_from_str_success() {
        let json: &str = r#"{"type":"object","properties":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_str_invalid_json() {
        let json: &str = "not json";
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(json);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn try_from_string_success() {
        let json: String = r#"{"type":"string"}"#.to_string();
        let actual: JsonSchema = JsonSchema::try_from(json).expect("try_from");
        let expected_type: Option<&str> = Some("string");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_slice_success() {
        let bytes: &[u8] = r#"{"type":"array","items":{"type":"string"}}"#.as_bytes();
        let actual: JsonSchema = JsonSchema::try_from(bytes).expect("try_from");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_serde_value_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "integer"}}});
        let actual: JsonSchema = JsonSchema::try_from(&value).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(&value);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_file_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"object","properties":{}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let f: std::fs::File = std::fs::File::open(&schema_path).expect("open");
        let actual: JsonSchema = JsonSchema::try_from(f).expect("try_from");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_file_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let f: std::fs::File = std::fs::File::open(&schema_path).expect("open");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(f);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn try_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("try_from");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn try_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path: std::path::PathBuf = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn try_from_path_buf_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"number"}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path).expect("try_from");
        let expected_type: Option<&str> = Some("number");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn is_object_with_properties() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_object_with_properties(),
            {
                s.type_ = Some("object".to_string());
                s.is_object_with_properties()
            },
            {
                s.properties.insert("x".to_string(), JsonSchema::default());
                s.is_object_with_properties()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string() {
        let mut s = JsonSchema::default();
        let actual = [s.is_string(), {
            s.type_ = Some("string".to_string());
            s.is_string()
        }];
        let expected = [false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_integer(),
            {
                s.type_ = Some("string".to_string());
                s.is_integer()
            },
            {
                s.type_ = Some("integer".to_string());
                s.is_integer()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_integer_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            ..Default::default()
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"integer\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn is_number() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_number(),
            {
                s.type_ = Some("string".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("integer".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("object".to_string());
                s.is_number()
            },
            {
                s.type_ = Some("number".to_string());
                s.is_number()
            },
        ];
        let expected = [false, false, false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_number_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            ..Default::default()
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"number\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn is_string_enum() {
        let no_enum: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        assert!(!no_enum.is_string_enum());
        let empty_enum: JsonSchema = JsonSchema {
            enum_values: Some(vec![]),
            ..Default::default()
        };
        assert!(!empty_enum.is_string_enum());
        let string_enum: JsonSchema = JsonSchema {
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            ..Default::default()
        };
        assert!(string_enum.is_string_enum());
        let mixed_enum: JsonSchema = JsonSchema {
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::Number(42_i64.into()),
            ]),
            ..Default::default()
        };
        assert!(!mixed_enum.is_string_enum());
    }

    #[test]
    fn is_array() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_array(),
            {
                s.type_ = Some("string".to_string());
                s.is_array()
            },
            {
                s.type_ = Some("array".to_string());
                s.is_array()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_array_with_items() {
        let mut s = JsonSchema::default();
        let actual = [
            s.is_array_with_items(),
            {
                s.type_ = Some("array".to_string());
                s.is_array_with_items()
            },
            {
                s.items = Some(Box::new(JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                }));
                s.is_array_with_items()
            },
        ];
        let expected = [false, false, true];
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_schema_array_with_items_to_string() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(item_schema)),
            ..Default::default()
        };
        let actual: String = schema.try_into().expect("serialize");
        let expected = r#"{"type":"array","items":{"type":"string"}}"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_unique_items_true() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = Some(true);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_false() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":false}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = Some(false);
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unique_items_absent() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<bool> = None;
        let actual: Option<bool> = parsed.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_compare_with_unique_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_min_length() {
        let json = r#"{"type":"string","minLength":5}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(5), parsed.min_length);
    }

    #[test]
    fn parse_max_length() {
        let json = r#"{"type":"string","maxLength":20}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(20), parsed.max_length);
    }

    #[test]
    fn parse_min_length_max_length_both() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(Some(2), parsed.min_length);
        assert_eq!(Some(50), parsed.max_length);
    }

    #[test]
    fn parse_min_length_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.min_length);
    }

    #[test]
    fn parse_max_length_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.max_length);
    }

    #[test]
    fn parse_pattern() {
        let json = r#"{"type":"string","pattern":"^\\d+$"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some(r"^\d+$".to_string());
        let actual: Option<String> = parsed.pattern.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_pattern_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        let actual: Option<String> = parsed.pattern.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_with_pattern() {
        let json = r#"{"type":"string","pattern":"^[a-z]+$"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        let expected: JsonSchema = parsed;
        let actual: JsonSchema = reparsed;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_parse_with_min_length_max_length() {
        let json = r#"{"type":"string","minLength":2,"maxLength":50}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn parse_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("uuid".to_string());
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_format_absent() {
        let json = r#"{"type":"string"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        let actual: Option<String> = parsed.format;
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_parse_serialize_format_uuid() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed, reparsed);
    }

    // --- tests merged from parser ---

    #[test]
    fn deserialize_simple_object_schema() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_present() {
        let json = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props_a = BTreeMap::new();
        props_a.insert(
            "a".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let mut props_b = BTreeMap::new();
        props_b.insert(
            "b".to_string(),
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![
            JsonSchema {
                type_: Some("object".to_string()),
                properties: props_a,
                ..Default::default()
            },
            JsonSchema {
                type_: Some("object".to_string()),
                properties: props_b,
                ..Default::default()
            },
        ]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_empty_array() {
        let json = r#"{"allOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_single_subschema() {
        let json = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props = BTreeMap::new();
        props.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![JsonSchema {
            type_: Some("object".to_string()),
            properties: props,
            ..Default::default()
        }]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_present() {
        let json = r#"{"anyOf":[{"type":"string"},{"type":"integer"}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        ]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_empty_array() {
        let json = r#"{"anyOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_single_subschema() {
        let json = r#"{"anyOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props = BTreeMap::new();
        props.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![JsonSchema {
            type_: Some("object".to_string()),
            properties: props,
            ..Default::default()
        }]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_present() {
        let json = r#"{"oneOf":[{"type":"string"},{"type":"integer"}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        ]);
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = None;
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_empty_array() {
        let json = r#"{"oneOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<JsonSchema>> = Some(vec![]);
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_single_subschema() {
        let json = r#"{"oneOf":[{"type":"object","properties":{"x":{"type":"string"}}}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let mut props = BTreeMap::new();
        props.insert(
            "x".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: Option<Vec<JsonSchema>> = Some(vec![JsonSchema {
            type_: Some("object".to_string()),
            properties: props,
            ..Default::default()
        }]);
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_with_required() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x"]}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "x".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m.insert(
                    "y".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: Some(vec!["x".to_string()]),
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_ignores_unknown_keys() {
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let expected: JsonSchema = JsonSchema {
            schema: Some("https://example.com".to_string()),
            type_: Some("object".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_type_array_takes_first() {
        let json = r#"{"type":["string", "null"],"properties":{}}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items present").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
    }

    #[test]
    fn parse_lenient_accepts_unknown_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_rejects_unknown_key_at_root() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{},"unknown":42}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown") || msg.contains("Unknown"),
            "error message should mention unknown field: {msg}"
        );
    }

    #[test]
    fn parse_strict_rejects_unknown_key_in_nested_properties() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"nested":{"type":"object","properties":{},"bad":1}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_err());
    }

    #[test]
    fn parse_strict_accepts_only_known_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"title":"Root"}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_with_schema_uri_preserves_schema() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> =
            Some("https://json-schema.org/draft/2020-12/schema".to_string());
        assert_eq!(expected, parsed.schema);
    }

    #[test]
    fn parse_without_schema_uri_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.schema);
    }

    #[test]
    fn parse_without_id_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn parse_with_id_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("http://example.com/schema".to_string());
        assert_eq!(expected, parsed.id);
    }

    #[test]
    fn round_trip_preserves_id() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse");
        assert_eq!(parsed.id, reparsed.id);
    }

    #[test]
    fn parse_strict_accepts_id_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("http://example.com/schema".to_string()), parsed.id);
    }

    #[test]
    fn parse_strict_accepts_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{}}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(
            Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            parsed.schema
        );
    }

    #[test]
    fn round_trip_preserves_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        assert!(
            serialized.contains("\"$schema\""),
            "serialized output should contain $schema key: {serialized}"
        );
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed.schema, reparsed.schema);
    }

    #[test]
    fn parsed_dialect_follows_schema_uri() {
        let json = r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object"}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft07);
        let actual: Dialect = schema.dialect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parsed_dialect_defaults_to_2020_12_when_schema_absent() {
        let json = r#"{"type":"object","properties":{}}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
        let actual: Dialect = schema.dialect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parsed_dialect_defaults_to_2020_12_when_schema_unknown_uri() {
        let json =
            r#"{"$schema":"https://unknown.example/schema","type":"object","properties":{}}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
        let actual: Dialect = schema.dialect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_without_settings_overrides_records_no_dialect() {
        let json = r#"{"$schema":"http://json-schema.org/draft-07/schema#"}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Dialect> = None;
        let actual: Option<Dialect> = schema.dialect;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_with_pinned_spec_version_records_dialect() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: Option<Dialect> = Some(Dialect::for_spec_version(SpecVersion::Draft07));
        let actual: Option<Dialect> = schema.dialect;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_with_pinned_spec_version_matching_schema_uri_records_no_dialect() {
        let json = r#"{"$schema":"http://json-schema.org/draft-07/schema#"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: Option<Dialect> = None;
        let actual: Option<Dialect> = schema.dialect;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_with_format_mode_override_records_it() {
        let json = r#"{"type":"string","format":"uuid"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .format_mode(FormatMode::Assertion)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: FormatMode = FormatMode::Assertion;
        let actual: FormatMode = schema.dialect().format_mode;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unknown_schema_uri_rejected_when_strict() {
        let json = r#"{"$schema":"https://example.com/custom-meta","type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_str(json, &settings);
        assert!(matches!(
            actual,
            Err(JsonSchemaParseError::UnknownSchemaUri { ref uri }) if uri == "https://example.com/custom-meta"
        ));
    }

    #[test]
    fn parse_unknown_schema_uri_rejected_when_strict_and_unknown_fields_disallowed() {
        let json = r#"{"$schema":"https://example.com/custom-meta","type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_str(json, &settings);
        assert!(matches!(
            actual,
            Err(JsonSchemaParseError::UnknownSchemaUri { ref uri }) if uri == "https://example.com/custom-meta"
        ));
    }

    // --- Draft-4 `id` is normalised to `$id` ---

    const DRAFT4_WITH_IDS: &str = r##"{
        "$schema": "http://json-schema.org/draft-04/schema#",
        "id": "http://example.com/root.json",
        "type": "object",
        "properties": {"id": {"type": "string", "id": "#name"}},
        "definitions": {"a": {"id": "#a", "type": "integer"}}
    }"##;

    /// Draft-4 document with schema positions the model does not hold (`items` array form,
    /// `dependencies`), for checking the normalisation walk on a raw [`serde_json::Value`].
    const DRAFT4_RAW: &str = r##"{
        "id": "http://example.com/root.json",
        "properties": {"id": {"type": "string", "id": "#name"}},
        "items": [{"id": "#first"}],
        "dependencies": {"b": ["c"], "d": {"id": "#d"}}
    }"##;

    fn ids(schema: &JsonSchema) -> Vec<Option<String>> {
        let definition: Option<String> = schema
            .definitions
            .as_ref()
            .and_then(|d| d.get("a"))
            .and_then(|a| a.id.clone());
        let property: Option<String> = schema.properties.get("id").and_then(|p| p.id.clone());
        vec![schema.id.clone(), property, definition]
    }

    fn draft4_expected_ids() -> Vec<Option<String>> {
        vec![
            Some("http://example.com/root.json".to_string()),
            Some("#name".to_string()),
            Some("#a".to_string()),
        ]
    }

    #[test]
    fn draft4_id_normalized_from_str() {
        let expected: Vec<Option<String>> = draft4_expected_ids();
        let schema: JsonSchema = JsonSchema::try_from(DRAFT4_WITH_IDS).expect("parse");
        let actual: Vec<Option<String>> = ids(&schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalized_from_slice() {
        let expected: Vec<Option<String>> = draft4_expected_ids();
        let schema: JsonSchema = JsonSchema::try_from(DRAFT4_WITH_IDS.as_bytes()).expect("parse");
        let actual: Vec<Option<String>> = ids(&schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalized_from_serde_value() {
        let expected: Vec<Option<String>> = draft4_expected_ids();
        let value: serde_json::Value = serde_json::from_str(DRAFT4_WITH_IDS).expect("json");
        let schema: JsonSchema = JsonSchema::try_from(&value).expect("parse");
        let actual: Vec<Option<String>> = ids(&schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalized_when_strict() {
        let json = r#"{"$schema":"http://json-schema.org/draft-04/schema#","id":"http://example.com/s.json","type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: Option<String> = Some("http://example.com/s.json".to_string());
        let actual: Option<String> = schema.id;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalized_inside_items_array() {
        let mut raw: serde_json::Value = serde_json::from_str(DRAFT4_RAW).expect("json");
        super::normalize_draft4_shapes(&mut raw, Dialect::for_spec_version(SpecVersion::Draft04));
        let expected: serde_json::Value = serde_json::json!([{"$id": "#first"}]);
        let actual: serde_json::Value = raw["items"].clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalization_leaves_property_named_id() {
        let mut raw: serde_json::Value = serde_json::from_str(DRAFT4_RAW).expect("json");
        super::normalize_draft4_shapes(&mut raw, Dialect::for_spec_version(SpecVersion::Draft04));
        let expected: serde_json::Value =
            serde_json::json!({"id": {"type": "string", "$id": "#name"}});
        let actual: serde_json::Value = raw["properties"].clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalized_in_dependencies_leaving_string_arrays() {
        let mut raw: serde_json::Value = serde_json::from_str(DRAFT4_RAW).expect("json");
        super::normalize_draft4_shapes(&mut raw, Dialect::for_spec_version(SpecVersion::Draft04));
        let expected: serde_json::Value = serde_json::json!({"b": ["c"], "d": {"$id": "#d"}});
        let actual: serde_json::Value = raw["dependencies"].clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalization_keeps_existing_dollar_id() {
        let mut raw: serde_json::Value = serde_json::json!({"$id": "http://example.com/dollar", "id": "http://example.com/plain"});
        super::normalize_draft4_shapes(&mut raw, Dialect::for_spec_version(SpecVersion::Draft04));
        let expected: serde_json::Value = serde_json::json!({"$id": "http://example.com/dollar", "id": "http://example.com/plain"});
        let actual: serde_json::Value = raw;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_id_normalization_leaves_non_string_id() {
        let mut raw: serde_json::Value = serde_json::json!({"id": 5});
        super::normalize_draft4_shapes(&mut raw, Dialect::for_spec_version(SpecVersion::Draft04));
        let expected: serde_json::Value = serde_json::json!({"id": 5});
        let actual: serde_json::Value = raw;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_escaped_id_key_normalized() {
        let json = r#"{"$schema":"http://json-schema.org/draft-04/schema#","\u0069d":"http://example.com/e.json"}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("http://example.com/e.json".to_string());
        let actual: Option<String> = schema.id;
        assert_eq!(expected, actual);
    }

    #[test]
    fn pinned_draft4_normalizes_id_without_schema_uri() {
        let json = r#"{"id":"http://example.com/p.json"}"#;
        let schema: JsonSchema =
            JsonSchema::new_from_str(json, &SpecVersion::Draft04.default_schema_settings())
                .expect("parse");
        let expected: Option<String> = Some("http://example.com/p.json".to_string());
        let actual: Option<String> = schema.id;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_id_is_not_normalized() {
        let json = r#"{"$schema":"http://json-schema.org/draft-06/schema#","id":"http://example.com/x.json"}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = None;
        let actual: Option<String> = schema.id;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_id_rejected_when_strict() {
        let json = r#"{"$schema":"http://json-schema.org/draft-06/schema#","id":"http://example.com/x.json"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_str(json, &settings);
        assert!(actual.is_err());
    }

    #[test]
    fn parse_dollar_comment_preserved() {
        let json = r#"{"type":"object","properties":{},"$comment":"Created by John Doe"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<String> = Some("Created by John Doe".to_string());
        assert_eq!(expected, parsed.comment);
    }

    #[test]
    fn parse_without_comment_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        assert_eq!(None, parsed.comment);
    }

    #[test]
    fn round_trip_preserves_dollar_comment() {
        let json = r#"{"type":"object","properties":{"country":{"type":"string","$comment":"TODO: add enum"}},"$comment":"Root schema"}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let serialized: String = (&parsed).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(serialized.as_str()).expect("parse again");
        assert_eq!(parsed.comment, reparsed.comment);
        let country_schema: &JsonSchema = reparsed.properties.get("country").expect("country");
        assert_eq!(Some("TODO: add enum".to_string()), country_schema.comment);
    }

    #[test]
    fn parse_strict_accepts_dollar_comment() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}},"$comment":"Note"}"#;
        let result: Result<JsonSchema, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(Some("Note".to_string()), parsed.comment);
    }

    #[test]
    fn deserialize_schema_with_enum() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}}}"#;
        let parsed: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let status_schema: &JsonSchema = parsed.properties.get("status").expect("status property");
        let expected_enum: Vec<serde_json::Value> = vec![
            serde_json::Value::String("open".to_string()),
            serde_json::Value::String("closed".to_string()),
        ];
        let actual: Option<&Vec<serde_json::Value>> = status_schema.enum_values.as_ref();
        assert_eq!(Some(&expected_enum), actual);
    }

    #[test]
    fn parse_strict_accepts_enum_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"x":{"enum":["a","b"]}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_const_string() {
        let json = r#"{"const":"only"}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> =
            Some(serde_json::Value::String("only".to_string()));
        let actual: Option<&serde_json::Value> = schema.const_value.as_ref();
        assert_eq!(expected.as_ref(), actual);
    }

    #[test]
    fn parse_const_number() {
        let json = r#"{"const":42}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> = Some(serde_json::Value::Number(42_i64.into()));
        let actual: Option<&serde_json::Value> = schema.const_value.as_ref();
        assert_eq!(expected.as_ref(), actual);
    }

    #[test]
    fn parse_const_round_trip() {
        let schema: JsonSchema = JsonSchema {
            const_value: Some(serde_json::Value::String("x".to_string())),
            ..Default::default()
        };
        let json: String = serde_json::to_string(&schema).expect("serialize");
        let parsed: JsonSchema = serde_json::from_str(&json).expect("parse");
        assert_eq!(schema.const_value, parsed.const_value);
    }

    #[test]
    fn parse_strict_accepts_const_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"x":{"const":"fixed"}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_default_string_has_value() {
        let json = r#"{"type":"string","default":"foo"}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> =
            Some(serde_json::Value::String("foo".to_string()));
        let actual: Option<serde_json::Value> = schema.default_value.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_default_string_round_trip() {
        let json = r#"{"type":"string","default":"foo"}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let serialized: String = serde_json::to_string(&schema).expect("serialize");
        let round_trip: JsonSchema = serde_json::from_str(&serialized).expect("round-trip");
        let expected: Option<serde_json::Value> = schema.default_value.clone();
        let actual: Option<serde_json::Value> = round_trip.default_value.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_default_number_has_value() {
        let json = r#"{"type":"integer","default":42}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let expected: Option<serde_json::Value> = Some(serde_json::Value::Number(42_i64.into()));
        let actual: Option<serde_json::Value> = schema.default_value.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_default_number_round_trip() {
        let json = r#"{"type":"integer","default":42}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        let serialized: String = serde_json::to_string(&schema).expect("serialize");
        let round_trip: JsonSchema = serde_json::from_str(&serialized).expect("round-trip");
        let expected: Option<serde_json::Value> = schema.default_value.clone();
        let actual: Option<serde_json::Value> = round_trip.default_value.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_default_null_accepted() {
        let json = r#"{"type":"string","default":null}"#;
        let schema: JsonSchema = serde_json::from_str(json).expect("parse");
        // Serde deserializes JSON null into Option<Value> as None; we assert the parsed value.
        let expected: Option<serde_json::Value> = None;
        let actual: Option<serde_json::Value> = schema.default_value.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_accepts_default_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"a":{"type":"string","default":"x"}}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        let expected: bool = true;
        let actual: bool = result.is_ok();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_object_two_properties_with_default() {
        let one_str: JsonSchema = JsonSchema::try_from(
            r#"{"type":"object","properties":{"name":{"type":"string","default":"foo"}}}"#,
        )
        .expect("parse one string default");
        let expected: Option<&serde_json::Value> = Some(&serde_json::json!("foo"));
        let actual: Option<&serde_json::Value> = one_str
            .properties
            .get("name")
            .unwrap()
            .default_value
            .as_ref();
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string_const_true_when_string() {
        let schema: JsonSchema = JsonSchema {
            const_value: Some(serde_json::Value::String("a".to_string())),
            ..Default::default()
        };
        let expected: bool = true;
        let actual: bool = schema.is_string_const();
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_string_const_false_when_non_string() {
        let schema: JsonSchema = JsonSchema {
            const_value: Some(serde_json::Value::Number(1_i64.into())),
            ..Default::default()
        };
        let expected: bool = false;
        let actual: bool = schema.is_string_const();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_accepts_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_strict_accepts_unique_items_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"uniqueItems":true}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_min_items_max_items() {
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected_min: Option<u64> = Some(2);
        let expected_max: Option<u64> = Some(5);
        assert_eq!(expected_min, actual.min_items);
        assert_eq!(expected_max, actual.max_items);
    }

    #[test]
    fn parse_strict_accepts_min_items_max_items() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":10}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn deserialize_integer_with_minimum_and_maximum() {
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: (Option<serde_json::Number>, Option<serde_json::Number>) =
            (Some(0_u64.into()), Some(255_u64.into()));
        assert_eq!(expected, (actual.minimum, actual.maximum));
    }

    #[test]
    fn deserialize_integer_with_minimum_only() {
        let json = r#"{"type":"integer","minimum":-100}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: (Option<serde_json::Number>, Option<serde_json::Number>) =
            (Some((-100_i64).into()), None);
        assert_eq!(expected, (actual.minimum, actual.maximum));
    }

    #[test]
    fn deserialize_number_with_minimum_and_maximum_float() {
        let json = r#"{"type":"number","minimum":0.5,"maximum":100.5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: (Option<serde_json::Number>, Option<serde_json::Number>) = (
            serde_json::Number::from_f64(0.5),
            serde_json::Number::from_f64(100.5),
        );
        assert_eq!(expected, (actual.minimum, actual.maximum));
    }

    #[test]
    fn deserialize_integer_with_maximum_only() {
        let json = r#"{"type":"integer","maximum":100}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: (Option<serde_json::Number>, Option<serde_json::Number>) =
            (None, Some(100_u64.into()));
        assert_eq!(expected, (actual.minimum, actual.maximum));
    }

    #[test]
    fn deserialize_number_with_maximum_only() {
        let json = r#"{"type":"number","maximum":99.5}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: (Option<serde_json::Number>, Option<serde_json::Number>) =
            (None, serde_json::Number::from_f64(99.5));
        assert_eq!(expected, (actual.minimum, actual.maximum));
    }

    #[test]
    fn parse_strict_accepts_minimum_and_maximum() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"integer","minimum":0,"maximum":255}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    // --- Numbers are held exactly ---

    /// Parses `json` with `settings` and serializes the model back to compact JSON.
    fn reserialize(json: &str, settings: &JsonSchemaSettings) -> String {
        let schema: JsonSchema = JsonSchema::new_from_str(json, settings).expect("parse");
        String::try_from(&schema).expect("serialize")
    }

    #[test]
    fn numeric_keywords_round_trip_exactly() {
        let json = r#"{"type":"integer","minimum":9007199254740993,"maximum":18446744073709551615,"exclusiveMinimum":-9223372036854775808,"exclusiveMaximum":1.5,"multipleOf":0.0001}"#;
        let expected: &str = json;
        let actual: String = reserialize(json, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_accepts_exclusive_bounds_and_multiple_of_keys() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json =
            r#"{"type":"number","exclusiveMinimum":0,"exclusiveMaximum":10,"multipleOf":0.5}"#;
        let expected: &str = json;
        let actual: String = reserialize(json, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_null_is_present() {
        let actual: JsonSchema = JsonSchema::try_from(r#"{"const":null}"#).expect("parse");
        let expected: Option<serde_json::Value> = Some(serde_json::Value::Null);
        assert_eq!(expected, actual.const_value);
    }

    #[test]
    fn const_null_is_present_when_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: &str = r#"{"const":null}"#;
        let actual: String = reserialize(r#"{"const":null}"#, &settings);
        assert_eq!(expected, actual);
    }

    // --- Draft-4 boolean exclusiveMinimum / exclusiveMaximum become numeric bounds ---

    const DRAFT4_BOUNDS: &str = r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"a":{"minimum":1.1,"exclusiveMinimum":true,"maximum":3,"exclusiveMaximum":false},"b":{"maximum":3,"exclusiveMaximum":true},"c":{"exclusiveMinimum":true},"d":{"minimum":0,"exclusiveMinimum":5}}}"#;

    const DRAFT4_BOUNDS_NORMALIZED: &str = r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"a":{"maximum":3,"exclusiveMinimum":1.1},"b":{"exclusiveMaximum":3},"c":{},"d":{"minimum":0,"exclusiveMinimum":5}}}"#;

    #[test]
    fn draft4_boolean_exclusive_bounds_normalized_from_str() {
        let expected: &str = DRAFT4_BOUNDS_NORMALIZED;
        let actual: String = reserialize(DRAFT4_BOUNDS, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_boolean_exclusive_bounds_normalized_from_slice() {
        let schema: JsonSchema = JsonSchema::try_from(DRAFT4_BOUNDS.as_bytes()).expect("parse");
        let expected: &str = DRAFT4_BOUNDS_NORMALIZED;
        let actual: String = String::try_from(&schema).expect("serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_boolean_exclusive_bounds_normalized_from_serde_value() {
        let value: serde_json::Value = serde_json::from_str(DRAFT4_BOUNDS).expect("json");
        let schema: JsonSchema = JsonSchema::try_from(&value).expect("parse");
        let expected: &str = DRAFT4_BOUNDS_NORMALIZED;
        let actual: String = String::try_from(&schema).expect("serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_boolean_exclusive_bounds_normalized_when_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: &str = DRAFT4_BOUNDS_NORMALIZED;
        let actual: String = reserialize(DRAFT4_BOUNDS, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_bounds_setting_normalizes_boolean_form_in_2020_12() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .exclusive_bounds(ExclusiveBounds::BooleanModifier)
            .build();
        let expected: &str = r#"{"exclusiveMinimum":1}"#;
        let actual: String = reserialize(r#"{"minimum":1,"exclusiveMinimum":true}"#, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_exclusive_minimum_is_rejected_in_2020_12() {
        let actual: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(r#"{"minimum":1,"exclusiveMinimum":true}"#);
        assert!(matches!(actual, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn invalid_schema_mentioning_exclusive_bound_keeps_direct_parse_error() {
        let json = "{\"exclusiveMinimum\":1,\n\"minLength\":\"x\"}";
        let expected: String =
            "invalid type: string \"x\", expected u64 at line 2 column 15".to_string();
        let actual: String = match JsonSchema::try_from(json) {
            Err(JsonSchemaParseError::Serde(e)) => e.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_accepts_format_key() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"string","format":"uuid"}"#;
        let result: Result<_, _> = JsonSchema::new_from_str(json, &settings);
        assert!(result.is_ok());
    }

    #[test]
    fn parse_schema_from_serde_value_lenient_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let actual: JsonSchema = JsonSchema::try_from(&value).expect("parse");
        let expected: JsonSchema =
            JsonSchema::try_from(r#"{"type":"object","properties":{"a":{"type":"string"}}}"#)
                .expect("reference parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_schema_from_serde_value_strict_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {}, "title": "Root"});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let actual: JsonSchema =
            JsonSchema::new_from_serde_value(&value, &settings).expect("parse");
        let expected: JsonSchema = JsonSchema::new_from_str(
            r#"{"type":"object","properties":{},"title":"Root"}"#,
            &settings,
        )
        .expect("reference parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_schema_from_serde_value_strict_rejects_unknown_key() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {}, "unknown": 42});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_serde_value(&value, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn parse_schema_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseError> = JsonSchema::try_from(&value);
        let actual_is_err: bool = result.is_err();
        assert!(actual_is_err, "string value should fail to parse as schema");
    }

    #[test]
    fn parse_schema_from_serde_value_round_trip_with_new_from_str() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"]}"#;
        let from_str: JsonSchema = JsonSchema::try_from(json).expect("parse from str");
        let serialized: String = (&from_str).try_into().expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&serialized).expect("str to value");
        let from_value: JsonSchema = JsonSchema::try_from(&value).expect("parse from value");
        assert_eq!(from_str, from_value);
    }

    #[test]
    fn parse_schema_from_reader_success() {
        let bytes: &[u8] = r#"{"type":"object","properties":{}}"#.as_bytes();
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(bytes);
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let actual: JsonSchema = JsonSchema::new_from_reader(reader, &settings).expect("parse");
        let expected_type: Option<&str> = Some("object");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_reader_invalid_json() {
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(b"not json");
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    struct FailingReader;

    impl io::Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("test"))
        }
    }

    #[test]
    fn parse_schema_from_reader_io_error() {
        let reader: FailingReader = FailingReader;
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn parse_schema_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("parse");
        let expected_type: Option<&str> = Some("array");
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn parse_schema_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Io(_))));
    }

    #[test]
    fn parse_schema_from_path_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let result: Result<JsonSchema, JsonSchemaParseError> =
            JsonSchema::try_from(schema_path.as_path());
        assert!(matches!(result, Err(JsonSchemaParseError::Serde(_))));
    }

    #[test]
    fn parse_defs_present() {
        let json = r#"{"$defs":{"Foo":{"type":"string"}}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let mut expected_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        expected_defs.insert(
            "Foo".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let expected: JsonSchema = JsonSchema {
            defs: Some(expected_defs),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_defs_empty_object() {
        let json = r#"{"$defs":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::new()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_defs_absent_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_definitions_present() {
        let json = r#"{"definitions":{"Bar":{"type":"integer"}}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let mut expected_definitions: BTreeMap<String, JsonSchema> = BTreeMap::new();
        expected_definitions.insert(
            "Bar".to_string(),
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        );
        let expected: JsonSchema = JsonSchema {
            definitions: Some(expected_definitions),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_definitions_empty_object() {
        let json = r#"{"definitions":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            definitions: Some(BTreeMap::new()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_definitions_absent_is_none() {
        let json = r#"{"type":"object","properties":{}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_with_defs_and_definitions() {
        let mut defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        defs.insert(
            "Foo".to_string(),
            JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            },
        );
        let mut definitions: BTreeMap<String, JsonSchema> = BTreeMap::new();
        definitions.insert(
            "Bar".to_string(),
            JsonSchema {
                type_: Some("integer".to_string()),
                ..Default::default()
            },
        );
        let schema: JsonSchema = JsonSchema {
            defs: Some(defs),
            definitions: Some(definitions),
            ..Default::default()
        };
        let json: String = (&schema).try_into().expect("serialize");
        let reparsed: JsonSchema = JsonSchema::try_from(json.as_str()).expect("parse");
        let expected: JsonSchema = schema;
        let actual: JsonSchema = reparsed;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_ref_only_schema_stores_ref_string() {
        let json = r##"{"$ref":"#/$defs/Foo"}"##;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            ref_: Some("#/$defs/Foo".to_string()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }
}
