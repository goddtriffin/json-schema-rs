//! Reverse codegen: Rust types to JSON Schema.
//!
//! Types implement [`ToJsonSchema`] to produce a [`JsonSchema`] value that can be
//! serialized via [`TryFrom`] to String or `Vec<u8>`.

use crate::json_schema::JsonSchema;
use crate::json_schema::json_schema::AdditionalProperties;
use std::collections::{BTreeMap, BTreeSet};

/// Trait for types that can produce a JSON Schema.
///
/// Implemented for primitive/standard types (e.g. `String`, `Option<T>`) and for
/// structs via `#[derive(ToJsonSchema)]` with optional container/field attributes.
pub trait ToJsonSchema {
    /// Returns the JSON Schema for this type as a standalone root schema.
    fn json_schema() -> JsonSchema;

    /// Returns the schema for this type as a subschema of the document `generator` is
    /// building.
    ///
    /// Named types (derived structs and enums) return a `$ref` and register their body in the
    /// generator's `$defs`; containers pass the generator through to their element types. The
    /// default inlines [`ToJsonSchema::json_schema`], hoisting any `$defs` it carries into the
    /// generator, so a hand-written impl only needs `json_schema`.
    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        generator.inline_subschema(Self::json_schema())
    }

    /// Identity of this type in a [`SchemaGenerator`]: two types with the same `schema_id`
    /// share one `$defs` entry.
    ///
    /// The derive returns the type's full path (`module_path!()` plus the type name), so
    /// same-name types from different modules stay distinct.
    #[doc(hidden)]
    #[must_use]
    fn schema_id() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Prefix of the provisional `$ref` the generator returns for a definition until
/// [`SchemaGenerator::into_root_schema`] assigns final `$defs` names. A second `#` is not a
/// valid URI character, so no `$ref` written by hand can start with it.
const PROVISIONAL_REF_PREFIX: &str = "#/$defs/#";

/// Identity of one `$defs` entry. Entries that share a name are suffixed in this order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionIdentity {
    /// A named type, by [`ToJsonSchema::schema_id`].
    Type(String),
    /// A `$defs` entry hoisted from a standalone schema, in the order it was first seen.
    Hoisted(usize),
}

/// One `$defs` entry of the document being built.
#[derive(Debug)]
struct Definition {
    identity: DefinitionIdentity,
    /// Preferred `$defs` key: the bare type name, or the hoisted key.
    name: String,
    /// `None` while the body is being built, and for a root that nothing refers back to.
    body: Option<JsonSchema>,
}

/// A standalone schema already passed to [`SchemaGenerator::inline_subschema`] and the
/// subschema it produced, so inlining the same schema again reuses its definitions.
#[derive(Debug)]
struct InlinedSchema {
    standalone: JsonSchema,
    inlined: JsonSchema,
}

/// Generator context for one root schema: owns the `$defs` entries and the set of named types
/// whose body is being built.
///
/// Entries are keyed by identity ([`ToJsonSchema::schema_id`] for named types), not by name.
/// A named type found in the pending set returns its `$ref` immediately, so a cycle of any
/// length between derived types terminates. Descent depth is bounded by the number of
/// distinct named types (each is built at most once per generator) plus the nesting of
/// container types written in the source.
///
/// `$ref`s returned while building are provisional: [`SchemaGenerator::into_root_schema`]
/// assigns the final `$defs` keys (the bare name when unique, otherwise a numeric suffix on
/// every entry sharing it, ordered by identity) and rewrites them.
#[derive(Debug, Default)]
pub struct SchemaGenerator {
    definitions: Vec<Definition>,
    type_definitions: BTreeMap<String, usize>,
    pending: BTreeSet<usize>,
    referenced_while_pending: BTreeSet<usize>,
    inlined: Vec<InlinedSchema>,
}

impl SchemaGenerator {
    /// Creates a generator with no definitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a `$ref` to the named type `schema_id` (preferred `$defs` key `name`), building
    /// its body with `build` the first time `schema_id` is requested.
    ///
    /// When `schema_id` is already being built (a recursive reference), the `$ref` is returned
    /// without calling `build`.
    pub fn named_subschema(
        &mut self,
        schema_id: &str,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        let index: usize = if let Some(&index) = self.type_definitions.get(schema_id) {
            if self.pending.contains(&index) {
                self.referenced_while_pending.insert(index);
            }
            index
        } else {
            let index: usize = self.push_type_definition(schema_id, name);
            self.pending.insert(index);
            let body: JsonSchema = build(self);
            self.pending.remove(&index);
            self.definitions[index].body = Some(body);
            index
        };
        JsonSchema {
            ref_: Some(provisional_ref(index)),
            ..Default::default()
        }
    }

    /// Builds the root schema for the named type `schema_id` (preferred `$defs` key `name`)
    /// from `build`.
    ///
    /// The root body is returned inline. It is also placed in `$defs` when a nested type
    /// refers back to it, so that `$ref` resolves.
    #[must_use]
    pub fn into_named_root_schema(
        mut self,
        schema_id: &str,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        let index: usize = match self.type_definitions.get(schema_id) {
            Some(&index) => index,
            None => self.push_type_definition(schema_id, name),
        };
        self.pending.insert(index);
        let body: JsonSchema = build(&mut self);
        self.pending.remove(&index);
        if self.referenced_while_pending.contains(&index) {
            self.definitions[index].body = Some(body.clone());
        }
        self.into_root_schema(body)
    }

    /// Assigns the final `$defs` keys, rewrites every provisional `$ref` in `body` and the
    /// definitions, attaches the `$defs` (if any) to `body` and returns it as the root schema.
    ///
    /// Each name used by one entry stays bare. When several entries share a name, every one
    /// of them gets a numeric suffix (`Name0`, `Name1`, ...) in identity order, skipping
    /// suffixed names that are already taken. A hoisted entry whose name and body equal
    /// another entry's is merged into it first.
    #[must_use]
    pub fn into_root_schema(self, mut body: JsonSchema) -> JsonSchema {
        let mut definitions: Vec<Definition> = self.definitions;
        let mut canonical: Vec<usize> = (0..definitions.len()).collect();
        merge_equal_hoisted_definitions(&mut definitions, &mut canonical, &mut body);

        let final_names: Vec<Option<String>> = assign_final_names(&definitions, &canonical);
        let resolve = |reference: &str| -> Option<String> {
            let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
            let name: &str = final_names[index].as_deref()?;
            Some(format!("#/$defs/{}{rest}", escape_pointer_token(name)))
        };

        rewrite_refs(&mut body, &resolve);
        let mut defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        for (index, definition) in definitions.into_iter().enumerate() {
            if let (Some(name), Some(mut definition_body)) =
                (final_names[index].clone(), definition.body)
            {
                rewrite_refs(&mut definition_body, &resolve);
                defs.insert(name, definition_body);
            }
        }
        if !defs.is_empty() {
            body.defs.get_or_insert_with(BTreeMap::new).extend(defs);
        }
        body
    }

    /// Embeds a standalone schema as a subschema: its `$defs` (at any depth) become entries of
    /// the generator's root `$defs`, its `$ref`s to them are rewritten, and its `$schema` is
    /// dropped unless it declares its own `$id` (`$schema` must not appear in a subschema that
    /// is not a resource root).
    ///
    /// A `$ref` is matched to a hoisted entry by its JSON Pointer (`#/$defs/Outer/$defs/Inner`).
    /// A `#/$defs/Inner` with no top-level `Inner` resolves to the one nested `Inner` when
    /// exactly one exists. Inlining an equal standalone schema again reuses its entries.
    pub fn inline_subschema(&mut self, schema: JsonSchema) -> JsonSchema {
        let has_defs: bool = schema.defs.as_ref().is_some_and(|defs| !defs.is_empty());
        if !has_defs {
            return drop_schema_keyword_unless_resource_root(schema);
        }
        if let Some(previous) = self
            .inlined
            .iter()
            .find(|previous| previous.standalone == schema)
        {
            return previous.inlined.clone();
        }

        let standalone: JsonSchema = schema.clone();
        let (mut flat, hoisted): (JsonSchema, Vec<HoistedDefinition>) = flatten_defs(schema);
        let mut by_pointer: BTreeMap<String, String> = BTreeMap::new();
        let mut nested_by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut indices: Vec<usize> = Vec::with_capacity(hoisted.len());
        for definition in &hoisted {
            let index: usize = self.definitions.len();
            self.definitions.push(Definition {
                identity: DefinitionIdentity::Hoisted(index),
                name: definition.key.clone(),
                body: None,
            });
            indices.push(index);
            by_pointer.insert(definition.pointer.clone(), provisional_ref(index));
            nested_by_key
                .entry(definition.key.clone())
                .or_default()
                .push(definition.pointer.clone());
        }
        for (key, pointers) in nested_by_key {
            let top_level: String = format!("#/$defs/{}", escape_pointer_token(&key));
            if let [pointer] = pointers.as_slice()
                && !by_pointer.contains_key(&top_level)
            {
                let target: String = by_pointer[pointer].clone();
                by_pointer.insert(top_level, target);
            }
        }

        let resolve = |reference: &str| -> Option<String> {
            by_pointer
                .iter()
                .filter(|(pointer, _)| {
                    reference
                        .strip_prefix(pointer.as_str())
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                })
                .max_by_key(|(pointer, _)| pointer.len())
                .map(|(pointer, target)| format!("{target}{}", &reference[pointer.len()..]))
        };
        rewrite_refs(&mut flat, &resolve);
        for (definition, index) in hoisted.into_iter().zip(indices) {
            let mut body: JsonSchema = definition.body;
            rewrite_refs(&mut body, &resolve);
            self.definitions[index].body = Some(body);
        }

        let inlined: JsonSchema = drop_schema_keyword_unless_resource_root(flat);
        self.inlined.push(InlinedSchema {
            standalone,
            inlined: inlined.clone(),
        });
        inlined
    }

    fn push_type_definition(&mut self, schema_id: &str, name: &str) -> usize {
        let index: usize = self.definitions.len();
        self.definitions.push(Definition {
            identity: DefinitionIdentity::Type(schema_id.to_string()),
            name: name.to_string(),
            body: None,
        });
        self.type_definitions.insert(schema_id.to_string(), index);
        index
    }
}

/// Returns `T`'s schema as a standalone root schema built with a fresh [`SchemaGenerator`]:
/// the subschema from [`ToJsonSchema::json_schema_with`] plus any `$defs` it registered.
#[must_use]
pub fn root_schema_for<T: ToJsonSchema + ?Sized>() -> JsonSchema {
    let mut generator: SchemaGenerator = SchemaGenerator::new();
    let body: JsonSchema = T::json_schema_with(&mut generator);
    generator.into_root_schema(body)
}

fn provisional_ref(index: usize) -> String {
    format!("{PROVISIONAL_REF_PREFIX}{index}")
}

/// Splits a provisional `$ref` into its definition index and the pointer after it.
fn parse_provisional_ref(reference: &str) -> Option<(usize, &str)> {
    let rest: &str = reference.strip_prefix(PROVISIONAL_REF_PREFIX)?;
    let digits: usize = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let index: usize = rest[..digits].parse().ok()?;
    Some((index, &rest[digits..]))
}

/// Escapes a `$defs` key as a JSON Pointer reference token (RFC 6901 §3).
fn escape_pointer_token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn drop_schema_keyword_unless_resource_root(mut schema: JsonSchema) -> JsonSchema {
    if schema.id.is_none() {
        schema.schema = None;
    }
    schema
}

/// Replaces every `$ref` in `schema` (at any depth) for which `resolve` returns a new value.
fn rewrite_refs(schema: &mut JsonSchema, resolve: &impl Fn(&str) -> Option<String>) {
    let mut stack: Vec<&mut JsonSchema> = vec![schema];
    while let Some(current) = stack.pop() {
        if let Some(reference) = current.ref_.as_deref()
            && let Some(resolved) = resolve(reference)
        {
            current.ref_ = Some(resolved);
        }
        stack.extend(current.subschemas_mut());
    }
}

/// A `$defs` entry taken out of a standalone schema by [`flatten_defs`].
#[derive(Debug)]
struct HoistedDefinition {
    /// JSON Pointer of the entry in the standalone schema (`#/$defs/Outer/$defs/Inner`).
    pointer: String,
    key: String,
    body: JsonSchema,
}

/// Removes the `$defs` of `schema` and, transitively, of every `$defs` entry. Returns the
/// schema without them and the entries ordered by pointer.
fn flatten_defs(schema: JsonSchema) -> (JsonSchema, Vec<HoistedDefinition>) {
    let mut root: JsonSchema = schema;
    let mut hoisted: Vec<HoistedDefinition> = Vec::new();
    let mut stack: Vec<(String, BTreeMap<String, JsonSchema>)> = Vec::new();
    if let Some(defs) = root.defs.take() {
        stack.push(("#".to_string(), defs));
    }
    while let Some((parent, defs)) = stack.pop() {
        for (key, mut body) in defs {
            let pointer: String = format!("{parent}/$defs/{}", escape_pointer_token(&key));
            if let Some(nested) = body.defs.take() {
                stack.push((pointer.clone(), nested));
            }
            hoisted.push(HoistedDefinition { pointer, key, body });
        }
    }
    hoisted.sort_by(|a, b| a.pointer.cmp(&b.pointer));
    (root, hoisted)
}

/// Merges each hoisted definition into another live definition with the same name and an
/// equal body (preferring a named type, then the earliest entry), rewriting its `$ref`s.
/// Repeats until nothing merges, since a merge can make two referring bodies equal.
fn merge_equal_hoisted_definitions(
    definitions: &mut [Definition],
    canonical: &mut [usize],
    body: &mut JsonSchema,
) {
    let has_hoisted: bool = definitions
        .iter()
        .any(|definition| matches!(definition.identity, DefinitionIdentity::Hoisted(_)));
    if !has_hoisted {
        return;
    }
    loop {
        let mut merge: Option<(usize, usize)> = None;
        'search: for duplicate in 0..definitions.len() {
            let is_hoisted: bool = matches!(
                definitions[duplicate].identity,
                DefinitionIdentity::Hoisted(_)
            );
            if !is_hoisted || canonical[duplicate] != duplicate {
                continue;
            }
            let Some(duplicate_body) = definitions[duplicate].body.as_ref() else {
                continue;
            };
            for target in 0..definitions.len() {
                let is_preferred: bool = match definitions[target].identity {
                    DefinitionIdentity::Type(_) => true,
                    DefinitionIdentity::Hoisted(_) => target < duplicate,
                };
                if target != duplicate
                    && is_preferred
                    && canonical[target] == target
                    && definitions[target].name == definitions[duplicate].name
                    && definitions[target].body.as_ref() == Some(duplicate_body)
                {
                    merge = Some((duplicate, target));
                    break 'search;
                }
            }
        }
        let Some((duplicate, target)) = merge else {
            return;
        };
        canonical[duplicate] = target;
        definitions[duplicate].body = None;
        let resolve = |reference: &str| -> Option<String> {
            let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
            (index == duplicate).then(|| format!("{}{rest}", provisional_ref(target)))
        };
        rewrite_refs(body, &resolve);
        for definition in definitions.iter_mut() {
            if let Some(definition_body) = definition.body.as_mut() {
                rewrite_refs(definition_body, &resolve);
            }
        }
    }
}

/// Final `$defs` key of each live definition (one with a body that was not merged away).
fn assign_final_names(definitions: &[Definition], canonical: &[usize]) -> Vec<Option<String>> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        if canonical[index] == index && definition.body.is_some() {
            groups.entry(&definition.name).or_default().push(index);
        }
    }
    let mut final_names: Vec<Option<String>> = vec![None; definitions.len()];
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (name, members) in &groups {
        if let [index] = members.as_slice() {
            taken.insert((*name).to_string());
            final_names[*index] = Some((*name).to_string());
        }
    }
    for (name, members) in &mut groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| definitions[*a].identity.cmp(&definitions[*b].identity));
        let mut suffix: usize = 0;
        for index in members.iter() {
            let mut candidate: String = format!("{name}{suffix}");
            while taken.contains(&candidate) {
                suffix += 1;
                candidate = format!("{name}{suffix}");
            }
            suffix += 1;
            taken.insert(candidate.clone());
            final_names[*index] = Some(candidate);
        }
    }
    final_names
}

impl ToJsonSchema for String {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        }
    }
}

impl ToJsonSchema for bool {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("boolean".to_string()),
            ..Default::default()
        }
    }
}

fn integer_schema_with_bounds(min: f64, max: f64) -> JsonSchema {
    JsonSchema {
        type_: Some("integer".to_string()),
        minimum: Some(min),
        maximum: Some(max),
        ..Default::default()
    }
}

impl ToJsonSchema for i8 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(i8::MIN), f64::from(i8::MAX))
    }
}

impl ToJsonSchema for u8 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(u8::MIN), f64::from(u8::MAX))
    }
}

impl ToJsonSchema for i16 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(i16::MIN), f64::from(i16::MAX))
    }
}

impl ToJsonSchema for u16 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(u16::MIN), f64::from(u16::MAX))
    }
}

impl ToJsonSchema for i32 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(i32::MIN), f64::from(i32::MAX))
    }
}

impl ToJsonSchema for u32 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(f64::from(u32::MIN), f64::from(u32::MAX))
    }
}

impl ToJsonSchema for i64 {
    fn json_schema() -> JsonSchema {
        #[expect(clippy::cast_precision_loss)]
        integer_schema_with_bounds(i64::MIN as f64, 9_223_372_036_854_775_807.0_f64)
    }
}

impl ToJsonSchema for u64 {
    fn json_schema() -> JsonSchema {
        integer_schema_with_bounds(0.0_f64, 18_446_744_073_709_551_615.0_f64)
    }
}

fn number_schema_with_bounds(min: f64, max: f64) -> JsonSchema {
    JsonSchema {
        type_: Some("number".to_string()),
        minimum: Some(min),
        maximum: Some(max),
        ..Default::default()
    }
}

impl ToJsonSchema for f32 {
    fn json_schema() -> JsonSchema {
        number_schema_with_bounds(f64::from(f32::MIN), f64::from(f32::MAX))
    }
}

impl ToJsonSchema for f64 {
    fn json_schema() -> JsonSchema {
        number_schema_with_bounds(f64::MIN, f64::MAX)
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Option<T> {
    fn json_schema() -> JsonSchema {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        T::json_schema_with(generator)
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Vec<T> {
    fn json_schema() -> JsonSchema {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(T::json_schema_with(generator))),
            ..Default::default()
        }
    }
}

#[expect(clippy::implicit_hasher)]
impl<T: ToJsonSchema + std::hash::Hash + Eq> ToJsonSchema for std::collections::HashSet<T> {
    fn json_schema() -> JsonSchema {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(T::json_schema_with(generator))),
            unique_items: Some(true),
            ..Default::default()
        }
    }
}

impl<V: ToJsonSchema> ToJsonSchema for BTreeMap<String, V> {
    fn json_schema() -> JsonSchema {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            additional_properties: Some(AdditionalProperties::Schema(Box::new(
                V::json_schema_with(generator),
            ))),
            ..Default::default()
        }
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Box<T> {
    fn json_schema() -> JsonSchema {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        T::json_schema_with(generator)
    }
}

/// Minimal hand-written struct implementing [`ToJsonSchema`] (used to validate trait shape).
#[derive(Debug, Clone)]
pub struct HandWrittenExample;

impl ToJsonSchema for HandWrittenExample {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            title: Some("HandWrittenExample".to_string()),
            ..Default::default()
        }
    }
}

#[cfg(feature = "uuid")]
impl ToJsonSchema for uuid::Uuid {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("string".to_string()),
            format: Some("uuid".to_string()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SchemaGenerator, ToJsonSchema, root_schema_for};
    use crate::json_schema::JsonSchema;
    use std::collections::BTreeMap;

    #[test]
    fn string_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = String::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("boolean".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = bool::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_string_json_schema() {
        let expected: JsonSchema = String::json_schema();
        let actual: JsonSchema = Option::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn i64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            #[expect(clippy::cast_precision_loss)]
            minimum: Some(i64::MIN as f64),
            maximum: Some(9_223_372_036_854_775_807.0_f64),
            ..Default::default()
        };
        let actual: JsonSchema = i64::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_i64_json_schema() {
        let expected: JsonSchema = i64::json_schema();
        let actual: JsonSchema = Option::<i64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn i32_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = i32::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(i32::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(i32::MAX)), actual.maximum);
    }

    #[test]
    fn u32_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u32::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(u32::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(u32::MAX)), actual.maximum);
    }

    #[test]
    fn u64_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u64::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(0.0_f64), actual.minimum);
        assert_eq!(Some(18_446_744_073_709_551_615.0_f64), actual.maximum);
    }

    #[test]
    fn i8_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = i8::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(i8::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(i8::MAX)), actual.maximum);
    }

    #[test]
    fn u8_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u8::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(0.0_f64), actual.minimum);
        assert_eq!(Some(255.0_f64), actual.maximum);
    }

    #[test]
    fn i16_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = i16::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(i16::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(i16::MAX)), actual.maximum);
    }

    #[test]
    fn u16_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u16::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(u16::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(u16::MAX)), actual.maximum);
    }

    #[test]
    fn f64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            minimum: Some(f64::MIN),
            maximum: Some(f64::MAX),
            ..Default::default()
        };
        let actual: JsonSchema = f64::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_f64_json_schema() {
        let expected: JsonSchema = f64::json_schema();
        let actual: JsonSchema = Option::<f64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn f32_json_schema() {
        let expected_type: Option<&str> = Some("number");
        let actual: JsonSchema = f32::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
        assert_eq!(Some(f64::from(f32::MIN)), actual.minimum);
        assert_eq!(Some(f64::from(f32::MAX)), actual.maximum);
    }

    #[test]
    fn hand_written_example_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            title: Some("HandWrittenExample".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = super::HandWrittenExample::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_string_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(String::json_schema())),
            ..Default::default()
        };
        let actual: JsonSchema = Vec::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_i64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(i64::json_schema())),
            ..Default::default()
        };
        let actual: JsonSchema = Vec::<i64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_vec_string_json_schema() {
        let expected: JsonSchema = Vec::<String>::json_schema();
        let actual: JsonSchema = Option::<Vec<String>>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn hash_set_string_json_schema_has_unique_items_true() {
        use std::collections::HashSet;
        let actual: JsonSchema = HashSet::<String>::json_schema();
        let expected_unique: Option<bool> = Some(true);
        assert_eq!(expected_unique, actual.unique_items);
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
    }

    #[test]
    fn vec_string_json_schema_has_unique_items_none() {
        let actual: JsonSchema = Vec::<String>::json_schema();
        let expected_unique: Option<bool> = None;
        assert_eq!(expected_unique, actual.unique_items);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_json_schema() {
        let actual: JsonSchema = uuid::Uuid::json_schema();
        assert_eq!(actual.type_.as_deref(), Some("string"));
        assert_eq!(actual.format.as_deref(), Some("uuid"));
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn option_uuid_json_schema() {
        let actual: JsonSchema = Option::<uuid::Uuid>::json_schema();
        assert_eq!(actual.type_.as_deref(), Some("string"));
        assert_eq!(actual.format.as_deref(), Some("uuid"));
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn vec_uuid_json_schema() {
        let actual: JsonSchema = Vec::<uuid::Uuid>::json_schema();
        assert_eq!(actual.type_.as_deref(), Some("array"));
        let items: &JsonSchema = actual.items.as_ref().expect("items").as_ref();
        assert_eq!(items.type_.as_deref(), Some("string"));
        assert_eq!(items.format.as_deref(), Some("uuid"));
    }

    fn ref_to(name: &str) -> JsonSchema {
        JsonSchema {
            ref_: Some(format!("#/$defs/{name}")),
            ..Default::default()
        }
    }

    fn string_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        }
    }

    fn integer_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("integer".to_string()),
            ..Default::default()
        }
    }

    fn object_schema(properties: &[(&str, JsonSchema)]) -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            properties: properties
                .iter()
                .map(|(key, schema)| ((*key).to_string(), schema.clone()))
                .collect(),
            ..Default::default()
        }
    }

    fn defs(entries: &[(&str, JsonSchema)]) -> BTreeMap<String, JsonSchema> {
        entries
            .iter()
            .map(|(key, schema)| ((*key).to_string(), schema.clone()))
            .collect()
    }

    /// A standalone schema `{"$ref": "#/$defs/<name>", "$defs": {"<name>": body}}`, as a
    /// hand-written `json_schema()` might return.
    fn standalone_with_def(name: &str, body: JsonSchema) -> JsonSchema {
        JsonSchema {
            defs: Some(defs(&[(name, body)])),
            ..ref_to(name)
        }
    }

    #[test]
    fn named_subschema_returns_ref_and_registers_body() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema =
            generator.named_subschema("a::Name", "Name", |_| string_schema());
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Name", string_schema())])),
            ..ref_to("Name")
        };
        let actual: JsonSchema = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_each_schema_id_once() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let mut builds: u32 = 0;
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let expected: u32 = 1;
        let actual: u32 = builds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_same_name_with_different_schema_ids_separately() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let mut builds: u32 = 0;
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let _: JsonSchema = generator.named_subschema("b::Name", "Name", |_| {
            builds += 1;
            integer_schema()
        });
        let expected: u32 = 2;
        let actual: u32 = builds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_returns_ref_for_pending_schema_id_without_building() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema =
            generator.named_subschema("a::Node", "Node", |generator: &mut SchemaGenerator| {
                JsonSchema {
                    items: Some(Box::new(generator.named_subschema(
                        "a::Node",
                        "Node",
                        |_| panic!("a pending schema id must not be rebuilt"),
                    ))),
                    ..Default::default()
                }
            });
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[(
                "Node",
                JsonSchema {
                    items: Some(Box::new(ref_to("Node"))),
                    ..Default::default()
                },
            )])),
            ..ref_to("Node")
        };
        let actual: JsonSchema = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn same_name_different_schema_ids_are_all_suffixed_in_schema_id_order() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Name0", string_schema()),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Name0")), ("second", ref_to("Name1"))])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn suffixed_names_skip_a_name_already_taken() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let taken: JsonSchema = generator.named_subschema("c::Name0", "Name0", |_| JsonSchema {
            type_: Some("boolean".to_string()),
            ..Default::default()
        });
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let body: JsonSchema =
            object_schema(&[("taken", taken), ("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                (
                    "Name0",
                    JsonSchema {
                        type_: Some("boolean".to_string()),
                        ..Default::default()
                    },
                ),
                ("Name1", string_schema()),
                ("Name2", integer_schema()),
            ])),
            ..object_schema(&[
                ("taken", ref_to("Name0")),
                ("first", ref_to("Name1")),
                ("second", ref_to("Name2")),
            ])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_without_back_reference_has_no_defs() {
        let expected: JsonSchema = string_schema();
        let actual: JsonSchema =
            SchemaGenerator::new().into_named_root_schema("a::Name", "Name", |_| string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_with_back_reference_also_defines_root() {
        let body: JsonSchema = JsonSchema {
            items: Some(Box::new(ref_to("Node"))),
            ..Default::default()
        };
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Node", body.clone())])),
            ..body
        };
        let actual: JsonSchema = SchemaGenerator::new().into_named_root_schema(
            "a::Node",
            "Node",
            |generator: &mut SchemaGenerator| JsonSchema {
                items: Some(Box::new(generator.named_subschema(
                    "a::Node",
                    "Node",
                    |_| panic!("a pending schema id must not be rebuilt"),
                ))),
                ..Default::default()
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_root_name_shared_with_nested_type_keeps_nested_unsuffixed() {
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Name", integer_schema())])),
            ..object_schema(&[("nested", ref_to("Name"))])
        };
        let actual: JsonSchema = SchemaGenerator::new().into_named_root_schema(
            "a::Name",
            "Name",
            |generator: &mut SchemaGenerator| {
                object_schema(&[(
                    "nested",
                    generator.named_subschema("b::Name", "Name", |_| integer_schema()),
                )])
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_back_referenced_root_shares_name_with_nested_type() {
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                (
                    "Name0",
                    object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))]),
                ),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))])
        };
        let actual: JsonSchema = SchemaGenerator::new().into_named_root_schema(
            "a::Name",
            "Name",
            |generator: &mut SchemaGenerator| {
                let nested: JsonSchema =
                    generator.named_subschema("b::Name", "Name", |_| integer_schema());
                let this: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
                    panic!("a pending schema id must not be rebuilt")
                });
                object_schema(&[("nested", nested), ("this", this)])
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_defs_and_drops_schema_keyword() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            ..standalone_with_def("Inner", string_schema())
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: JsonSchema = standalone_with_def("Inner", string_schema());
        let actual: JsonSchema = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_keeps_schema_keyword_on_resource_root() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            id: Some("http://example.com/inner".to_string()),
            ..string_schema()
        };
        let expected: JsonSchema = standalone.clone();
        let actual: JsonSchema = SchemaGenerator::new().inline_subschema(standalone);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_flattens_nested_defs_and_resolves_bare_ref_to_unique_nested_def() {
        let outer: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[("b", ref_to("Inner"))])
        };
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Outer", outer)])),
            ..object_schema(&[("a", ref_to("Outer"))])
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner", string_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer"))])
        };
        let actual: JsonSchema = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_resolves_nested_pointer_ref_when_names_conflict() {
        let outer: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", integer_schema())])),
            ..object_schema(&[(
                "b",
                JsonSchema {
                    ref_: Some("#/$defs/Outer/$defs/Inner".to_string()),
                    ..Default::default()
                },
            )])
        };
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema()), ("Outer", outer)])),
            ..object_schema(&[("a", ref_to("Outer")), ("c", ref_to("Inner"))])
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner1"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer")), ("c", ref_to("Inner0"))])
        };
        let actual: JsonSchema = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_rewrites_ref_into_a_def_by_pointer_prefix() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", object_schema(&[("x", string_schema())]))])),
            ref_: Some("#/$defs/Inner/properties/x".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let _: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| integer_schema());
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner0", integer_schema()),
                ("Inner1", object_schema(&[("x", string_schema())])),
            ])),
            ref_: Some("#/$defs/Inner1/properties/x".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_escapes_pointer_tokens_in_def_keys() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        };
        let actual: JsonSchema = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_conflicting_bodies_are_all_suffixed() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let second: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", integer_schema()));
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Inner0")), ("second", ref_to("Inner1"))])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_equal_bodies_from_different_schemas_merge() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let second: JsonSchema = generator.inline_subschema(JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(ref_to("Inner"))),
            defs: Some(defs(&[("Inner", string_schema())])),
            ..Default::default()
        });
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[
                ("first", ref_to("Inner")),
                (
                    "second",
                    JsonSchema {
                        type_: Some("array".to_string()),
                        items: Some(Box::new(ref_to("Inner"))),
                        ..Default::default()
                    },
                ),
            ])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_def_equal_to_named_type_merges_into_it() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let named: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| string_schema());
        let hoisted: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let body: JsonSchema = object_schema(&[("hoisted", hoisted), ("named", named)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[("hoisted", ref_to("Inner")), ("named", ref_to("Inner"))])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_merge_transitively_once_their_references_merge() {
        let outer: JsonSchema = object_schema(&[("inner", ref_to("Inner"))]);
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner", string_schema()),
                ("Outer", outer.clone()),
            ])),
            ..ref_to("Outer")
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(JsonSchema {
            title: Some("Second".to_string()),
            ..standalone
        });
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema()), ("Outer", outer)])),
            ..object_schema(&[
                ("first", ref_to("Outer")),
                (
                    "second",
                    JsonSchema {
                        title: Some("Second".to_string()),
                        ..ref_to("Outer")
                    },
                ),
            ])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inlining_an_equal_recursive_schema_twice_reuses_its_definitions() {
        let standalone: JsonSchema =
            standalone_with_def("List", object_schema(&[("next", ref_to("List"))]));
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(standalone);
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: JsonSchema = JsonSchema {
            defs: Some(defs(&[(
                "List",
                object_schema(&[("next", ref_to("List"))]),
            )])),
            ..object_schema(&[("first", ref_to("List")), ("second", ref_to("List"))])
        };
        let actual: JsonSchema = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_root_schema_without_defs_leaves_defs_none() {
        let expected: JsonSchema = string_schema();
        let actual: JsonSchema = SchemaGenerator::new().into_root_schema(string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_schema_id_is_the_type_name() {
        let expected: &str = std::any::type_name::<String>();
        let actual: &str = String::schema_id();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_of_named_type_root_schema_has_defs() {
        struct Named;
        impl ToJsonSchema for Named {
            fn json_schema() -> JsonSchema {
                root_schema_for::<Self>()
            }

            fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
                generator.named_subschema("a::Named", "Named", |_| string_schema())
            }
        }
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(ref_to("Named"))),
            defs: Some(defs(&[("Named", string_schema())])),
            ..Default::default()
        };
        let actual: JsonSchema = Vec::<Named>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn hand_written_impl_inlines_through_default_json_schema_with() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(super::HandWrittenExample::json_schema())),
            ..Default::default()
        };
        let actual: JsonSchema = Vec::<super::HandWrittenExample>::json_schema();
        assert_eq!(expected, actual);
    }
}
