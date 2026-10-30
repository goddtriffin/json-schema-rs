//! In-memory representation of JSON Schema for codegen.

use super::dialect::{
    Dialect, ExclusiveBounds, IntegerSemantics, ResolvedDialect, SubschemaPosition,
};
use super::error::{JsonSchemaParseError, JsonSchemaParseErrors, JsonSchemaParseResult};
use super::settings::{DialectSettings, JsonSchemaSettings, resolve_dialect_for_uri};
use super::type_set::TypeSet;
use crate::json_pointer::JsonPointer;
use memchr::memmem;
use serde::de::{self, DeserializeOwned, DeserializeSeed, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;
use std::sync::LazyLock;

/// Returns true when `required` should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_required(v: &Option<Vec<String>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// Returns true when a defs/definitions map should be omitted from serialized output (None or empty).
#[expect(clippy::ref_option)]
fn skip_defs_map(v: &Option<BTreeMap<String, Schema>>) -> bool {
    v.as_ref().is_none_or(BTreeMap::is_empty)
}

/// Returns true when an `allOf` / `anyOf` / `oneOf` list should be omitted from serialized
/// output (None or empty).
#[expect(clippy::ref_option)]
fn skip_schema_list(v: &Option<Vec<Schema>>) -> bool {
    v.as_ref().is_none_or(Vec::is_empty)
}

/// A JSON Schema at any schema position: a boolean schema or a schema object.
///
/// `true` accepts every instance (like `{}`) and `false` accepts none. Every subschema position
/// of [`JsonSchema`] (`properties` values, `items`, `additionalProperties`, `$defs`, `allOf`,
/// ...) holds a `Schema`, and so does a whole document: parse one with
/// [`Schema::new_from_str`] and friends. A boolean schema is accepted in every draft, draft 4
/// included (which only defines it for `additionalProperties`), since its meaning is the same.
#[derive(Debug, Clone, PartialEq)]
pub enum Schema {
    /// A boolean schema: `true` accepts everything, `false` nothing.
    Bool(bool),
    /// A schema object.
    Object(Box<JsonSchema>),
}

impl Default for Schema {
    /// The schema that accepts everything (`true`).
    fn default() -> Self {
        Schema::Bool(true)
    }
}

impl From<JsonSchema> for Schema {
    fn from(schema: JsonSchema) -> Self {
        Schema::Object(Box::new(schema))
    }
}

impl From<bool> for Schema {
    fn from(value: bool) -> Self {
        Schema::Bool(value)
    }
}

/// The empty schema object, which stands in for a boolean schema where a consumer needs a
/// [`JsonSchema`] (see [`Schema::object_or_empty`]).
static EMPTY_SCHEMA: LazyLock<JsonSchema> = LazyLock::new(JsonSchema::default);

impl Schema {
    /// The schema object, or `None` for a boolean schema.
    #[must_use]
    pub fn as_object(&self) -> Option<&JsonSchema> {
        match self {
            Schema::Bool(_) => None,
            Schema::Object(schema) => Some(schema),
        }
    }

    /// The schema object, or `None` for a boolean schema.
    #[must_use]
    pub fn as_object_mut(&mut self) -> Option<&mut JsonSchema> {
        match self {
            Schema::Bool(_) => None,
            Schema::Object(schema) => Some(schema),
        }
    }

    /// The schema object; for a boolean schema, the empty schema object `{}`. For consumers
    /// that describe a value's shape rather than decide validity (codegen): `true` and `{}`
    /// mean the same, and `false` (no value is valid) has no shape of its own.
    #[must_use]
    pub fn object_or_empty(&self) -> &JsonSchema {
        self.as_object().unwrap_or(&EMPTY_SCHEMA)
    }

    /// Borrows this schema as a [`SchemaRef`].
    #[must_use]
    pub fn as_schema_ref(&self) -> SchemaRef<'_> {
        match self {
            Schema::Bool(value) => SchemaRef::Bool(*value),
            Schema::Object(schema) => SchemaRef::Object(schema),
        }
    }

    /// Parses a JSON Schema document (a schema object or a boolean schema) from a string with
    /// the given settings. See [`JsonSchema::new_from_str`] for the parsing rules.
    ///
    /// # Errors
    ///
    /// Same as [`JsonSchema::new_from_str`], except that a boolean document is accepted.
    pub fn new_from_str(json: &str, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<Self> {
        parse_raw(RawSchema::Str(json), settings)
    }

    /// Parses a JSON Schema document from a byte slice with the given settings.
    ///
    /// # Errors
    ///
    /// Same as [`Schema::new_from_str`].
    pub fn new_from_slice(
        slice: &[u8],
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        parse_raw(RawSchema::Slice(slice), settings)
    }

    /// Parses a JSON Schema document from an already-parsed [`serde_json::Value`] with the given
    /// settings. The value is borrowed and never modified.
    ///
    /// # Errors
    ///
    /// Same as [`Schema::new_from_str`].
    pub fn new_from_serde_value(
        value: &serde_json::Value,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        parse_value(Cow::Borrowed(value), settings)
    }

    /// Parses a JSON Schema document from a reader with the given settings.
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on read failure; otherwise same as
    /// [`Schema::new_from_str`].
    pub fn new_from_reader<R: Read>(
        mut reader: R,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let mut buf: Vec<u8> = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(JsonSchemaParseError::Io)?;
        Self::new_from_slice(&buf, settings)
    }

    /// Parses a JSON Schema document from a file path with the given settings.
    ///
    /// # Errors
    ///
    /// Returns [`JsonSchemaParseError::Io`] on open or read failure; otherwise same as
    /// [`Schema::new_from_str`].
    pub fn new_from_path<P: AsRef<std::path::Path>>(
        path: P,
        settings: &JsonSchemaSettings,
    ) -> JsonSchemaParseResult<Self> {
        let f: std::fs::File = std::fs::File::open(path.as_ref())?;
        Self::new_from_reader(f, settings)
    }
}

impl Serialize for Schema {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Schema::Bool(value) => serializer.serialize_bool(*value),
            Schema::Object(schema) => schema.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Schema {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(SchemaVisitor)
    }
}

/// Reads a schema position: a boolean or a schema object; anything else names that shape.
struct SchemaVisitor;

impl<'de> Visitor<'de> for SchemaVisitor {
    type Value = Schema;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a schema (an object or a boolean)")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Schema, E> {
        Ok(Schema::Bool(v))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Schema, A::Error> {
        JsonSchemaVisitor
            .visit_map(map)
            .map(|schema| Schema::Object(Box::new(schema)))
    }
}

/// The value of `items`: one schema, or the array of schemas drafts 4 to 2019-09 define (the
/// tuple form), which the model keeps raw in [`JsonSchema::extensions`].
enum ItemsValue {
    Schema(Schema),
    Array(Value),
}

impl<'de> Deserialize<'de> for ItemsValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ItemsVisitor)
    }
}

/// Reads `items`: a schema position that may also hold an array of schemas.
struct ItemsVisitor;

impl<'de> Visitor<'de> for ItemsVisitor {
    type Value = ItemsValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a schema (an object or a boolean) or an array of schemas")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<ItemsValue, E> {
        Ok(ItemsValue::Schema(Schema::Bool(v)))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<ItemsValue, A::Error> {
        SchemaVisitor.visit_map(map).map(ItemsValue::Schema)
    }

    fn visit_seq<A: de::SeqAccess<'de>>(self, seq: A) -> Result<ItemsValue, A::Error> {
        Value::deserialize(de::value::SeqAccessDeserializer::new(seq)).map(ItemsValue::Array)
    }
}

/// A borrowed [`Schema`]: a boolean schema or a reference to a schema object. What the validator
/// takes, so it accepts a `&Schema` and a `&JsonSchema` alike.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchemaRef<'a> {
    /// A boolean schema.
    Bool(bool),
    /// A schema object.
    Object(&'a JsonSchema),
}

impl<'a> SchemaRef<'a> {
    /// The schema object, or `None` for a boolean schema.
    #[must_use]
    pub fn as_object(self) -> Option<&'a JsonSchema> {
        match self {
            SchemaRef::Bool(_) => None,
            SchemaRef::Object(schema) => Some(schema),
        }
    }

    /// The schema object; for a boolean schema, the empty schema object (see
    /// [`Schema::object_or_empty`]).
    #[must_use]
    pub fn object_or_empty(self) -> &'a JsonSchema {
        self.as_object().unwrap_or(&EMPTY_SCHEMA)
    }
}

impl<'a> From<&'a Schema> for SchemaRef<'a> {
    fn from(schema: &'a Schema) -> Self {
        schema.as_schema_ref()
    }
}

impl<'a> From<&'a JsonSchema> for SchemaRef<'a> {
    fn from(schema: &'a JsonSchema) -> Self {
        SchemaRef::Object(schema)
    }
}

/// Deserializes a keyword whose presence matters even when its value is `null` (`"const": null`):
/// a present key is always `Some`, so `null` becomes `Some(Value::Null)` rather than `None`.
struct Present;

impl<'de> DeserializeSeed<'de> for Present {
    type Value = Option<Value>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Option<Value>, D::Error> {
        Value::deserialize(deserializer).map(Some)
    }
}

/// Deserializes `multipleOf`, which every draft requires to be strictly greater than 0 (draft 4
/// validation §5.1.1.1; draft 6+ "MUST be a number, strictly greater than 0"). Zero or a negative
/// value makes the schema invalid, so it is a parse error, like `"minLength": -1`.
struct MultipleOf;

impl<'de> DeserializeSeed<'de> for MultipleOf {
    type Value = Option<serde_json::Number>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let Some(n) = Option::<serde_json::Number>::deserialize(deserializer)? else {
            return Ok(None);
        };
        if n.as_f64().is_some_and(|f| f > 0.0) {
            Ok(Some(n))
        } else {
            Err(de::Error::custom(format!(
                "multipleOf must be strictly greater than 0, got {n}"
            )))
        }
    }
}

/// Deserializes a count keyword (`minLength`, `maxLength`, `minItems`, `maxItems`). Since draft 6
/// these are non-negative integers by value, so any number with a zero fractional part that fits
/// a `u64` is accepted (`2`, `2.0`, `2e0`); a negative, fractional or out-of-range value makes
/// the schema invalid. Draft 4 asks for an integer too; its value-based reading is accepted in
/// every draft (design.md, *minLength / maxLength*).
struct NonNegativeInteger;

impl<'de> DeserializeSeed<'de> for NonNegativeInteger {
    type Value = Option<u64>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let Some(n) = Option::<serde_json::Number>::deserialize(deserializer)? else {
            return Ok(None);
        };
        non_negative_integer(&n)
            .map(Some)
            .ok_or_else(|| de::Error::custom(format!("expected a non-negative integer, got {n}")))
    }
}

/// The value of `n` as a `u64` when it is a non-negative integer by value: an unsigned integer,
/// or a float with a zero fractional part below 2^64.
fn non_negative_integer(n: &serde_json::Number) -> Option<u64> {
    // 2^64: every float with a zero fraction in [0, 2^64) converts to `u64` exactly.
    const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;
    if let Some(u) = n.as_u64() {
        return Some(u);
    }
    let f: f64 = n.as_f64()?;
    if (0.0..U64_LIMIT).contains(&f) && f.fract() == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "f is a non-negative whole number below 2^64, so the cast is exact"
        )]
        let u: u64 = f as u64;
        Some(u)
    } else {
        None
    }
}

/// Schema model used for code generation and validation: one schema object.
///
/// Every keyword the model holds is a field; every other key of the object (a keyword this
/// model does not hold, such as `not` or `if`, an `x-` extension, or any unknown key) is kept
/// in [`extensions`](Self::extensions), so parsing then serializing loses nothing.
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

    /// The `type` keyword: the set of JSON types an instance may have. `None` when absent (any
    /// type). Read with [`TypeSet`] queries.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<TypeSet>,

    /// Reusable subschemas (draft 2019-09+). Stored and round-tripped.
    #[serde(rename = "$defs", skip_serializing_if = "skip_defs_map")]
    pub defs: Option<BTreeMap<String, Schema>>,

    /// Reusable subschemas (legacy drafts). Stored and round-tripped.
    #[serde(skip_serializing_if = "skip_defs_map")]
    pub definitions: Option<BTreeMap<String, Schema>>,

    /// Object properties. Default empty; `null` reads as empty. Use `BTreeMap` for stable ordering.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Schema>,

    /// Schema for the values of properties not in `properties` (we do not implement
    /// `patternProperties`, so "additional" means any key not in `properties`). `None` when
    /// absent, which allows them like `true`; `false` forbids them.
    #[serde(
        rename = "additionalProperties",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_properties: Option<Schema>,

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
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<serde_json::Value>>,

    /// Single allowed value for the instance (JSON Schema `const`, draft-06+). When present, instance must equal this value (numbers by value: `1` equals `1.0`). `"const": null` is `Some(Value::Null)`. Codegen uses only string const (single-variant enum).
    #[serde(rename = "const", skip_serializing_if = "Option::is_none")]
    pub const_value: Option<serde_json::Value>,

    /// Schema for all array elements. Single-schema form only; the array form (tuple typing,
    /// drafts 4 to 2019-09) is a parse error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Schema>,

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
    /// design.md, *multipleOf*). Parsing rejects zero and negative values, which every draft
    /// forbids. Used for validation.
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

    /// allOf: instance must validate against every subschema. Stored as-is at ingestion; validator validates each; codegen merges on-the-fly.
    #[serde(rename = "allOf", skip_serializing_if = "skip_schema_list")]
    pub all_of: Option<Vec<Schema>>,

    /// anyOf: instance must validate against at least one subschema. Stored as-is at ingestion; validator and codegen (union enum) use it. Not emitted by reverse codegen.
    #[serde(rename = "anyOf", skip_serializing_if = "skip_schema_list")]
    pub any_of: Option<Vec<Schema>>,

    /// oneOf: instance must validate against exactly one subschema. Stored as-is at ingestion; validator and codegen (union enum) use it. Not emitted by reverse codegen.
    #[serde(rename = "oneOf", skip_serializing_if = "skip_schema_list")]
    pub one_of: Option<Vec<Schema>>,

    /// Every key of the schema object that no field above holds, with its value as written:
    /// keywords this model does not hold (`not`, `if`, `patternProperties`, ...), `x-`
    /// extensions and unknown keys. Serialized after the fields, in key order. A key that a
    /// field holds never appears here when parsed; one inserted by hand would be written twice.
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,

    /// The dialect settings this root was parsed with, when any is set (a forced or default
    /// draft, a per-flag override); `None` means the defaults (follow `$schema`, Draft 2020-12
    /// when absent). [`JsonSchema::dialect`] and the [`Registry`](super::Registry) read them;
    /// the registry applies them to every schema resource of the document. `None` on every
    /// subschema. Not serialized.
    #[serde(skip)]
    pub dialect_settings: Option<DialectSettings>,
}

/// Generates the [`Field`] enum of the keys [`JsonSchema`] holds, its lookup from a key, and the
/// key of each field, from one list so they cannot drift apart.
macro_rules! schema_fields {
    ($($variant:ident => $key:literal),* $(,)?) => {
        /// A key of a schema object: one of the fields of [`JsonSchema`], or an extension.
        enum Field {
            $($variant,)*
            Extension(String),
        }

        impl Field {
            /// The field holding `key`, or [`Field::Extension`].
            fn from_key(key: &str) -> Field {
                match key {
                    $($key => Field::$variant,)*
                    _ => Field::Extension(key.to_string()),
                }
            }
        }
    };
}

schema_fields! {
    Schema => "$schema",
    Id => "$id",
    Ref => "$ref",
    Type => "type",
    Defs => "$defs",
    Definitions => "definitions",
    Properties => "properties",
    AdditionalProperties => "additionalProperties",
    Required => "required",
    Title => "title",
    Description => "description",
    Comment => "$comment",
    Enum => "enum",
    Const => "const",
    Items => "items",
    UniqueItems => "uniqueItems",
    MinItems => "minItems",
    MaxItems => "maxItems",
    Minimum => "minimum",
    Maximum => "maximum",
    ExclusiveMinimum => "exclusiveMinimum",
    ExclusiveMaximum => "exclusiveMaximum",
    MultipleOf => "multipleOf",
    MinLength => "minLength",
    MaxLength => "maxLength",
    Pattern => "pattern",
    Format => "format",
    Default => "default",
    Deprecated => "deprecated",
    Examples => "examples",
    AllOf => "allOf",
    AnyOf => "anyOf",
    OneOf => "oneOf",
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FieldVisitor;
        impl Visitor<'_> for FieldVisitor {
            type Value = Field;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a keyword")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Field, E> {
                Ok(Field::from_key(v))
            }
        }
        deserializer.deserialize_identifier(FieldVisitor)
    }
}

impl<'de> Deserialize<'de> for JsonSchema {
    /// Reads a schema object. Accepts only a JSON object; anything else (a boolean schema
    /// included: use [`Schema`] for a position that may hold one) is an error naming that shape.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(JsonSchemaVisitor)
    }
}

/// Reads a schema object key by key into [`JsonSchema`].
struct JsonSchemaVisitor;

impl<'de> Visitor<'de> for JsonSchemaVisitor {
    type Value = JsonSchema;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a schema object")
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per field of JsonSchema; splitting it would scatter the field list"
    )]
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JsonSchema, A::Error> {
        let mut schema: JsonSchema = JsonSchema::default();
        // Which fields have been read, by their index in `set!` below.
        let mut seen: [bool; 33] = [false; 33];
        /// Stores `$read` in `$field`, rejecting a key read before (as a derived `Deserialize`
        /// does, even when the first value was `null`).
        macro_rules! set {
            ($index:literal, $field:expr, $key:literal, $read:expr) => {{
                if std::mem::replace(&mut seen[$index], true) {
                    return Err(de::Error::duplicate_field($key));
                }
                $field = $read;
            }};
        }
        while let Some(field) = map.next_key::<Field>()? {
            match field {
                Field::Schema => set!(0, schema.schema, "$schema", map.next_value()?),
                Field::Id => set!(1, schema.id, "$id", map.next_value()?),
                Field::Ref => set!(2, schema.ref_, "$ref", map.next_value()?),
                Field::Type => set!(3, schema.type_, "type", map.next_value()?),
                Field::Defs => set!(4, schema.defs, "$defs", map.next_value()?),
                Field::Definitions => {
                    set!(5, schema.definitions, "definitions", map.next_value()?);
                }
                Field::Properties => set!(
                    6,
                    schema.properties,
                    "properties",
                    map.next_value::<Option<BTreeMap<String, Schema>>>()?
                        .unwrap_or_default()
                ),
                Field::AdditionalProperties => set!(
                    7,
                    schema.additional_properties,
                    "additionalProperties",
                    map.next_value()?
                ),
                Field::Required => set!(8, schema.required, "required", map.next_value()?),
                Field::Title => set!(9, schema.title, "title", map.next_value()?),
                Field::Description => {
                    set!(10, schema.description, "description", map.next_value()?);
                }
                Field::Comment => set!(11, schema.comment, "$comment", map.next_value()?),
                Field::Enum => set!(12, schema.enum_values, "enum", map.next_value()?),
                Field::Const => set!(
                    13,
                    schema.const_value,
                    "const",
                    map.next_value_seed(Present)?
                ),
                Field::Items => {
                    let (items, array): (Option<Schema>, Option<Value>) =
                        match map.next_value::<Option<ItemsValue>>()? {
                            Some(ItemsValue::Schema(items)) => (Some(items), None),
                            Some(ItemsValue::Array(array)) => (None, Some(array)),
                            None => (None, None),
                        };
                    set!(14, schema.items, "items", items);
                    if let Some(array) = array {
                        schema.extensions.insert("items".to_string(), array);
                    }
                }
                Field::UniqueItems => {
                    set!(15, schema.unique_items, "uniqueItems", map.next_value()?);
                }
                Field::MinItems => set!(
                    16,
                    schema.min_items,
                    "minItems",
                    map.next_value_seed(NonNegativeInteger)?
                ),
                Field::MaxItems => set!(
                    17,
                    schema.max_items,
                    "maxItems",
                    map.next_value_seed(NonNegativeInteger)?
                ),
                Field::Minimum => set!(18, schema.minimum, "minimum", map.next_value()?),
                Field::Maximum => set!(19, schema.maximum, "maximum", map.next_value()?),
                Field::ExclusiveMinimum => set!(
                    20,
                    schema.exclusive_minimum,
                    "exclusiveMinimum",
                    map.next_value()?
                ),
                Field::ExclusiveMaximum => set!(
                    21,
                    schema.exclusive_maximum,
                    "exclusiveMaximum",
                    map.next_value()?
                ),
                Field::MultipleOf => set!(
                    22,
                    schema.multiple_of,
                    "multipleOf",
                    map.next_value_seed(MultipleOf)?
                ),
                Field::MinLength => set!(
                    23,
                    schema.min_length,
                    "minLength",
                    map.next_value_seed(NonNegativeInteger)?
                ),
                Field::MaxLength => set!(
                    24,
                    schema.max_length,
                    "maxLength",
                    map.next_value_seed(NonNegativeInteger)?
                ),
                Field::Pattern => set!(25, schema.pattern, "pattern", map.next_value()?),
                Field::Format => set!(26, schema.format, "format", map.next_value()?),
                Field::Default => {
                    set!(27, schema.default_value, "default", map.next_value()?);
                }
                Field::Deprecated => {
                    set!(28, schema.deprecated, "deprecated", map.next_value()?);
                }
                Field::Examples => set!(29, schema.examples, "examples", map.next_value()?),
                Field::AllOf => set!(30, schema.all_of, "allOf", map.next_value()?),
                Field::AnyOf => set!(31, schema.any_of, "anyOf", map.next_value()?),
                Field::OneOf => set!(32, schema.one_of, "oneOf", map.next_value()?),
                Field::Extension(key) => {
                    let value: Value = map.next_value()?;
                    if schema.extensions.insert(key.clone(), value).is_some() {
                        return Err(de::Error::custom(format!("duplicate field `{key}`")));
                    }
                }
            }
        }
        Ok(schema)
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
/// `exclusiveMaximum`, which the model cannot hold) or its count keywords checked under
/// [`IntegerSemantics::Lexical`] (the model stores `2.0` as `2`, so the written form is gone);
/// strict parsing, and those cases, go through a [`Value`] so the dialect is known before the
/// model is built.
fn parse_raw(raw: RawSchema<'_>, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<Schema> {
    if !settings.disallow_unknown_fields {
        match raw.parse::<Schema>() {
            Ok(schema) => {
                let schema_uri: Option<&str> = schema.as_object().and_then(|s| s.schema.as_deref());
                let resolved: ResolvedDialect = resolve_dialect_for_uri(schema_uri, settings)?;
                if !needs_value_pass(raw, resolved.dialect) {
                    return Ok(finish_parse(schema, &resolved, settings));
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
                return parse_value_in_dialect(Cow::Owned(value), &resolved, settings);
            }
        }
    }
    let value: Value = raw.parse()?;
    parse_value(Cow::Owned(value), settings)
}

/// Parses a schema document held as a [`Value`], borrowed or owned. A borrowed document is
/// copied only when draft-4 shapes must be rewritten.
fn parse_value(
    value: Cow<'_, Value>,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<Schema> {
    let resolved: ResolvedDialect = resolve_dialect_for_value(&value, settings)?;
    parse_value_in_dialect(value, &resolved, settings)
}

/// Resolves the dialect of a document held as a [`Value`] from its `$schema` and `settings`.
fn resolve_dialect_for_value(
    value: &Value,
    settings: &JsonSchemaSettings,
) -> Result<ResolvedDialect, JsonSchemaParseError> {
    let schema_uri: Option<&str> = value.get("$schema").and_then(Value::as_str);
    resolve_dialect_for_uri(schema_uri, settings)
}

/// Checks the document's keywords (unknown keys when strict, count keywords' written form under
/// [`IntegerSemantics::Lexical`]), normalises its draft-4 shapes for `resolved`'s dialect and
/// builds the model. Keywords are checked before normalising, so a draft-4 document is held to
/// draft 4's keywords as written (`id`, not `$id`).
fn parse_value_in_dialect(
    mut value: Cow<'_, Value>,
    resolved: &ResolvedDialect,
    settings: &JsonSchemaSettings,
) -> JsonSchemaParseResult<Schema> {
    let checks: KeywordChecks = KeywordChecks {
        reject_unknown: settings.disallow_unknown_fields,
        lexical_counts: resolved.dialect.integer_semantics == IntegerSemantics::Lexical,
    };
    let errors: Vec<JsonSchemaParseError> = if checks.reject_unknown || checks.lexical_counts {
        check_keywords(&value, resolved.dialect, checks)
    } else {
        Vec::new()
    };
    if needs_draft4_normalization(resolved.dialect) {
        normalize_draft4_shapes(value.to_mut(), resolved.dialect);
    }
    // Built even when the walk found problems: a model error (a keyword's value of the wrong
    // type) is one more independent problem to report alongside them.
    let built: Result<Schema, serde_json::Error> = match value {
        Cow::Owned(value) => serde_json::from_value(value),
        Cow::Borrowed(value) => Schema::deserialize(value),
    };
    match (built, JsonSchemaParseErrors::from_vec(errors)) {
        (Ok(schema), None) => Ok(finish_parse(schema, resolved, settings)),
        (Ok(_), Some(found)) => Err(found),
        (Err(error), None) => Err(JsonSchemaParseError::Serde(error).into()),
        (Err(error), Some(mut found)) => {
            found.push(JsonSchemaParseError::Serde(error));
            Err(found)
        }
    }
}

/// The count keywords: their value is a non-negative integer.
const COUNT_KEYWORDS: [&str; 8] = [
    "maxContains",
    "maxItems",
    "maxLength",
    "maxProperties",
    "minContains",
    "minItems",
    "minLength",
    "minProperties",
];

/// Which checks [`check_keywords`] runs.
#[derive(Clone, Copy)]
struct KeywordChecks {
    /// Reject keys that are not keywords of the dialect (strict parsing).
    reject_unknown: bool,
    /// Reject count keywords written with a fraction or exponent (`2.0`, `2e0`), which are not
    /// integers under [`IntegerSemantics::Lexical`] (draft 4).
    lexical_counts: bool,
}

/// Checks the keys of every schema object and returns every problem found, one error each, in
/// walk order: depth-first, each object's keys in map order, an object's own keys before its
/// subschemas' keys. The same document always lists the same errors in the same order.
///
/// - with `reject_unknown`, each key that is not a keyword of `dialect` (its keyword table,
///   see [`Dialect::keywords`]) is a [`JsonSchemaParseError::UnknownField`];
/// - with `lexical_counts`, each count keyword whose number was written with a fraction or
///   exponent is a [`JsonSchemaParseError::InvalidKeywordValue`]. `serde_json` reads such a
///   number as a float and an integer literal as an integer, so the float form is the written
///   form.
///
/// Every schema object is checked: the root and each subschema the table's
/// [`SubschemaPosition`]s reach, including those under keywords the model does not hold (`not`,
/// `if`, ...). Keys in non-schema positions (property names, `enum` values, `default`) are data,
/// not keywords, and are not checked. Iterative.
fn check_keywords(
    root: &Value,
    dialect: Dialect,
    checks: KeywordChecks,
) -> Vec<JsonSchemaParseError> {
    let mut errors: Vec<JsonSchemaParseError> = Vec::new();
    // Each visited subschema records how it was reached (`steps`) instead of carrying its own
    // pointer, so the walk allocates no strings; the pointer is built only for an error.
    let mut steps: Vec<PathStep<'_>> = Vec::new();
    let mut stack: Vec<(&Value, Option<usize>)> = vec![(root, None)];
    let mut children: Vec<(&Value, Option<usize>)> = Vec::new();
    while let Some((node, at)) = stack.pop() {
        let Value::Object(map) = node else {
            continue;
        };
        for (key, child) in map {
            let Some(keyword) = dialect.keyword(key) else {
                if checks.reject_unknown {
                    errors.push(JsonSchemaParseError::UnknownField {
                        key: key.clone(),
                        path: path_to(&steps, at),
                    });
                }
                continue;
            };
            if checks.lexical_counts
                && COUNT_KEYWORDS.contains(&key.as_str())
                && let Value::Number(n) = child
                && n.is_f64()
            {
                errors.push(JsonSchemaParseError::InvalidKeywordValue {
                    keyword: key.clone(),
                    path: path_to(&steps, at),
                    value: child.clone(),
                    expected: "a non-negative integer written without a fraction or exponent",
                });
                continue;
            }
            let Some(position) = keyword.subschemas else {
                continue;
            };
            steps.push(PathStep {
                parent: at,
                segment: PathSegment::Key(key),
            });
            let keyword_at: usize = steps.len() - 1;
            match (position, child) {
                (
                    SubschemaPosition::Array | SubschemaPosition::SingleOrArray,
                    Value::Array(items),
                ) => {
                    for (index, item) in items.iter().enumerate() {
                        steps.push(PathStep {
                            parent: Some(keyword_at),
                            segment: PathSegment::Index(index),
                        });
                        children.push((item, Some(steps.len() - 1)));
                    }
                }
                (SubschemaPosition::Map, Value::Object(entries)) => {
                    for (name, entry) in entries {
                        steps.push(PathStep {
                            parent: Some(keyword_at),
                            segment: PathSegment::Key(name),
                        });
                        children.push((entry, Some(steps.len() - 1)));
                    }
                }
                (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, child) => {
                    children.push((child, Some(keyword_at)));
                }
                (SubschemaPosition::Array | SubschemaPosition::Map, _) => {}
            }
        }
        // Reversed so the stack visits subschemas in map order.
        stack.extend(children.drain(..).rev());
    }
    errors
}

/// One step of the strict key walk's path: a segment and the step it extends (`None` = root).
struct PathStep<'a> {
    parent: Option<usize>,
    segment: PathSegment<'a>,
}

/// An object key or array index in a [`PathStep`].
enum PathSegment<'a> {
    Key(&'a str),
    Index(usize),
}

/// The JSON Pointer of the subschema reached by step `at` (`None` = root).
fn path_to(steps: &[PathStep<'_>], at: Option<usize>) -> JsonPointer {
    let mut segments: Vec<String> = Vec::new();
    let mut current: Option<usize> = at;
    while let Some(index) = current {
        let step: &PathStep<'_> = &steps[index];
        segments.push(match step.segment {
            PathSegment::Key(key) => key.to_string(),
            PathSegment::Index(i) => i.to_string(),
        });
        current = step.parent;
    }
    segments.reverse();
    JsonPointer::from(segments)
}

/// Logs the dialect warnings and records the dialect settings on a root schema object when any
/// is set (a boolean schema means the same in every dialect).
fn finish_parse(
    mut schema: Schema,
    resolved: &ResolvedDialect,
    settings: &JsonSchemaSettings,
) -> Schema {
    for warning in &resolved.warnings {
        log::debug!("json-schema-rs: {warning}");
    }
    let dialect_settings: DialectSettings = settings.dialect_settings();
    if let Some(object) = schema.as_object_mut()
        && dialect_settings != DialectSettings::default()
    {
        object.dialect_settings = Some(dialect_settings);
    }
    schema
}

/// Whether `dialect` has draft-4 shapes that [`normalize_draft4_shapes`] rewrites.
fn needs_draft4_normalization(dialect: Dialect) -> bool {
    uses_legacy_id(dialect) || dialect.exclusive_bounds == ExclusiveBounds::BooleanModifier
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

/// Whether a lenient parse that succeeded directly must be redone through a [`Value`]: the
/// dialect reads draft 4's `id` and the text can contain an `id` key, or the dialect has
/// [`IntegerSemantics::Lexical`] and the text can contain a count keyword (the model stores
/// `2.0` as `2`, so only the `Value` still has the written form). A `\u` escape could spell
/// either key, so it always answers `true`; text that is not UTF-8 also answers `true` and is
/// left to the parser. Drafts 6 and later scan nothing; draft 4 scans the text at most three
/// times.
fn needs_value_pass(raw: RawSchema<'_>, dialect: Dialect) -> bool {
    let legacy_id: bool = uses_legacy_id(dialect);
    let lexical_counts: bool = dialect.integer_semantics == IntegerSemantics::Lexical;
    if !legacy_id && !lexical_counts {
        return false;
    }
    let text: &str = match raw {
        RawSchema::Str(s) => s,
        RawSchema::Slice(b) => match std::str::from_utf8(b) {
            Ok(s) => s,
            Err(_) => return true,
        },
    };
    let bytes: &[u8] = text.as_bytes();
    memmem::find(bytes, b"\\u").is_some()
        || (legacy_id && memmem::find(bytes, b"\"id\"").is_some())
        || (lexical_counts && contains_count_key(text))
}

/// Whether `text` has a quoted name from [`COUNT_KEYWORDS`] written literally. One SIMD pass over
/// the `"m` positions (every count keyword starts with `min` or `max`) instead of one scan per
/// keyword. A string value that spells a keyword also matches; the check on the `Value` sorts
/// that out.
fn contains_count_key(text: &str) -> bool {
    let bytes: &[u8] = text.as_bytes();
    memmem::find_iter(bytes, b"\"m").any(|quote| {
        let name: &[u8] = &bytes[quote + 1..];
        COUNT_KEYWORDS.iter().any(|keyword| {
            name.starts_with(keyword.as_bytes()) && name.get(keyword.len()) == Some(&b'"')
        })
    })
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

/// The schema object of a parsed document root, or [`JsonSchemaParseError::BooleanRoot`].
fn require_object(schema: Schema) -> JsonSchemaParseResult<JsonSchema> {
    match schema {
        Schema::Object(schema) => Ok(*schema),
        Schema::Bool(value) => Err(JsonSchemaParseError::BooleanRoot { value }.into()),
    }
}

impl JsonSchema {
    /// The effective [`Dialect`] of this schema as a document root, from its `$schema` and the
    /// [`dialect_settings`](JsonSchema::dialect_settings) it was parsed with (Draft 2020-12, or
    /// the default draft, when `$schema` is absent or unrecognised). A custom metaschema's
    /// `$vocabulary` and embedded resources' `$schema` are the [`Registry`](super::Registry)'s.
    #[must_use]
    pub fn dialect(&self) -> Dialect {
        let dialect_settings: DialectSettings = self.dialect_settings.unwrap_or_default();
        let settings: JsonSchemaSettings = JsonSchemaSettings {
            forced_spec_version: dialect_settings.forced_spec_version,
            default_spec_version: dialect_settings.default_spec_version,
            format_mode: dialect_settings.format_mode,
            integer_semantics: dialect_settings.integer_semantics,
            exclusive_bounds: dialect_settings.exclusive_bounds,
            ref_siblings: dialect_settings.ref_siblings,
            ..JsonSchemaSettings::default()
        };
        resolve_dialect_for_uri(self.schema.as_deref(), &settings)
            .map(|resolved| resolved.dialect)
            .unwrap_or_default()
    }

    /// Returns mutable references to this schema's direct subschema objects (boolean schemas
    /// have none), in a fixed order: `$defs`, `definitions`, `properties`,
    /// `additionalProperties`, `items`, `allOf`, `anyOf`, `oneOf`. Walk the whole tree with an
    /// explicit stack over this.
    pub(crate) fn subschemas_mut(&mut self) -> Vec<&mut JsonSchema> {
        let mut children: Vec<&mut Schema> = Vec::new();
        if let Some(defs) = self.defs.as_mut() {
            children.extend(defs.values_mut());
        }
        if let Some(definitions) = self.definitions.as_mut() {
            children.extend(definitions.values_mut());
        }
        children.extend(self.properties.values_mut());
        children.extend(self.additional_properties.as_mut());
        children.extend(self.items.as_mut());
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
            .into_iter()
            .filter_map(Schema::as_object_mut)
            .collect()
    }

    /// Parse a JSON Schema from a string with the given settings.
    ///
    /// When [`JsonSchemaSettings::disallow_unknown_fields`] is `false`, unknown keys
    /// are ignored (lenient). When `true`, a key the document's draft does not define as a
    /// keyword, in any schema object, causes an error (strict).
    ///
    /// # Errors
    ///
    /// Returns every problem found, one [`JsonSchemaParseError`] each, in walk order (see
    /// [`JsonSchemaParseErrors`]):
    ///
    /// - [`JsonSchemaParseError::UnknownField`] for each key the draft does not define as a
    ///   keyword, when strict;
    /// - [`JsonSchemaParseError::InvalidKeywordValue`] for each count keyword written with a
    ///   fraction or exponent under [`IntegerSemantics::Lexical`] (draft 4);
    /// - [`JsonSchemaParseError::Serde`] on invalid JSON, a type mismatch, or an invalid keyword
    ///   value (e.g. `"minLength": 2.5`). The model reports only its first such error, after
    ///   any of the above; invalid JSON is the only error;
    /// - [`JsonSchemaParseError::UnknownSchemaUri`] alone, when `$schema` is unrecognised under
    ///   [`UnknownSchemaUri::Reject`](super::UnknownSchemaUri::Reject);
    /// - [`JsonSchemaParseError::BooleanRoot`] alone, when the document is a boolean schema
    ///   (`true` / `false`), which a [`JsonSchema`] cannot hold; parse it with
    ///   [`Schema::new_from_str`] instead.
    pub fn new_from_str(json: &str, settings: &JsonSchemaSettings) -> JsonSchemaParseResult<Self> {
        parse_raw(RawSchema::Str(json), settings).and_then(require_object)
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
        parse_raw(RawSchema::Slice(slice), settings).and_then(require_object)
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
        parse_value(Cow::Borrowed(value), settings).and_then(require_object)
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
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value, &settings)
    }
}

/// Parses a JSON Schema from an owned string using default settings.
/// For custom settings, use [`JsonSchema::new_from_str`](Self::new_from_str).
impl TryFrom<String> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_str(value.as_str(), &settings)
    }
}

/// Parses a JSON Schema from a byte slice using default settings.
/// For custom settings, use [`JsonSchema::new_from_slice`](Self::new_from_slice).
impl TryFrom<&[u8]> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_slice(value, &settings)
    }
}

/// Parses a JSON Schema from an already-parsed [`serde_json::Value`] using default settings.
/// For custom settings, use [`JsonSchema::new_from_serde_value`](Self::new_from_serde_value).
impl TryFrom<&serde_json::Value> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_serde_value(value, &settings)
    }
}

/// Parses a JSON Schema from an open file using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_reader`](Self::new_from_reader).
impl TryFrom<std::fs::File> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: std::fs::File) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_reader(value, &settings)
    }
}

/// Parses a JSON Schema from a file path using default settings. I/O errors (e.g. file not found) are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<&std::path::Path> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(value, &settings)
    }
}

/// Parses a JSON Schema from an owned path using default settings. I/O errors are reported as [`JsonSchemaParseError::Io`].
/// For custom settings, use [`JsonSchema::new_from_path`](Self::new_from_path).
impl TryFrom<std::path::PathBuf> for JsonSchema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: std::path::PathBuf) -> Result<Self, Self::Error> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        JsonSchema::new_from_path(&value, &settings)
    }
}

/// Parses a JSON Schema document (object or boolean) from a string using default settings.
impl TryFrom<&str> for Schema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Schema::new_from_str(value, &JsonSchemaSettings::default())
    }
}

/// Parses a JSON Schema document (object or boolean) from a byte slice using default settings.
impl TryFrom<&[u8]> for Schema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Schema::new_from_slice(value, &JsonSchemaSettings::default())
    }
}

/// Parses a JSON Schema document (object or boolean) from a [`serde_json::Value`] using default
/// settings.
impl TryFrom<&serde_json::Value> for Schema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &serde_json::Value) -> Result<Self, Self::Error> {
        Schema::new_from_serde_value(value, &JsonSchemaSettings::default())
    }
}

/// Parses a JSON Schema document (object or boolean) from a file path using default settings.
impl TryFrom<&std::path::Path> for Schema {
    type Error = JsonSchemaParseErrors;

    fn try_from(value: &std::path::Path) -> Result<Self, Self::Error> {
        Schema::new_from_path(value, &JsonSchemaSettings::default())
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
    use super::{JsonSchema, Schema};
    use crate::json_schema::{
        Dialect, DialectSettings, ExclusiveBounds, FormatMode, IntegerSemantics,
        JsonSchemaParseError, JsonSchemaParseErrors, JsonSchemaSettings, JsonType, RefSiblings,
        SpecVersion, TypeSet, UnknownSchemaUri,
    };
    use std::collections::BTreeMap;
    use std::io;

    #[test]
    fn try_from_schema_to_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
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
            type_: Some(JsonType::Object.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_examples_empty_array() {
        let json = r#"{"type":"string","examples":[]}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::Object.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
        let mut properties: BTreeMap<String, Schema> = BTreeMap::new();
        properties.insert(
            "x".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                deprecated: Some(true),
                ..Default::default()
            }),
        );
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
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
        let expected_type: Option<TypeSet> = Some(JsonType::Object.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_str_invalid_json() {
        let json: &str = "not json";
        let result: Result<JsonSchema, JsonSchemaParseErrors> = JsonSchema::try_from(json);
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
    }

    #[test]
    fn try_from_string_success() {
        let json: String = r#"{"type":"string"}"#.to_string();
        let actual: JsonSchema = JsonSchema::try_from(json).expect("try_from");
        let expected_type: Option<TypeSet> = Some(JsonType::String.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_slice_success() {
        let bytes: &[u8] = r#"{"type":"array","items":{"type":"string"}}"#.as_bytes();
        let actual: JsonSchema = JsonSchema::try_from(bytes).expect("try_from");
        let expected_type: Option<TypeSet> = Some(JsonType::Array.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_serde_value_success() {
        let value: serde_json::Value =
            serde_json::json!({"type": "object", "properties": {"a": {"type": "integer"}}});
        let actual: JsonSchema = JsonSchema::try_from(&value).expect("try_from");
        let expected_type: Option<TypeSet> = Some(JsonType::Object.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseErrors> = JsonSchema::try_from(&value);
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
        let expected_type: Option<TypeSet> = Some(JsonType::Object.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_file_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let f: std::fs::File = std::fs::File::open(&schema_path).expect("open");
        let result: Result<JsonSchema, JsonSchemaParseErrors> = JsonSchema::try_from(f);
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
    }

    #[test]
    fn try_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("try_from");
        let expected_type: Option<TypeSet> = Some(JsonType::Array.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path: std::path::PathBuf = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Io(_)])
        ));
    }

    #[test]
    fn try_from_path_buf_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path: std::path::PathBuf = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"number"}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path).expect("try_from");
        let expected_type: Option<TypeSet> = Some(JsonType::Number.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn try_from_schema_integer_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
            ..Default::default()
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"integer\"}";
        assert_eq!(expected, actual.as_slice());
    }

    #[test]
    fn try_from_schema_number_to_vec_u8() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
            ..Default::default()
        };
        let actual: Vec<u8> = schema.try_into().expect("serialize");
        let expected: &[u8] = b"{\"type\":\"number\"}";
        assert_eq!(expected, actual.as_slice());
    }

    /// The primary type of a schema whose `type` is `type_json` (`null` = keyword absent).

    #[test]
    fn try_from_schema_array_with_items_to_string() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(Schema::from(item_schema)),
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
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    Schema::from(JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }),
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
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
        );
        let mut props_b = BTreeMap::new();
        props_b.insert(
            "b".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Integer.into()),
                ..Default::default()
            }),
        );
        let expected: Option<Vec<Schema>> = Some(vec![
            Schema::from(JsonSchema {
                type_: Some(JsonType::Object.into()),
                properties: props_a,
                ..Default::default()
            }),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Object.into()),
                properties: props_b,
                ..Default::default()
            }),
        ]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = None;
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_all_of_empty_array() {
        let json = r#"{"allOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = Some(vec![]);
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
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
        );
        let expected: Option<Vec<Schema>> = Some(vec![Schema::from(JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: props,
            ..Default::default()
        })]);
        let actual = parsed.all_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_present() {
        let json = r#"{"anyOf":[{"type":"string"},{"type":"integer"}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = Some(vec![
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Integer.into()),
                ..Default::default()
            }),
        ]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = None;
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_any_of_empty_array() {
        let json = r#"{"anyOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = Some(vec![]);
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
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
        );
        let expected: Option<Vec<Schema>> = Some(vec![Schema::from(JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: props,
            ..Default::default()
        })]);
        let actual = parsed.any_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_present() {
        let json = r#"{"oneOf":[{"type":"string"},{"type":"integer"}]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = Some(vec![
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Integer.into()),
                ..Default::default()
            }),
        ]);
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_absent() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = None;
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_one_of_empty_array() {
        let json = r#"{"oneOf":[]}"#;
        let parsed = JsonSchema::try_from(json).expect("parse");
        let expected: Option<Vec<Schema>> = Some(vec![]);
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
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
        );
        let expected: Option<Vec<Schema>> = Some(vec![Schema::from(JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: props,
            ..Default::default()
        })]);
        let actual = parsed.one_of.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_with_required() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"},"y":{"type":"string"}},"required":["x"]}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "x".to_string(),
                    Schema::from(JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }),
                );
                m.insert(
                    "y".to_string(),
                    Schema::from(JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }),
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
    fn deserialize_keeps_unknown_keys_in_extensions() {
        let json =
            r#"{"type":"object","properties":{},"$schema":"https://example.com","unknown":42}"#;
        let expected: JsonSchema = JsonSchema {
            schema: Some("https://example.com".to_string()),
            type_: Some(JsonType::Object.into()),
            extensions: BTreeMap::from([("unknown".to_string(), serde_json::json!(42))]),
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_type_array_keeps_every_entry() {
        let json = r#"{"type":["string", "null"],"properties":{}}"#;
        let expected: JsonSchema = JsonSchema {
            type_: Some([JsonType::String, JsonType::Null].into_iter().collect()),
            ..Default::default()
        };
        let actual: JsonSchema = serde_json::from_str(json).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_array_with_items() {
        let json = r#"{"type":"array","items":{"type":"string"}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        assert_eq!(expected, actual);
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
        let expected: StrictOutcome = unknown("unknown", "");
        let actual: StrictOutcome = strict_outcome(JsonSchema::new_from_str(json, &settings));
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_strict_rejects_unknown_key_in_nested_properties() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let json = r#"{"type":"object","properties":{"nested":{"type":"object","properties":{},"bad":1}}}"#;
        let expected: StrictOutcome = unknown("bad", "/properties/nested");
        let actual: StrictOutcome = strict_outcome(JsonSchema::new_from_str(json, &settings));
        assert_eq!(expected, actual);
    }

    /// A strict parse's outcome: `Ok` on success, or every error as `(key, path)` for each
    /// [`JsonSchemaParseError::UnknownField`], in the order reported. Any other error panics,
    /// failing the test.
    type StrictOutcome = Result<(), Vec<(String, String)>>;

    fn strict_outcome(result: Result<JsonSchema, JsonSchemaParseErrors>) -> StrictOutcome {
        match result {
            Ok(_) => Ok(()),
            Err(errors) => Err(errors
                .into_iter()
                .map(|error| match error {
                    JsonSchemaParseError::UnknownField { key, path } => {
                        (key, path.as_str().to_string())
                    }
                    other => panic!("expected UnknownField, got {other:?}"),
                })
                .collect()),
        }
    }

    fn strict_parse(json: &str) -> StrictOutcome {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        strict_outcome(JsonSchema::new_from_str(json, &settings))
    }

    fn unknown(key: &str, path: &str) -> StrictOutcome {
        unknowns(&[(key, path)])
    }

    fn unknowns(keys: &[(&str, &str)]) -> StrictOutcome {
        Err(keys
            .iter()
            .map(|(key, path)| ((*key).to_string(), (*path).to_string()))
            .collect())
    }

    #[test]
    fn strict_reports_unknown_key_with_schema_path() {
        let expected: StrictOutcome = unknown("bogus", "/properties/a");
        let actual: StrictOutcome =
            strict_parse(r#"{"type":"object","properties":{"a":{"type":"string","bogus":1}}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_in_defs() {
        let expected: StrictOutcome = unknown("bogus", "/$defs/a");
        let actual: StrictOutcome = strict_parse(r#"{"$defs":{"a":{"bogus":1}}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_in_all_of_entry() {
        let expected: StrictOutcome = unknown("bogus", "/allOf/1");
        let actual: StrictOutcome = strict_parse(r#"{"allOf":[{},{"bogus":1}]}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_in_items() {
        let expected: StrictOutcome = unknown("bogus", "/items");
        let actual: StrictOutcome = strict_parse(r#"{"items":{"bogus":1}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_in_draft7_items_array() {
        let expected: Result<(), Vec<String>> =
            Err(vec!["unknown keyword \"bogus\" at /items/1".to_string()]);
        let actual: Result<(), Vec<String>> = strict_error_messages(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","items":[{},{"bogus":1}]}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_in_additional_properties() {
        let expected: StrictOutcome = unknown("bogus", "/additionalProperties");
        let actual: StrictOutcome = strict_parse(r#"{"additionalProperties":{"bogus":1}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_key_under_keyword_the_model_does_not_hold() {
        let expected: StrictOutcome = unknown("bogus", "/not/properties/x");
        let actual: StrictOutcome = strict_parse(r#"{"not":{"properties":{"x":{"bogus":1}}}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_escapes_property_name_in_path() {
        let expected: StrictOutcome = unknown("bogus", "/properties/a~1b~0c");
        let actual: StrictOutcome = strict_parse(r#"{"properties":{"a/b~c":{"bogus":1}}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_root_key_before_nested_key() {
        let expected: StrictOutcome = unknowns(&[("zzz", ""), ("bogus", "/properties/a")]);
        let actual: StrictOutcome = strict_parse(r#"{"properties":{"a":{"bogus":1}},"zzz":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_every_unknown_key_at_root_in_map_order() {
        let expected: StrictOutcome = unknowns(&[("alpha", ""), ("mid", ""), ("zeta", "")]);
        let actual: StrictOutcome = strict_parse(r#"{"zeta":1,"type":"string","alpha":2,"mid":3}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_keys_in_sibling_subschemas_in_walk_order() {
        let expected: StrictOutcome = unknowns(&[
            ("b", "/allOf/0"),
            ("a", "/allOf/1"),
            ("c", "/properties/x"),
            ("d", "/properties/y"),
        ]);
        let actual: StrictOutcome =
            strict_parse(r#"{"properties":{"x":{"c":1},"y":{"d":1}},"allOf":[{"b":1},{"a":1}]}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_unknown_keys_in_nested_subschemas_depth_first() {
        let expected: StrictOutcome = unknowns(&[
            ("a", "/allOf/0"),
            ("b", "/allOf/0/not"),
            ("c", "/allOf/0/not/items"),
            ("d", "/allOf/1"),
        ]);
        let actual: StrictOutcome =
            strict_parse(r#"{"allOf":[{"a":1,"not":{"b":1,"items":{"c":1}}},{"d":1}]}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_parses_document_with_no_problems() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"type":"object","properties":{"a":{"type":"string","minLength":1}},"allOf":[{"required":["a"]}]}"#,
        );
        assert_eq!(expected, actual);
    }

    /// Every error a strict parse of `json` reports, rendered with `Display`, in order.
    fn strict_error_messages(json: &str) -> Result<(), Vec<String>> {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        JsonSchema::new_from_str(json, &settings)
            .map(|_| ())
            .map_err(|errors| errors.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn strict_reports_unknown_keys_mixed_with_invalid_counts_in_walk_order() {
        let expected: Result<(), Vec<String>> = Err(vec![
            "invalid \"minLength\" value 2.0 at root: expected a non-negative integer written without a fraction or exponent".to_string(),
            "unknown keyword \"zzz\" at root".to_string(),
            "unknown keyword \"bogus\" at /properties/a".to_string(),
            "invalid \"maxItems\" value 3.0 at /properties/a: expected a non-negative integer written without a fraction or exponent".to_string(),
        ]);
        let actual: Result<(), Vec<String>> = strict_error_messages(
            r#"{"$schema":"http://json-schema.org/draft-04/schema#","minLength":2.0,"zzz":1,"properties":{"a":{"maxItems":3.0,"bogus":true}}}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_model_error_after_unknown_keys() {
        let expected: Result<(), Vec<String>> = Err(vec![
            "unknown keyword \"bogus\" at root".to_string(),
            "unknown keyword \"extra\" at /items".to_string(),
            "invalid JSON Schema: invalid type: string \"x\", expected a JSON number".to_string(),
        ]);
        let actual: Result<(), Vec<String>> =
            strict_error_messages(r#"{"bogus":1,"minLength":"x","items":{"extra":1}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_reports_only_model_error_when_keys_are_known() {
        let expected: Result<(), Vec<String>> = Err(vec![
            "invalid JSON Schema: invalid type: string \"x\", expected a JSON number".to_string(),
        ]);
        let actual: Result<(), Vec<String>> = strict_error_messages(r#"{"minLength":"x"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_errors_display_one_line_per_error() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: &str = "unknown keyword \"bogus\" at root\n\
            invalid \"minLength\" value 2.0 at root: expected a non-negative integer written without a fraction or exponent\n\
            unknown keyword \"extra\" at /properties/a";
        let actual: String = JsonSchema::new_from_str(
            r#"{"$schema":"http://json-schema.org/draft-04/schema#","bogus":1,"minLength":2.0,"properties":{"a":{"extra":true}}}"#,
            &settings,
        )
        .expect_err("three problems")
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_accepts_spec_keywords_the_model_does_not_hold() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"not":{},"if":{},"then":{},"else":{},"patternProperties":{"^a":{}},"propertyNames":{},"contains":{},"minContains":1,"dependentRequired":{"a":["b"]},"unevaluatedProperties":false,"$anchor":"x"}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_does_not_check_property_names() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(r#"{"properties":{"bogus":{"type":"string"}}}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_does_not_check_keys_inside_keyword_values() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"enum":[{"bogus":1}],"const":{"bogus":1},"default":{"bogus":1},"examples":[{"bogus":1}]}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_skips_string_array_entries_of_dependencies() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","dependencies":{"a":["b"],"c":{"required":["d"]}}}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_rejects_defs_in_draft7() {
        let expected: StrictOutcome = unknown("$defs", "");
        let actual: StrictOutcome = strict_parse(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","$defs":{"a":{}}}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_accepts_defs_in_2019_09() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","$defs":{"a":{}}}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_rejects_defs_when_settings_pin_draft7() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .forced_spec_version(SpecVersion::Draft07)
            .build();
        let expected: StrictOutcome = unknown("$defs", "");
        let actual: StrictOutcome =
            strict_outcome(JsonSchema::new_from_str(r#"{"$defs":{"a":{}}}"#, &settings));
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_rejects_dollar_id_in_draft4() {
        let expected: StrictOutcome = unknown("$id", "");
        let actual: StrictOutcome = strict_parse(
            r#"{"$schema":"http://json-schema.org/draft-04/schema#","$id":"http://example.com/x.json"}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_rejects_additional_items_in_2020_12() {
        let expected: StrictOutcome = unknown("additionalItems", "");
        let actual: StrictOutcome = strict_parse(r#"{"additionalItems":false}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_accepts_additional_items_in_2019_09() {
        let expected: StrictOutcome = Ok(());
        let actual: StrictOutcome = strict_parse(
            r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","additionalItems":false}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_from_serde_value_reports_nested_unknown_key() {
        let value: serde_json::Value = serde_json::json!({"anyOf": [{"bogus": 1}]});
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: StrictOutcome = unknown("bogus", "/anyOf/0");
        let actual: StrictOutcome =
            strict_outcome(JsonSchema::new_from_serde_value(&value, &settings));
        assert_eq!(expected, actual);
    }

    #[test]
    fn lenient_keeps_unknown_keys_everywhere() {
        let expected: serde_json::Value =
            serde_json::json!({"bogus": 1, "properties": {"a": {"type": "string", "bogus": 1}}});
        let parsed: JsonSchema = JsonSchema::try_from(&expected).expect("parse");
        let actual: serde_json::Value = serde_json::to_value(&parsed).expect("serialize");
        assert_eq!(expected, actual);
    }

    // --- extensions: every key the model does not hold ---

    /// Parses `json` with default settings and serializes the result back to a `Value`.
    fn round_trip(json: &str) -> serde_json::Value {
        let schema: Schema = Schema::try_from(json).expect("parse");
        serde_json::to_value(&schema).expect("serialize")
    }

    #[test]
    fn extensions_hold_keywords_the_model_does_not() {
        let expected: BTreeMap<String, serde_json::Value> = BTreeMap::from([
            ("if".to_string(), serde_json::json!({"type": "string"})),
            ("not".to_string(), serde_json::json!({"type": "null"})),
            ("x-rust-type".to_string(), serde_json::json!({"crate": "a"})),
        ]);
        let actual: BTreeMap<String, serde_json::Value> = JsonSchema::try_from(
            r#"{"type":"string","not":{"type":"null"},"if":{"type":"string"},"x-rust-type":{"crate":"a"}}"#,
        )
        .expect("parse")
        .extensions;
        assert_eq!(expected, actual);
    }

    #[test]
    fn extensions_round_trip_unknown_and_x_keywords() {
        let expected: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {"a": {"x-foo": [1, 2], "minLength": 1}},
            "patternProperties": {"^x": {"type": "string"}},
            "not": {"type": "null"},
            "x-bar": null
        });
        let actual: serde_json::Value = round_trip(&expected.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn extensions_serialize_after_fields_in_key_order() {
        let expected: &str = r#"{"type":"string","a":1,"z":2}"#;
        let schema: JsonSchema =
            JsonSchema::try_from(r#"{"z":2,"type":"string","a":1}"#).expect("parse");
        let actual: String = String::try_from(&schema).expect("serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_parse_keeps_known_keywords_in_extensions() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("not".to_string(), serde_json::json!({"type": "null"}))]);
        let actual: BTreeMap<String, serde_json::Value> =
            JsonSchema::new_from_str(r#"{"not":{"type":"null"}}"#, &settings)
                .expect("parse")
                .extensions;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_non_string_id_stays_in_extensions() {
        let expected: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("id".to_string(), serde_json::json!(5))]);
        let actual: BTreeMap<String, serde_json::Value> =
            JsonSchema::try_from(r#"{"$schema":"http://json-schema.org/draft-04/schema#","id":5}"#)
                .expect("parse")
                .extensions;
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_id_stays_in_extensions() {
        let expected: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("id".to_string(), serde_json::json!("x"))]);
        let actual: BTreeMap<String, serde_json::Value> = JsonSchema::try_from(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","id":"x"}"#,
        )
        .expect("parse")
        .extensions;
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_field_is_an_error() {
        let expected: String =
            "invalid JSON Schema: duplicate field `title` at line 1 column 20".to_string();
        let actual: String = JsonSchema::try_from(r#"{"title":"a","title":"b"}"#)
            .expect_err("duplicate")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_field_after_null_is_an_error() {
        let expected: String =
            "invalid JSON Schema: duplicate field `title` at line 1 column 21".to_string();
        let actual: String = JsonSchema::try_from(r#"{"title":null,"title":"b"}"#)
            .expect_err("duplicate")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_extension_is_an_error() {
        let expected: String =
            "invalid JSON Schema: duplicate field `x-a` at line 1 column 17".to_string();
        let actual: String = JsonSchema::try_from(r#"{"x-a":1,"x-a":2}"#)
            .expect_err("duplicate")
            .to_string();
        assert_eq!(expected, actual);
    }

    // --- type: a set of JSON types ---

    #[test]
    fn parse_type_array_round_trips_in_name_order() {
        let expected: serde_json::Value =
            serde_json::json!({"type": ["integer", "null", "string"]});
        let actual: serde_json::Value = round_trip(r#"{"type":["string","null","integer"]}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_type_null() {
        let expected: Option<TypeSet> = Some(JsonType::Null.into());
        let actual: Option<TypeSet> = JsonSchema::try_from(r#"{"type":"null"}"#)
            .expect("parse")
            .type_;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_type_empty_array_is_empty_set() {
        let expected: Option<TypeSet> = Some(TypeSet::empty());
        let actual: Option<TypeSet> = JsonSchema::try_from(r#"{"type":[]}"#).expect("parse").type_;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unknown_type_name_is_an_error() {
        let expected: String = "invalid JSON Schema: unknown type \"strng\", expected one of \
                                \"array\", \"boolean\", \"integer\", \"null\", \"number\", \
                                \"object\", \"string\" at line 1 column 15"
            .to_string();
        let actual: String = JsonSchema::try_from(r#"{"type":"strng"}"#)
            .expect_err("unknown type")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unknown_type_name_in_nested_schema_is_an_error() {
        let expected: String = "invalid JSON Schema: unknown type \"any\", expected one of \
                                \"array\", \"boolean\", \"integer\", \"null\", \"number\", \
                                \"object\", \"string\" at line 1 column 33"
            .to_string();
        let actual: String = JsonSchema::try_from(r#"{"properties":{"a":{"type":["any"]}}}"#)
            .expect_err("unknown type")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unknown_type_name_from_value_is_an_error() {
        let expected: String = "invalid JSON Schema: unknown type \"strng\", expected one of \
                                \"array\", \"boolean\", \"integer\", \"null\", \"number\", \
                                \"object\", \"string\""
            .to_string();
        let actual: String = JsonSchema::try_from(&serde_json::json!({"type": "strng"}))
            .expect_err("unknown type")
            .to_string();
        assert_eq!(expected, actual);
    }

    // --- boolean schemas ---

    #[test]
    fn schema_document_true() {
        let expected: Schema = Schema::Bool(true);
        let actual: Schema = Schema::try_from("true").expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_document_false_from_value() {
        let expected: Schema = Schema::Bool(false);
        let actual: Schema = Schema::try_from(&serde_json::json!(false)).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_document_false_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: Schema = Schema::Bool(false);
        let actual: Schema = Schema::new_from_str("false", &settings).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_document_object() {
        let expected: Schema = Schema::from(JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        });
        let actual: Schema = Schema::try_from(r#"{"type":"string"}"#).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_document_records_forced_spec_version() {
        let settings: JsonSchemaSettings = SpecVersion::Draft07.default_schema_settings();
        let expected: Option<DialectSettings> = Some(DialectSettings {
            forced_spec_version: Some(SpecVersion::Draft07),
            ..DialectSettings::default()
        });
        let actual: Option<DialectSettings> = Schema::new_from_str("{}", &settings)
            .expect("parse")
            .as_object()
            .expect("object")
            .dialect_settings;
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_schema_document_boolean_is_an_error() {
        let expected: String =
            "the document is the boolean schema `true`, not a schema object".to_string();
        let actual: String = JsonSchema::try_from("true")
            .expect_err("boolean root")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_document_number_is_an_error() {
        let expected: String = "invalid JSON Schema: invalid type: integer `1`, expected a schema \
                                (an object or a boolean) at line 1 column 1"
            .to_string();
        let actual: String = Schema::try_from("1").expect_err("number").to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_schemas_at_every_position_round_trip() {
        let expected: serde_json::Value = serde_json::json!({
            "$defs": {"t": true, "f": false},
            "definitions": {"t": true},
            "properties": {"a": true, "b": false},
            "additionalProperties": false,
            "items": true,
            "allOf": [true],
            "anyOf": [false, {"type": "string"}],
            "oneOf": [true, false]
        });
        let actual: serde_json::Value = round_trip(&expected.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_property_schema_parses_as_bool() {
        let expected: BTreeMap<String, Schema> = BTreeMap::from([
            ("a".to_string(), Schema::Bool(true)),
            ("b".to_string(), Schema::Bool(false)),
        ]);
        let actual: BTreeMap<String, Schema> =
            JsonSchema::try_from(r#"{"properties":{"a":true,"b":false}}"#)
                .expect("parse")
                .properties;
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_items_parses_as_bool() {
        let expected: Option<Schema> = Some(Schema::Bool(false));
        let actual: Option<Schema> = JsonSchema::try_from(r#"{"items":false}"#)
            .expect("parse")
            .items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_null_is_absent() {
        let expected: Option<Schema> = None;
        let actual: Option<Schema> = JsonSchema::try_from(r#"{"additionalProperties":null}"#)
            .expect("parse")
            .additional_properties;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_where_schema_expected_names_the_shape() {
        let expected: String = "invalid JSON Schema: invalid type: sequence, expected a schema \
                                (an object or a boolean) at line 1 column 25"
            .to_string();
        let actual: String = JsonSchema::try_from(r#"{"additionalProperties":[{}]}"#)
            .expect_err("array additionalProperties")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn tuple_items_are_kept_in_extensions() {
        let expected: JsonSchema = JsonSchema {
            extensions: BTreeMap::from([("items".to_string(), serde_json::json!([{}, true]))]),
            ..Default::default()
        };
        let actual: JsonSchema = JsonSchema::try_from(r#"{"items":[{},true]}"#).expect("parse");
        assert_eq!(expected, actual);
    }

    #[test]
    fn tuple_items_round_trip() {
        let expected: String = r#"{"items":[{"type":"string"}]}"#.to_string();
        let actual: String = serde_json::to_string(
            &JsonSchema::try_from(r#"{"items":[{"type":"string"}]}"#).expect("parse"),
        )
        .expect("serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn string_where_schema_expected_names_the_shape() {
        let expected: String = "invalid JSON Schema: invalid type: string \"x\", expected a \
                                schema (an object or a boolean) at line 1 column 22"
            .to_string();
        let actual: String = JsonSchema::try_from(r#"{"properties":{"a":"x"}}"#)
            .expect_err("string schema")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_schema_deserialize_rejects_boolean() {
        let expected: String = "invalid type: boolean `true`, expected a schema object".to_string();
        let actual: String = serde_json::from_value::<JsonSchema>(serde_json::json!(true))
            .expect_err("boolean")
            .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn subschemas_mut_skips_boolean_schemas() {
        let mut schema: JsonSchema = JsonSchema::try_from(
            r#"{"properties":{"a":true,"b":{"title":"B"}},"items":false,"allOf":[{"title":"C"}]}"#,
        )
        .expect("parse");
        let expected: Vec<Option<String>> = vec![Some("B".to_string()), Some("C".to_string())];
        let actual: Vec<Option<String>> = schema
            .subschemas_mut()
            .into_iter()
            .map(|s| s.title.clone())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn object_or_empty_reads_boolean_as_empty_object() {
        let expected: &JsonSchema = &JsonSchema::default();
        let actual: &JsonSchema = Schema::Bool(false).object_or_empty();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_field_display_names_root() {
        let expected: &str = "unknown keyword \"bogus\" at root";
        let actual: String = JsonSchema::new_from_str(
            r#"{"bogus":1}"#,
            &JsonSchemaSettings::builder()
                .disallow_unknown_fields(true)
                .build(),
        )
        .expect_err("strict parse fails")
        .to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_field_display_names_path() {
        let expected: &str = "unknown keyword \"bogus\" at /properties/a";
        let actual: String = JsonSchema::new_from_str(
            r#"{"properties":{"a":{"bogus":1}}}"#,
            &JsonSchemaSettings::builder()
                .disallow_unknown_fields(true)
                .build(),
        )
        .expect_err("strict parse fails")
        .to_string();
        assert_eq!(expected, actual);
    }

    /// Parses `json` with default settings, mapping the error to its message.
    fn parse_message(json: &str) -> Result<JsonSchema, String> {
        JsonSchema::try_from(json).map_err(|e| e.to_string())
    }

    #[test]
    fn null_properties_parse_as_no_properties() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"type":"object"}"#);
        let actual: Result<JsonSchema, String> =
            parse_message(r#"{"type":"object","properties":null}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_length_integer_valued_decimal_parses_as_integer() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"minLength":2}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minLength":2.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_length_integer_valued_decimal_parses_as_integer() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"maxLength":2}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"maxLength":2.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_integer_valued_decimal_parses_as_integer() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"minItems":2}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minItems":2.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_integer_valued_decimal_parses_as_integer() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"maxItems":2}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"maxItems":2.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_exponent_form_parses_as_integer() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"minLength":100}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minLength":1e2}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_negative_zero_parses_as_zero() {
        let expected: Result<JsonSchema, String> = parse_message(r#"{"minLength":0}"#);
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minLength":-0.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_accepts_u64_max() {
        let expected: Result<Option<u64>, String> = Ok(Some(u64::MAX));
        let actual: Result<Option<u64>, String> =
            parse_message(r#"{"maxLength":18446744073709551615}"#).map(|s| s.max_length);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_integer_valued_decimal_parses_when_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: Result<Option<u64>, String> = Ok(Some(2));
        let actual: Result<Option<u64>, String> =
            JsonSchema::new_from_str(r#"{"minItems":2.0}"#, &settings)
                .map(|s| s.min_items)
                .map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_integer_valued_decimal_parses_from_serde_value() {
        let value: serde_json::Value = serde_json::json!({"maxItems": 3.0});
        let expected: Result<Option<u64>, String> = Ok(Some(3));
        let actual: Result<Option<u64>, String> = JsonSchema::try_from(&value)
            .map(|s| s.max_items)
            .map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_integer_valued_decimal_serializes_as_integer() {
        let expected: String = r#"{"minLength":2}"#.to_string();
        let actual: String =
            String::try_from(&JsonSchema::try_from(r#"{"minLength":2.0}"#).expect("parse"))
                .expect("serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_rejects_fraction() {
        let expected: Result<JsonSchema, String> = Err(
            "invalid JSON Schema: expected a non-negative integer, got 2.5 at line 1 column 17"
                .to_string(),
        );
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minLength":2.5}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_rejects_negative_integer() {
        let expected: Result<JsonSchema, String> = Err(
            "invalid JSON Schema: expected a non-negative integer, got -1 at line 1 column 15"
                .to_string(),
        );
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minItems":-1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_rejects_negative_decimal() {
        let expected: Result<JsonSchema, String> = Err(
            "invalid JSON Schema: expected a non-negative integer, got -2.0 at line 1 column 17"
                .to_string(),
        );
        let actual: Result<JsonSchema, String> = parse_message(r#"{"maxItems":-2.0}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_rejects_value_beyond_u64() {
        let expected: Result<JsonSchema, String> = Err(
            "invalid JSON Schema: expected a non-negative integer, got 1.8446744073709552e+19 at line 1 column 34"
                .to_string(),
        );
        let actual: Result<JsonSchema, String> =
            parse_message(r#"{"maxLength":18446744073709551616}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_rejects_string() {
        let expected: Result<JsonSchema, String> = Err(
            "invalid JSON Schema: invalid type: string \"2\", expected a JSON number at line 1 column 16"
                .to_string(),
        );
        let actual: Result<JsonSchema, String> = parse_message(r#"{"minLength":"2"}"#);
        assert_eq!(expected, actual);
    }

    const DRAFT4_SCHEMA: &str = "http://json-schema.org/draft-04/schema#";

    /// A parse's outcome: `Ok` on success, or every error as `(keyword, path, value)` for each
    /// [`JsonSchemaParseError::InvalidKeywordValue`], in the order reported. Any other error
    /// panics, failing the test.
    type KeywordValueOutcome = Result<(), Vec<(String, String, String)>>;

    fn keyword_value_outcome(
        result: Result<JsonSchema, JsonSchemaParseErrors>,
    ) -> KeywordValueOutcome {
        match result {
            Ok(_) => Ok(()),
            Err(errors) => Err(errors
                .into_iter()
                .map(|error| match error {
                    JsonSchemaParseError::InvalidKeywordValue {
                        keyword,
                        path,
                        value,
                        ..
                    } => (keyword, path.as_str().to_string(), value.to_string()),
                    other => panic!("expected InvalidKeywordValue, got {other:?}"),
                })
                .collect()),
        }
    }

    fn invalid_value(keyword: &str, path: &str, value: &str) -> KeywordValueOutcome {
        Err(vec![(
            keyword.to_string(),
            path.to_string(),
            value.to_string(),
        )])
    }

    /// `body` (the keys after `$schema`) as a draft-4 document, parsed with `settings`.
    fn parse_draft4(body: &str, settings: &JsonSchemaSettings) -> KeywordValueOutcome {
        let json: String = format!(r#"{{"$schema":"{DRAFT4_SCHEMA}",{body}}}"#);
        keyword_value_outcome(JsonSchema::new_from_str(&json, settings))
    }

    #[test]
    fn draft4_rejects_integer_valued_decimal_count() {
        let expected: KeywordValueOutcome = invalid_value("minLength", "", "2.0");
        let actual: KeywordValueOutcome =
            parse_draft4(r#""minLength":2.0"#, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_exponent_count() {
        let expected: KeywordValueOutcome = invalid_value("maxItems", "", "2.0");
        let actual: KeywordValueOutcome =
            parse_draft4(r#""maxItems":2e0"#, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_decimal_count_in_nested_schema() {
        let expected: KeywordValueOutcome = invalid_value("maxLength", "/properties/a", "3.0");
        let actual: KeywordValueOutcome = parse_draft4(
            r#""properties":{"a":{"type":"string","maxLength":3.0}}"#,
            &JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_decimal_count_the_model_does_not_hold() {
        let expected: KeywordValueOutcome = invalid_value("minProperties", "", "1.0");
        let actual: KeywordValueOutcome =
            parse_draft4(r#""minProperties":1.0"#, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_accepts_integer_count() {
        let expected: KeywordValueOutcome = Ok(());
        let actual: KeywordValueOutcome = parse_draft4(
            r#""minLength":2,"maxItems":3,"minProperties":1"#,
            &JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_does_not_check_count_keyword_names_used_as_data() {
        let expected: KeywordValueOutcome = Ok(());
        let actual: KeywordValueOutcome = parse_draft4(
            r#""properties":{"minLength":{"type":"string"}},"enum":[{"minLength":2.0}]"#,
            &JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_decimal_count_when_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: KeywordValueOutcome = invalid_value("minItems", "", "2.0");
        let actual: KeywordValueOutcome = parse_draft4(r#""minItems":2.0"#, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_decimal_count_from_serde_value() {
        let value: serde_json::Value =
            serde_json::json!({"$schema": DRAFT4_SCHEMA, "items": {"minLength": 2.0}});
        let expected: KeywordValueOutcome = invalid_value("minLength", "/items", "2.0");
        let actual: KeywordValueOutcome = keyword_value_outcome(JsonSchema::try_from(&value));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_accepts_decimal_count_with_numeric_integer_override() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .integer_semantics(IntegerSemantics::Numeric)
            .build();
        let expected: KeywordValueOutcome = Ok(());
        let actual: KeywordValueOutcome = parse_draft4(r#""minLength":2.0"#, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn lexical_integer_override_rejects_decimal_count_in_2020_12() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .integer_semantics(IntegerSemantics::Lexical)
            .build();
        let expected: KeywordValueOutcome = invalid_value("minLength", "", "2.0");
        let actual: KeywordValueOutcome =
            keyword_value_outcome(JsonSchema::new_from_str(r#"{"minLength":2.0}"#, &settings));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft6_accepts_integer_valued_decimal_count() {
        let expected: KeywordValueOutcome = Ok(());
        let actual: KeywordValueOutcome = keyword_value_outcome(JsonSchema::try_from(
            r#"{"$schema":"http://json-schema.org/draft-06/schema#","minLength":2.0}"#,
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_prefilter_finds_quoted_name() {
        let expected: bool = true;
        let actual: bool = super::contains_count_key(r#"{"minimum":1,"maxLength":2}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn count_keyword_prefilter_ignores_other_min_max_keys() {
        let expected: bool = false;
        let actual: bool =
            super::contains_count_key(r#"{"minimum":1,"maximum":2,"maxLengthy":3,"x":"min"}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_pass_assumes_escaped_key_may_match_in_draft4() {
        let expected: bool = true;
        let actual: bool = super::needs_value_pass(
            super::RawSchema::Str(r#"{"\u006dinLength":2}"#),
            Dialect::for_spec_version(SpecVersion::Draft04),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_pass_not_needed_in_2020_12() {
        let expected: bool = false;
        let actual: bool = super::needs_value_pass(
            super::RawSchema::Str(r#"{"\u0069d":"x","minLength":2.0}"#),
            Dialect::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_pass_not_needed_in_draft4_without_id_or_count_keys() {
        let expected: bool = false;
        let actual: bool = super::needs_value_pass(
            super::RawSchema::Str(r#"{"type":"string","minimum":1}"#),
            Dialect::for_spec_version(SpecVersion::Draft04),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_rejects_decimal_count_spelled_with_escape() {
        let expected: KeywordValueOutcome = invalid_value("minLength", "", "2.0");
        let actual: KeywordValueOutcome =
            parse_draft4(r#""\u006dinLength":2.0"#, &JsonSchemaSettings::default());
        assert_eq!(expected, actual);
    }

    #[test]
    fn invalid_keyword_value_display() {
        let expected: &str = "invalid \"maxLength\" value 3.0 at /properties/a: expected a non-negative integer written without a fraction or exponent";
        let json: String =
            format!(r#"{{"$schema":"{DRAFT4_SCHEMA}","properties":{{"a":{{"maxLength":3.0}}}}}}"#);
        let actual: String = JsonSchema::try_from(json.as_str())
            .expect_err("draft 4 rejects 3.0")
            .to_string();
        assert_eq!(expected, actual);
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
    fn parse_without_dialect_settings_records_none() {
        let json = r#"{"$schema":"http://json-schema.org/draft-07/schema#"}"#;
        let schema: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let expected: Option<DialectSettings> = None;
        let actual: Option<DialectSettings> = schema.dialect_settings;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_with_forced_spec_version_uses_it_over_schema_uri() {
        let json = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft07);
        let actual: Dialect = schema.dialect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_with_forced_spec_version_matching_schema_uri_still_records_it() {
        let json = r#"{"$schema":"http://json-schema.org/draft-07/schema#"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: Option<DialectSettings> = Some(DialectSettings {
            forced_spec_version: Some(SpecVersion::Draft07),
            ..DialectSettings::default()
        });
        let actual: Option<DialectSettings> = schema.dialect_settings;
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
    fn parse_with_ref_siblings_override_records_it() {
        let json =
            r##"{"$schema":"http://json-schema.org/draft-07/schema#","$ref":"#/definitions/a"}"##;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .ref_siblings(RefSiblings::Evaluated)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_str(json, &settings).expect("parse");
        let expected: RefSiblings = RefSiblings::Evaluated;
        let actual: RefSiblings = schema.dialect().ref_siblings;
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_unknown_schema_uri_rejected_when_strict() {
        let json = r#"{"$schema":"https://example.com/custom-meta","type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::new_from_str(json, &settings);
        assert!(matches!(
            actual.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::UnknownSchemaUri { uri }]) if uri == "https://example.com/custom-meta"
        ));
    }

    #[test]
    fn parse_unknown_schema_uri_rejected_when_strict_and_unknown_fields_disallowed() {
        let json = r#"{"$schema":"https://example.com/custom-meta","type":"string"}"#;
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .unknown_schema_uri(UnknownSchemaUri::Reject)
            .build();
        let actual: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::new_from_str(json, &settings);
        assert!(matches!(
            actual.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::UnknownSchemaUri { uri }]) if uri == "https://example.com/custom-meta"
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
            .and_then(|a| a.as_object()?.id.clone());
        let property: Option<String> = schema
            .properties
            .get("id")
            .and_then(|p| p.as_object()?.id.clone());
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
        let expected: StrictOutcome = unknown("id", "");
        let actual: StrictOutcome = strict_parse(json);
        assert_eq!(expected, actual);
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
        let country_schema: &JsonSchema = reparsed
            .properties
            .get("country")
            .and_then(Schema::as_object)
            .expect("country");
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
        let status_schema: &JsonSchema = parsed
            .properties
            .get("status")
            .and_then(Schema::as_object)
            .expect("status property");
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
            .and_then(Schema::as_object)
            .unwrap()
            .default_value
            .as_ref();
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

    #[test]
    fn multiple_of_zero_is_rejected() {
        let expected: String =
            "multipleOf must be strictly greater than 0, got 0 at line 1 column 16".to_string();
        let actual: String = match JsonSchema::try_from(r#"{"multipleOf":0}"#)
            .as_ref()
            .map_err(|errors| &errors[..])
        {
            Err([JsonSchemaParseError::Serde(e)]) => e.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_negative_is_rejected() {
        let expected: String =
            "multipleOf must be strictly greater than 0, got -1.5 at line 1 column 19".to_string();
        let actual: String = match JsonSchema::try_from(r#"{"multipleOf":-1.5}"#)
            .as_ref()
            .map_err(|errors| &errors[..])
        {
            Err([JsonSchemaParseError::Serde(e)]) => e.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_is_rejected_when_strict() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .disallow_unknown_fields(true)
            .build();
        let expected: String = "multipleOf must be strictly greater than 0, got 0".to_string();
        let actual: String = match JsonSchema::new_from_str(r#"{"multipleOf":0}"#, &settings)
            .as_ref()
            .map_err(|errors| &errors[..])
        {
            Err([JsonSchemaParseError::Serde(e)]) => e.to_string(),
            other => format!("{other:?}"),
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_is_rejected_in_draft4() {
        let actual: Result<JsonSchema, JsonSchemaParseErrors> = JsonSchema::try_from(
            r#"{"$schema":"http://json-schema.org/draft-04/schema#","multipleOf":0}"#,
        );
        assert!(matches!(
            actual.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
    }

    #[test]
    fn multiple_of_small_positive_is_accepted() {
        let expected: &str = r#"{"multipleOf":1e-8}"#;
        let actual: String = reserialize(r#"{"multipleOf":1e-8}"#, &JsonSchemaSettings::default());
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
        let actual: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::try_from(r#"{"minimum":1,"exclusiveMinimum":true}"#);
        assert!(matches!(
            actual.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
    }

    #[test]
    fn invalid_schema_mentioning_exclusive_bound_keeps_direct_parse_error() {
        let json = "{\"exclusiveMinimum\":1,\n\"minLength\":\"x\"}";
        let expected: String =
            "invalid type: string \"x\", expected a JSON number at line 2 column 15".to_string();
        let actual: String = match JsonSchema::try_from(json)
            .as_ref()
            .map_err(|errors| &errors[..])
        {
            Err([JsonSchemaParseError::Serde(e)]) => e.to_string(),
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
        let expected: StrictOutcome = unknown("unknown", "");
        let actual: StrictOutcome =
            strict_outcome(JsonSchema::new_from_serde_value(&value, &settings));
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_schema_from_serde_value_invalid_type() {
        let value: serde_json::Value = serde_json::json!("not an object");
        let result: Result<JsonSchema, JsonSchemaParseErrors> = JsonSchema::try_from(&value);
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
        let expected_type: Option<TypeSet> = Some(JsonType::Object.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn parse_schema_from_reader_invalid_json() {
        let reader: std::io::Cursor<&[u8]> = std::io::Cursor::new(b"not json");
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let result: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
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
        let result: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::new_from_reader(reader, &settings);
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Io(_)])
        ));
    }

    #[test]
    fn parse_schema_from_path_success() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("schema.json");
        let schema_json = r#"{"type":"array","items":{"type":"string"}}"#;
        std::fs::write(&schema_path, schema_json).expect("write temp file");
        let actual: JsonSchema = JsonSchema::try_from(schema_path.as_path()).expect("parse");
        let expected_type: Option<TypeSet> = Some(JsonType::Array.into());
        assert_eq!(expected_type, actual.type_);
    }

    #[test]
    fn parse_schema_from_path_file_not_found() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let missing_path = temp_dir.path().join("nonexistent.json");
        let result: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::try_from(missing_path.as_path());
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Io(_)])
        ));
    }

    #[test]
    fn parse_schema_from_path_invalid_json() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let schema_path = temp_dir.path().join("bad.json");
        std::fs::write(&schema_path, "not json").expect("write temp file");
        let result: Result<JsonSchema, JsonSchemaParseErrors> =
            JsonSchema::try_from(schema_path.as_path());
        assert!(matches!(
            result.as_ref().map_err(|errors| &errors[..]),
            Err([JsonSchemaParseError::Serde(_)])
        ));
    }

    #[test]
    fn parse_defs_present() {
        let json = r#"{"$defs":{"Foo":{"type":"string"}}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let mut expected_defs: BTreeMap<String, Schema> = BTreeMap::new();
        expected_defs.insert(
            "Foo".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
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
            type_: Some(JsonType::Object.into()),
            properties: BTreeMap::new(),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn parse_definitions_present() {
        let json = r#"{"definitions":{"Bar":{"type":"integer"}}}"#;
        let actual: JsonSchema = JsonSchema::try_from(json).expect("parse");
        let mut expected_definitions: BTreeMap<String, Schema> = BTreeMap::new();
        expected_definitions.insert(
            "Bar".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Integer.into()),
                ..Default::default()
            }),
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
            type_: Some(JsonType::Object.into()),
            properties: BTreeMap::new(),
            ..Default::default()
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn round_trip_with_defs_and_definitions() {
        let mut defs: BTreeMap<String, Schema> = BTreeMap::new();
        defs.insert(
            "Foo".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::String.into()),
                ..Default::default()
            }),
        );
        let mut definitions: BTreeMap<String, Schema> = BTreeMap::new();
        definitions.insert(
            "Bar".to_string(),
            Schema::from(JsonSchema {
                type_: Some(JsonType::Integer.into()),
                ..Default::default()
            }),
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
