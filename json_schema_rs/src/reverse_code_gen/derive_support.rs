//! Run-time support for `#[derive(ToJsonSchema)]` (`json_schema_rs_macro`).
//!
//! The derive emits static data (one [`FieldSpec`] per struct field, one [`EnumValue`] per
//! value-enum variant, one [`SchemaFn`] per union variant) and a call to one of the
//! non-generic builders here, so the code it generates for a type is the same size whatever
//! the type's width: a struct with 800 fields compiles one call, not 800 inlined blocks.

use super::SchemaGenerator;
use crate::json_schema::{JsonSchema, JsonType, Schema};
use std::collections::BTreeMap;

/// A type's `ToJsonSchema::json_schema_with`, as a fn pointer.
pub type SchemaFn = fn(&mut SchemaGenerator) -> JsonSchema;

/// A numeric attribute literal, kept exactly as written: integers stay integers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumberLiteral {
    /// A non-negative integer literal.
    Unsigned(u64),
    /// A negative integer literal.
    Signed(i64),
    /// A float literal.
    Float(f64),
}

impl NumberLiteral {
    /// The literal as a JSON number; `None` for a float that is not finite.
    #[must_use]
    pub fn to_number(self) -> Option<serde_json::Number> {
        match self {
            NumberLiteral::Unsigned(n) => Some(serde_json::Number::from(n)),
            NumberLiteral::Signed(n) => Some(serde_json::Number::from(n)),
            NumberLiteral::Float(f) => serde_json::Number::from_f64(f),
        }
    }
}

/// The literal of a field's `#[json_schema(default = ...)]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultLiteral {
    /// A string literal.
    String(&'static str),
    /// A boolean literal.
    Bool(bool),
    /// An integer or float literal.
    Number(NumberLiteral),
}

impl DefaultLiteral {
    /// The literal as a JSON value; `None` for a float that is not finite.
    #[must_use]
    pub fn to_value(self) -> Option<serde_json::Value> {
        match self {
            DefaultLiteral::String(s) => Some(serde_json::Value::String(s.to_string())),
            DefaultLiteral::Bool(b) => Some(serde_json::Value::Bool(b)),
            DefaultLiteral::Number(n) => n.to_number().map(serde_json::Value::Number),
        }
    }
}

/// One property of a derived struct: its key, its type's schema and the field's attributes.
///
/// Each attribute that is present replaces the corresponding keyword of the type's schema;
/// an absent one keeps it.
#[derive(Debug, Clone, Copy)]
pub struct FieldSpec {
    /// The JSON property key (`#[serde(rename)]` or the field name).
    pub key: &'static str,
    /// The field's `#[serde(alias = "...")]` keys, which serde reads as `key`.
    pub aliases: &'static [&'static str],
    /// The schema of the field's type (one `Option` stripped for an optional field).
    pub schema: SchemaFn,
    /// Whether the key is in `required`.
    pub required: bool,
    /// The field's `///` doc lines, as written (see [`doc_description`]).
    pub doc: &'static [&'static str],
    /// `#[json_schema(minimum = N)]`.
    pub minimum: Option<NumberLiteral>,
    /// `#[json_schema(maximum = N)]`.
    pub maximum: Option<NumberLiteral>,
    /// `#[json_schema(exclusive_minimum = N)]`.
    pub exclusive_minimum: Option<NumberLiteral>,
    /// `#[json_schema(exclusive_maximum = N)]`.
    pub exclusive_maximum: Option<NumberLiteral>,
    /// `#[json_schema(multiple_of = N)]`.
    pub multiple_of: Option<NumberLiteral>,
    /// `#[json_schema(min_items = N)]`.
    pub min_items: Option<u64>,
    /// `#[json_schema(max_items = N)]`.
    pub max_items: Option<u64>,
    /// `#[json_schema(unique_items)]`: sets `uniqueItems: true`.
    pub unique_items: bool,
    /// `#[json_schema(min_length = N)]`.
    pub min_length: Option<u64>,
    /// `#[json_schema(max_length = N)]`.
    pub max_length: Option<u64>,
    /// `#[json_schema(pattern = "...")]`.
    pub pattern: Option<&'static str>,
    /// `#[json_schema(deprecated = true)]`: sets `deprecated: true`.
    pub deprecated: bool,
    /// `#[json_schema(default = ...)]`: replaces the type's `default` (a non-finite float
    /// literal removes it).
    pub default: Option<DefaultLiteral>,
}

/// The `properties` and `required` of a derived struct, from its fields in declaration order.
///
/// Each field's schema is `(field.schema)(generator)` with the field's attributes applied on
/// top. `required` lists the required keys in field order, and is `None` when there are none.
pub fn object_properties(
    generator: &mut SchemaGenerator,
    fields: &[FieldSpec],
) -> (BTreeMap<String, Schema>, Option<Vec<String>>) {
    let mut properties: BTreeMap<String, Schema> = BTreeMap::new();
    let mut required: Vec<String> = Vec::new();
    for field in fields {
        let mut schema: JsonSchema = (field.schema)(generator);
        schema.description = doc_description(field.doc).or(schema.description);
        schema.minimum = field
            .minimum
            .and_then(NumberLiteral::to_number)
            .or(schema.minimum);
        schema.maximum = field
            .maximum
            .and_then(NumberLiteral::to_number)
            .or(schema.maximum);
        schema.exclusive_minimum = field
            .exclusive_minimum
            .and_then(NumberLiteral::to_number)
            .or(schema.exclusive_minimum);
        schema.exclusive_maximum = field
            .exclusive_maximum
            .and_then(NumberLiteral::to_number)
            .or(schema.exclusive_maximum);
        schema.multiple_of = field
            .multiple_of
            .and_then(NumberLiteral::to_number)
            .or(schema.multiple_of);
        schema.min_items = field.min_items.or(schema.min_items);
        schema.max_items = field.max_items.or(schema.max_items);
        schema.unique_items = field.unique_items.then_some(true).or(schema.unique_items);
        schema.min_length = field.min_length.or(schema.min_length);
        schema.max_length = field.max_length.or(schema.max_length);
        schema.pattern = field.pattern.map(str::to_string).or(schema.pattern);
        schema.deprecated = field.deprecated.then_some(true).or(schema.deprecated);
        if let Some(default) = field.default {
            schema.default_value = default.to_value();
        }
        if field.required && field.aliases.is_empty() {
            required.push(field.key.to_string());
        }
        for alias in field.aliases {
            properties.insert((*alias).to_string(), Schema::from(schema.clone()));
        }
        properties.insert(field.key.to_string(), Schema::from(schema));
    }
    let required: Option<Vec<String>> = (!required.is_empty()).then_some(required);
    (properties, required)
}

/// What an aliased field's names require of the object: serde reads the field from any one of
/// them and rejects a second as a duplicate, so a required field's names are a `oneOf` of
/// single-key `required` sets, and an optional field's are `not` any two together.
fn alias_constraints(fields: &[FieldSpec]) -> Vec<Schema> {
    let required_one = |key: &str| -> Schema {
        Schema::from(JsonSchema {
            required: Some(vec![key.to_string()]),
            ..JsonSchema::default()
        })
    };
    let mut constraints: Vec<Schema> = Vec::new();
    for field in fields.iter().filter(|field| !field.aliases.is_empty()) {
        let names: Vec<&str> = std::iter::once(field.key)
            .chain(field.aliases.iter().copied())
            .collect();
        if field.required {
            constraints.push(Schema::from(JsonSchema {
                one_of: Some(names.iter().map(|name| required_one(name)).collect()),
                ..JsonSchema::default()
            }));
            continue;
        }
        let mut pairs: Vec<serde_json::Value> = Vec::new();
        for (index, first) in names.iter().enumerate() {
            for second in &names[index + 1..] {
                pairs.push(serde_json::json!({ "required": [first, second] }));
            }
        }
        let forbidden: serde_json::Value = if pairs.len() == 1 {
            pairs.pop().unwrap_or(serde_json::Value::Bool(true))
        } else {
            serde_json::json!({ "anyOf": pairs })
        };
        let mut constraint: JsonSchema = JsonSchema::default();
        constraint.extensions.insert("not".to_string(), forbidden);
        constraints.push(Schema::from(constraint));
    }
    constraints
}

/// The description from doc lines: each line trimmed, joined with `\n`, leading and trailing
/// blank lines removed; `None` when no text remains.
#[must_use]
pub fn doc_description(lines: &[&str]) -> Option<String> {
    let trimmed: Vec<&str> = lines.iter().map(|line: &&str| line.trim()).collect();
    let joined: String = trimmed.join("\n").trim().to_string();
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// The value of one variant of a derived value enum.
#[derive(Debug, Clone, Copy)]
pub enum EnumValue {
    /// The variant's serde name, as a string.
    Name(&'static str),
    /// The variant's `#[json_schema(value = <JSON literal>)]`.
    Json(fn() -> serde_json::Value),
}

impl EnumValue {
    /// The variant's JSON value.
    #[must_use]
    pub fn to_value(self) -> serde_json::Value {
        match self {
            EnumValue::Name(name) => serde_json::Value::String(name.to_string()),
            EnumValue::Json(value) => value(),
        }
    }
}

/// The `enum` and `const` of a derived value enum: `const` for one variant, else `enum` of
/// every variant's value in order (`[]` for no variants).
#[must_use]
pub fn value_enum_keywords(
    values: &[EnumValue],
) -> (Option<Vec<serde_json::Value>>, Option<serde_json::Value>) {
    if let [value] = values {
        (None, Some(value.to_value()))
    } else {
        (
            Some(values.iter().map(|value| value.to_value()).collect()),
            None,
        )
    }
}

/// One `#[serde(flatten)]` field of a derived struct or struct variant.
#[derive(Debug, Clone, Copy)]
pub struct FlattenSpec {
    /// The field type's `ToJsonSchema::json_schema_flattened` (one `Option` stripped).
    pub schema: SchemaFn,
    /// Whether the field is an `Option`: serde reads `None` when its keys do not match, so
    /// none of them is required.
    pub optional: bool,
}

/// The object a derived struct (or struct variant) deserializes from.
#[derive(Debug, Clone, Copy)]
pub struct ObjectSpec {
    /// The named fields, in declaration order.
    pub fields: &'static [FieldSpec],
    /// The `#[serde(flatten)]` fields, in declaration order.
    pub flattens: &'static [FlattenSpec],
    /// Whether keys that no field (and no flattened map) claims are rejected.
    pub closed: bool,
}

/// What a derived object says about the keys its fields do not name.
enum Unnamed {
    /// Rejected.
    Closed,
    /// Accepted, whatever their values.
    Open,
    /// Accepted when their values match the schema (a flattened map's value schema).
    Schema(Schema),
}

/// The schema of the object `spec` describes.
///
/// A flattened field whose schema is a plain object (a derived struct's body, a map) is
/// merged in: its properties join the parent's (the parent's own field wins a shared key,
/// as serde gives the key to it), its required keys too unless the field is an `Option`, and
/// a map's value schema types the keys no field names. Any other flattened schema (an enum,
/// a `$ref`) is an `allOf` part, and the parent then closes itself with
/// `unevaluatedProperties` (which sees the keys the parts evaluate) instead of
/// `additionalProperties`.
pub fn object_schema(generator: &mut SchemaGenerator, spec: &ObjectSpec) -> JsonSchema {
    let (mut properties, required) = object_properties(generator, spec.fields);
    let mut required: Vec<String> = required.unwrap_or_default();
    let mut unnamed: Unnamed = if spec.closed {
        Unnamed::Closed
    } else {
        Unnamed::Open
    };
    let mut parts: Vec<Schema> = Vec::new();
    let aliases: Vec<Schema> = alias_constraints(spec.fields);
    for flatten in spec.flattens {
        let mut inner: JsonSchema = (flatten.schema)(generator);
        if !is_plain_object(&inner) {
            allow_unknown_properties(&mut inner);
            parts.push(Schema::from(if flatten.optional {
                JsonSchema {
                    any_of: Some(vec![Schema::from(inner), Schema::Bool(true)]),
                    ..JsonSchema::default()
                }
            } else {
                inner
            }));
            continue;
        }
        for (key, schema) in inner.properties {
            properties.entry(key).or_insert(schema);
        }
        if !flatten.optional {
            for key in inner.required.unwrap_or_default() {
                if !required.contains(&key) {
                    required.push(key);
                }
            }
        }
        match inner.additional_properties {
            // A closed inner object claims only its own properties.
            Some(Schema::Bool(false)) => {}
            None | Some(Schema::Bool(true)) => {
                if matches!(unnamed, Unnamed::Closed) {
                    unnamed = Unnamed::Open;
                }
            }
            Some(Schema::Object(value)) if *value == JsonSchema::default() => {
                if matches!(unnamed, Unnamed::Closed) {
                    unnamed = Unnamed::Open;
                }
            }
            Some(value) => {
                // Every flattened map sees every key no field claims: the values match all.
                unnamed = match unnamed {
                    Unnamed::Closed | Unnamed::Open => Unnamed::Schema(value),
                    Unnamed::Schema(previous) => Unnamed::Schema(Schema::from(JsonSchema {
                        all_of: Some(vec![previous, value]),
                        ..JsonSchema::default()
                    })),
                };
            }
        }
    }
    let unnamed: Option<Schema> = match unnamed {
        Unnamed::Closed => Some(Schema::Bool(false)),
        Unnamed::Open => None,
        Unnamed::Schema(schema) => Some(schema),
    };
    let mut schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties,
        required: (!required.is_empty()).then_some(required),
        ..JsonSchema::default()
    };
    if parts.is_empty() {
        schema.additional_properties = unnamed;
        if !aliases.is_empty() {
            schema.all_of = Some(aliases);
        }
    } else {
        parts.extend(aliases);
        schema.all_of = Some(parts);
        if let Some(unnamed) = unnamed {
            schema.extensions.insert(
                "unevaluatedProperties".to_string(),
                serde_json::to_value(unnamed).unwrap_or(serde_json::Value::Bool(false)),
            );
        }
    }
    schema
}

/// Whether `schema` only lists properties of an object, so a parent can merge them into its
/// own: no combinator, `$ref`, `$id`, `enum`, `const` or other keyword that would be lost.
fn is_plain_object(schema: &JsonSchema) -> bool {
    let plain: JsonSchema = JsonSchema {
        type_: None,
        properties: BTreeMap::new(),
        additional_properties: None,
        required: None,
        title: None,
        description: None,
        comment: None,
        ..schema.clone()
    };
    plain == JsonSchema::default()
        && schema
            .type_
            .is_none_or(|types| types == JsonType::Object.into())
}

/// Removes `additionalProperties: false` (and `unevaluatedProperties: false`) from `schema`
/// and from the branches of its `allOf`, `anyOf` and `oneOf`, at any depth: under
/// `#[serde(flatten)]` the parent's own keys sit beside the flattened ones, and the parent's
/// `unevaluatedProperties` rejects unknown keys instead.
fn allow_unknown_properties(schema: &mut JsonSchema) {
    let mut stack: Vec<&mut JsonSchema> = vec![schema];
    while let Some(current) = stack.pop() {
        if current.additional_properties == Some(Schema::Bool(false)) {
            current.additional_properties = None;
        }
        if current.extensions.get("unevaluatedProperties") == Some(&serde_json::Value::Bool(false))
        {
            current.extensions.remove("unevaluatedProperties");
        }
        for list in [
            current.all_of.as_mut(),
            current.any_of.as_mut(),
            current.one_of.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            stack.extend(list.iter_mut().filter_map(Schema::as_object_mut));
        }
    }
}

/// How a derived enum's variants are tagged: serde's enum representations.
#[derive(Debug, Clone, Copy)]
pub enum Tagging {
    /// `{"Variant": payload}`, a unit variant as the string `"Variant"`.
    External,
    /// `#[serde(tag = "t")]`: the tag inside the payload's own object.
    Internal {
        /// The tag key.
        tag: &'static str,
    },
    /// `#[serde(tag = "t", content = "c")]`: `{"t": "Variant", "c": payload}`.
    Adjacent {
        /// The tag key.
        tag: &'static str,
        /// The content key.
        content: &'static str,
    },
    /// `#[serde(untagged)]`: the bare payload.
    Untagged {
        /// `#[json_schema(one_of)]`: the branches under `oneOf`, not `anyOf`.
        one_of: bool,
        /// `#[json_schema(flatten_single_entry)]`: an enum of one branch is that branch.
        flatten_single_entry: bool,
    },
}

/// The fields of one derived enum variant.
#[derive(Debug, Clone, Copy)]
pub enum VariantShape {
    /// No fields.
    Unit,
    /// One unnamed field.
    Newtype {
        /// Its type's `json_schema_with`.
        schema: SchemaFn,
        /// Its type's `json_schema_flattened`: the object an internal tag merges into.
        flattened: SchemaFn,
    },
    /// Several unnamed fields: an array of exactly their schemas.
    Tuple(&'static [SchemaFn]),
    /// Named fields.
    Struct(ObjectSpec),
}

/// One variant of a derived enum.
#[derive(Debug, Clone, Copy)]
pub struct VariantSpec {
    /// The serde name (the tag value).
    pub name: &'static str,
    /// The variant's `#[serde(alias = "...")]` names, which serde reads as `name`.
    pub aliases: &'static [&'static str],
    /// The variant's `///` doc lines (see [`doc_description`]).
    pub doc: &'static [&'static str],
    /// The variant's fields.
    pub shape: VariantShape,
    /// `#[serde(untagged)]` on the variant: tried after the tagged ones, as its bare payload.
    pub untagged: bool,
    /// `#[serde(other)]`: the unit variant every unknown tag deserializes to.
    pub other: bool,
}

/// A derived enum (other than an externally tagged one of unit variants, a value enum).
#[derive(Debug, Clone, Copy)]
pub struct EnumSpec {
    /// The tagging of every variant without `#[serde(untagged)]`.
    pub tagging: Tagging,
    /// The variants serde deserializes, in declaration order.
    pub variants: &'static [VariantSpec],
    /// Whether an internally or adjacently tagged object rejects keys it does not name.
    pub closed: bool,
    /// Whether this is the schema serde reads under `#[serde(flatten)]`, where an externally
    /// tagged unit variant is the key with a `null` value.
    pub flattened: bool,
}

/// The schema of a derived enum: one branch per variant under `oneOf` (`anyOf` when
/// untagged), the tagged ones before any `#[serde(untagged)]` variant.
///
/// Externally tagged unit variants share one `{"type": "string", "enum": [...]}` branch,
/// first.
pub fn enum_schema(generator: &mut SchemaGenerator, spec: &EnumSpec) -> JsonSchema {
    let mut unit_names: Vec<serde_json::Value> = Vec::new();
    let mut tagged: Vec<Schema> = Vec::new();
    let mut untagged: Vec<Schema> = Vec::new();
    let other_names: Vec<serde_json::Value> = spec
        .variants
        .iter()
        .filter(|variant| !variant.untagged && !variant.other)
        .flat_map(variant_names)
        .collect();
    for variant in spec.variants {
        let tagging: Option<Tagging> = (!variant.untagged).then_some(spec.tagging);
        let branch: Option<JsonSchema> = match tagging {
            Some(Tagging::External) => external_branch(generator, spec, variant, &mut unit_names),
            Some(Tagging::Internal { tag }) => {
                Some(internal_branch(generator, spec, variant, tag, &other_names))
            }
            Some(Tagging::Adjacent { tag, content }) => Some(adjacent_branch(
                generator,
                spec,
                variant,
                tag,
                content,
                &other_names,
            )),
            Some(Tagging::Untagged { .. }) | None => Some(payload_schema(generator, variant.shape)),
        };
        let Some(mut branch) = branch else {
            continue;
        };
        branch.description = doc_description(variant.doc).or(branch.description);
        if variant.untagged || matches!(spec.tagging, Tagging::Untagged { .. }) {
            untagged.push(Schema::from(branch));
        } else {
            tagged.push(Schema::from(branch));
        }
    }
    if !unit_names.is_empty() {
        tagged.insert(0, Schema::from(string_values(unit_names)));
    }
    let tagged: Option<Schema> = match tagged.len() {
        0 => None,
        1 => tagged.pop(),
        _ => Some(Schema::from(JsonSchema {
            one_of: Some(tagged),
            ..JsonSchema::default()
        })),
    };
    let mut branches: Vec<Schema> = tagged.into_iter().chain(untagged).collect();
    let (untagged_enum, one_of, flatten_single_entry): (bool, bool, bool) = match spec.tagging {
        Tagging::Untagged {
            one_of,
            flatten_single_entry,
        } => (true, one_of, flatten_single_entry),
        _ => (false, false, false),
    };
    if flatten_single_entry
        && let [Schema::Object(branch)] = branches.as_slice()
        && !super::resource_uri::is_resource(branch)
    {
        let Some(Schema::Object(branch)) = branches.pop() else {
            unreachable!("matched one object branch");
        };
        return *branch;
    }
    if let [Schema::Object(branch)] = branches.as_slice()
        && !untagged_enum
        && branch.one_of.is_some()
    {
        // Every variant is tagged: the branches' own `oneOf` is the enum's.
        let Some(Schema::Object(branch)) = branches.pop() else {
            unreachable!("matched one object branch");
        };
        return *branch;
    }
    if one_of {
        JsonSchema {
            one_of: Some(branches),
            ..JsonSchema::default()
        }
    } else if branches.len() == 1 && !untagged_enum {
        match branches.pop() {
            Some(Schema::Object(branch)) => *branch,
            _ => JsonSchema::default(),
        }
    } else {
        JsonSchema {
            any_of: Some(branches),
            ..JsonSchema::default()
        }
    }
}

/// `{"type": "string", "const": v}` for one value, `{"type": "string", "enum": [...]}` for
/// several.
fn string_values(mut values: Vec<serde_json::Value>) -> JsonSchema {
    let (enum_values, const_value) = if values.len() == 1 {
        (None, values.pop())
    } else {
        (Some(values), None)
    };
    JsonSchema {
        type_: Some(JsonType::String.into()),
        enum_values,
        const_value,
        ..JsonSchema::default()
    }
}

/// The schema of a variant's fields on their own (an untagged variant, an external or
/// adjacent payload): `null` for a unit variant, the field's schema for a newtype, an array
/// of exactly the fields' schemas for a tuple, an object for named fields.
fn payload_schema(generator: &mut SchemaGenerator, shape: VariantShape) -> JsonSchema {
    match shape {
        VariantShape::Unit => JsonSchema {
            type_: Some(JsonType::Null.into()),
            ..JsonSchema::default()
        },
        VariantShape::Newtype { schema, .. } => schema(generator),
        VariantShape::Tuple(items) => tuple_schema(generator, items),
        VariantShape::Struct(object) => object_schema(generator, &object),
    }
}

/// An array of exactly `items.len()` values, each matching the schema at its position.
pub fn tuple_schema(generator: &mut SchemaGenerator, items: &[SchemaFn]) -> JsonSchema {
    let prefix: Vec<serde_json::Value> = items
        .iter()
        .map(|item: &SchemaFn| {
            serde_json::to_value(Schema::from(item(generator)))
                .unwrap_or(serde_json::Value::Bool(true))
        })
        .collect();
    let len: u64 = u64::try_from(items.len()).unwrap_or(u64::MAX);
    let mut schema: JsonSchema = JsonSchema {
        type_: Some(JsonType::Array.into()),
        min_items: Some(len),
        max_items: Some(len),
        ..JsonSchema::default()
    };
    if !prefix.is_empty() {
        schema
            .extensions
            .insert("prefixItems".to_string(), serde_json::Value::Array(prefix));
    }
    schema
}

/// An object with exactly the one key `key`, whose value matches `value`.
fn single_key_object(key: &str, value: JsonSchema) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties: BTreeMap::from([(key.to_string(), Schema::from(value))]),
        required: Some(vec![key.to_string()]),
        additional_properties: Some(Schema::Bool(false)),
        ..JsonSchema::default()
    }
}

/// An externally tagged variant's branch, or `None` for a unit variant, whose name joins
/// `unit_names` (outside `#[serde(flatten)]`).
fn external_branch(
    generator: &mut SchemaGenerator,
    spec: &EnumSpec,
    variant: &VariantSpec,
    unit_names: &mut Vec<serde_json::Value>,
) -> Option<JsonSchema> {
    if matches!(variant.shape, VariantShape::Unit) && !spec.flattened {
        unit_names.extend(variant_names(variant));
        return None;
    }
    let payload: JsonSchema = payload_schema(generator, variant.shape);
    if variant.aliases.is_empty() {
        return Some(single_key_object(variant.name, payload));
    }
    // One single-key object per name: the names are distinct keys, so exactly one matches.
    let branches: Vec<Schema> = std::iter::once(variant.name)
        .chain(variant.aliases.iter().copied())
        .map(|name| Schema::from(single_key_object(name, payload.clone())))
        .collect();
    Some(JsonSchema {
        one_of: Some(branches),
        ..JsonSchema::default()
    })
}

/// A variant's serde name and its aliases, as JSON strings.
fn variant_names(variant: &VariantSpec) -> impl Iterator<Item = serde_json::Value> {
    std::iter::once(variant.name)
        .chain(variant.aliases.iter().copied())
        .map(|name| serde_json::Value::String(name.to_string()))
}

/// The schema of a tag key's value: the variant's name, or for `#[serde(other)]` any string
/// that names no other variant.
fn tag_schema(variant: &VariantSpec, other_names: &[serde_json::Value]) -> JsonSchema {
    if variant.other {
        let mut schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
            ..JsonSchema::default()
        };
        if !other_names.is_empty() {
            schema.extensions.insert(
                "not".to_string(),
                serde_json::json!({ "enum": other_names }),
            );
        }
        schema
    } else {
        string_values(variant_names(variant).collect())
    }
}

/// An internally tagged variant's branch: its fields' object with the tag key added.
///
/// A newtype's payload is read from the same object, so its flattened schema takes the tag
/// when it is a plain object, and is an `allOf` part beside the tag otherwise.
fn internal_branch(
    generator: &mut SchemaGenerator,
    spec: &EnumSpec,
    variant: &VariantSpec,
    tag: &str,
    other_names: &[serde_json::Value],
) -> JsonSchema {
    let tag_value: Schema = Schema::from(tag_schema(variant, other_names));
    let mut object: JsonSchema = match variant.shape {
        VariantShape::Unit | VariantShape::Tuple(_) => JsonSchema {
            type_: Some(JsonType::Object.into()),
            additional_properties: spec.closed.then_some(Schema::Bool(false)),
            ..JsonSchema::default()
        },
        VariantShape::Struct(object) => object_schema(
            generator,
            &ObjectSpec {
                closed: object.closed && spec.closed,
                ..object
            },
        ),
        VariantShape::Newtype { flattened, .. } => {
            let payload: JsonSchema = flattened(generator);
            if payload.type_ == Some(JsonType::Null.into()) && is_plain_null(&payload) {
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    ..JsonSchema::default()
                }
            } else if is_plain_object(&payload) {
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    ..payload
                }
            } else {
                let mut payload: JsonSchema = payload;
                allow_unknown_properties(&mut payload);
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    all_of: Some(vec![Schema::from(payload)]),
                    ..JsonSchema::default()
                }
            }
        }
    };
    object.properties.insert(tag.to_string(), tag_value);
    let mut required: Vec<String> = vec![tag.to_string()];
    required.extend(
        object
            .required
            .take()
            .unwrap_or_default()
            .into_iter()
            .filter(|key| key != tag),
    );
    object.required = Some(required);
    object
}

/// Whether `schema` is `{"type": "null"}` and nothing else (a unit type's payload).
fn is_plain_null(schema: &JsonSchema) -> bool {
    *schema
        == JsonSchema {
            type_: Some(JsonType::Null.into()),
            ..JsonSchema::default()
        }
}

/// An adjacently tagged variant's branch: `{tag: name, content: payload}`, the content
/// absent or `null` for a unit variant.
fn adjacent_branch(
    generator: &mut SchemaGenerator,
    spec: &EnumSpec,
    variant: &VariantSpec,
    tag: &str,
    content: &str,
    other_names: &[serde_json::Value],
) -> JsonSchema {
    let mut properties: BTreeMap<String, Schema> = BTreeMap::new();
    properties.insert(
        tag.to_string(),
        Schema::from(tag_schema(variant, other_names)),
    );
    let mut required: Vec<String> = vec![tag.to_string()];
    let payload: JsonSchema = payload_schema(generator, variant.shape);
    if !matches!(variant.shape, VariantShape::Unit) {
        required.push(content.to_string());
    }
    properties.insert(content.to_string(), Schema::from(payload));
    JsonSchema {
        type_: Some(JsonType::Object.into()),
        properties,
        required: Some(required),
        additional_properties: spec.closed.then_some(Schema::Bool(false)),
        ..JsonSchema::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DefaultLiteral, EnumValue, FieldSpec, FlattenSpec, NumberLiteral, ObjectSpec,
        doc_description, object_properties, object_schema, value_enum_keywords,
    };
    use crate::json_schema::{JsonSchema, JsonType, Schema};
    use crate::reverse_code_gen::SchemaGenerator;
    use std::collections::BTreeMap;

    type Properties = (BTreeMap<String, Schema>, Option<Vec<String>>);

    /// A schema with every keyword a field attribute can replace set: the base every override
    /// test applies its field attributes to.
    fn base_schema(_generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Integer.into()),
            description: Some("base".to_string()),
            minimum: Some(serde_json::Number::from(3_u64)),
            maximum: Some(serde_json::Number::from(4_u64)),
            exclusive_minimum: Some(serde_json::Number::from(1_u64)),
            exclusive_maximum: Some(serde_json::Number::from(9_u64)),
            multiple_of: Some(serde_json::Number::from(2_u64)),
            min_items: Some(1),
            max_items: Some(2),
            unique_items: Some(false),
            min_length: Some(3),
            max_length: Some(4),
            pattern: Some("base".to_string()),
            deprecated: Some(false),
            default_value: Some(serde_json::Value::Bool(false)),
            ..JsonSchema::default()
        }
    }

    /// A field with no attributes over [`base_schema`].
    const PLAIN: FieldSpec = FieldSpec {
        key: "k",
        aliases: &[],
        schema: base_schema,
        required: false,
        doc: &[],
        minimum: None,
        maximum: None,
        exclusive_minimum: None,
        exclusive_maximum: None,
        multiple_of: None,
        min_items: None,
        max_items: None,
        unique_items: false,
        min_length: None,
        max_length: None,
        pattern: None,
        deprecated: false,
        default: None,
    };

    /// The properties of one field `k` with the given schema and nothing required.
    fn one_property(schema: JsonSchema) -> Properties {
        (
            BTreeMap::from([("k".to_string(), Schema::from(schema))]),
            None,
        )
    }

    fn build(fields: &[FieldSpec]) -> Properties {
        object_properties(&mut SchemaGenerator::default(), fields)
    }

    #[test]
    fn field_without_attributes_keeps_type_schema() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::default()));
        let actual: Properties = build(&[PLAIN]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn field_with_every_attribute_replaces_type_keywords() {
        let expected: Properties = one_property(JsonSchema {
            type_: Some(JsonType::Integer.into()),
            description: Some("Doc".to_string()),
            minimum: Some(serde_json::Number::from(-5_i64)),
            maximum: Some(serde_json::Number::from(18_446_744_073_709_551_615_u64)),
            exclusive_minimum: serde_json::Number::from_f64(0.5),
            exclusive_maximum: serde_json::Number::from_f64(-2.5),
            multiple_of: Some(serde_json::Number::from(7_u64)),
            min_items: Some(10),
            max_items: Some(20),
            unique_items: Some(true),
            min_length: Some(30),
            max_length: Some(40),
            pattern: Some("^a$".to_string()),
            deprecated: Some(true),
            default_value: Some(serde_json::Value::String("d".to_string())),
            ..JsonSchema::default()
        });
        let actual: Properties = build(&[FieldSpec {
            doc: &[" Doc "],
            minimum: Some(NumberLiteral::Signed(-5)),
            maximum: Some(NumberLiteral::Unsigned(u64::MAX)),
            exclusive_minimum: Some(NumberLiteral::Float(0.5)),
            exclusive_maximum: Some(NumberLiteral::Float(-2.5)),
            multiple_of: Some(NumberLiteral::Unsigned(7)),
            min_items: Some(10),
            max_items: Some(20),
            unique_items: true,
            min_length: Some(30),
            max_length: Some(40),
            pattern: Some("^a$"),
            deprecated: true,
            default: Some(DefaultLiteral::String("d")),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_finite_float_attribute_keeps_type_keyword() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::default()));
        let actual: Properties = build(&[FieldSpec {
            minimum: Some(NumberLiteral::Float(f64::INFINITY)),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn blank_doc_keeps_type_description() {
        let expected: Properties = one_property(base_schema(&mut SchemaGenerator::default()));
        let actual: Properties = build(&[FieldSpec {
            doc: &["  ", ""],
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: Some(serde_json::Value::Bool(true)),
            ..base_schema(&mut SchemaGenerator::default())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Bool(true)),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: Some(serde_json::Value::Number(serde_json::Number::from(-3_i64))),
            ..base_schema(&mut SchemaGenerator::default())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Signed(-3))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn float_default_replaces_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: serde_json::Number::from_f64(2.5).map(serde_json::Value::Number),
            ..base_schema(&mut SchemaGenerator::default())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Float(2.5))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_finite_float_default_removes_type_default() {
        let expected: Properties = one_property(JsonSchema {
            default_value: None,
            ..base_schema(&mut SchemaGenerator::default())
        });
        let actual: Properties = build(&[FieldSpec {
            default: Some(DefaultLiteral::Number(NumberLiteral::Float(f64::NAN))),
            ..PLAIN
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_fields_listed_in_field_order() {
        let expected: Option<Vec<String>> = Some(vec!["z".to_string(), "a".to_string()]);
        let actual: Option<Vec<String>> = build(&[
            FieldSpec {
                key: "z",
                required: true,
                ..PLAIN
            },
            FieldSpec { key: "m", ..PLAIN },
            FieldSpec {
                key: "a",
                required: true,
                ..PLAIN
            },
        ])
        .1;
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_aliased_field_is_left_out_of_required() {
        let expected: Option<Vec<String>> = Some(vec!["b".to_string()]);
        let actual: Option<Vec<String>> = build(&[
            FieldSpec {
                key: "a",
                aliases: &["x"],
                required: true,
                ..PLAIN
            },
            FieldSpec {
                key: "b",
                required: true,
                ..PLAIN
            },
        ])
        .1;
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_fields_build_empty_properties_and_no_required() {
        let expected: Properties = (BTreeMap::new(), None);
        let actual: Properties = build(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_trims_lines_and_drops_outer_blank_lines() {
        let expected: Option<String> = Some("First\n\nx\nLast".to_string());
        let actual: Option<String> = doc_description(&["", " First", "", "   x  ", " Last ", " "]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_of_no_lines_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = doc_description(&[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn doc_description_of_blank_lines_is_none() {
        let expected: Option<String> = None;
        let actual: Option<String> = doc_description(&[" ", "\t"]);
        assert_eq!(expected, actual);
    }

    type EnumKeywords = (Option<Vec<serde_json::Value>>, Option<serde_json::Value>);

    #[test]
    fn single_value_enum_is_const() {
        let expected: EnumKeywords = (None, Some(serde_json::Value::String("a".to_string())));
        let actual: EnumKeywords = value_enum_keywords(&[EnumValue::Name("a")]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_enum_lists_names_and_json_values_in_order() {
        let expected: EnumKeywords = (
            Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::json!({"k": [1, null]}),
            ]),
            None,
        );
        let actual: EnumKeywords = value_enum_keywords(&[
            EnumValue::Name("a"),
            EnumValue::Json(|| serde_json::json!({"k": [1, null]})),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn value_enum_without_variants_is_empty_enum() {
        let expected: EnumKeywords = (Some(Vec::new()), None);
        let actual: EnumKeywords = value_enum_keywords(&[]);
        assert_eq!(expected, actual);
    }

    fn string_schema(_generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::String.into()),
            ..JsonSchema::default()
        }
    }

    fn integer_schema(_generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Integer.into()),
            ..JsonSchema::default()
        }
    }

    fn open_object(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: BTreeMap::from([("a".to_string(), Schema::from(string_schema(generator)))]),
            required: Some(vec!["a".to_string()]),
            ..JsonSchema::default()
        }
    }

    fn closed_object(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            additional_properties: Some(Schema::Bool(false)),
            ..open_object(generator)
        }
    }

    fn string_map(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            additional_properties: Some(Schema::from(string_schema(generator))),
            ..JsonSchema::default()
        }
    }

    fn integer_map(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            additional_properties: Some(Schema::from(integer_schema(generator))),
            ..JsonSchema::default()
        }
    }

    fn flattened(flattens: &'static [FlattenSpec]) -> JsonSchema {
        let spec: ObjectSpec = ObjectSpec {
            fields: &[],
            flattens,
            closed: true,
        };
        object_schema(&mut SchemaGenerator::default(), &spec)
    }

    #[test]
    fn object_schema_flattening_a_closed_object_stays_closed() {
        let expected: JsonSchema = closed_object(&mut SchemaGenerator::default());
        let actual: JsonSchema = flattened(&[FlattenSpec {
            schema: closed_object,
            optional: false,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn object_schema_flattening_an_open_object_opens_the_parent() {
        let expected: JsonSchema = open_object(&mut SchemaGenerator::default());
        let actual: JsonSchema = flattened(&[FlattenSpec {
            schema: open_object,
            optional: false,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn object_schema_optional_flatten_requires_none_of_its_keys() {
        let expected: JsonSchema = JsonSchema {
            required: None,
            ..closed_object(&mut SchemaGenerator::default())
        };
        let actual: JsonSchema = flattened(&[FlattenSpec {
            schema: closed_object,
            optional: true,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn object_schema_two_flattened_maps_require_both_value_schemas() {
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            additional_properties: Some(Schema::from(JsonSchema {
                all_of: Some(vec![
                    Schema::from(string_schema(&mut SchemaGenerator::default())),
                    Schema::from(integer_schema(&mut SchemaGenerator::default())),
                ]),
                ..JsonSchema::default()
            })),
            ..JsonSchema::default()
        };
        let actual: JsonSchema = flattened(&[
            FlattenSpec {
                schema: string_map,
                optional: false,
            },
            FlattenSpec {
                schema: integer_map,
                optional: false,
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn object_schema_open_spec_without_flattens_has_no_additional_properties() {
        let spec: ObjectSpec = ObjectSpec {
            fields: &[],
            flattens: &[],
            closed: false,
        };
        let expected: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            ..JsonSchema::default()
        };
        let actual: JsonSchema = object_schema(&mut SchemaGenerator::default(), &spec);
        assert_eq!(expected, actual);
    }
}
