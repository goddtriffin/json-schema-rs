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
}

/// Generator context for one root schema: owns the `$defs` map and the set of named types
/// whose body is being built.
///
/// A named type found in the pending set returns its `$ref` immediately, so a cycle of any
/// length between derived types terminates. Descent depth is bounded by the number of
/// distinct named types (each is built at most once per generator) plus the nesting of
/// container types written in the source.
#[derive(Debug, Default)]
pub struct SchemaGenerator {
    defs: BTreeMap<String, JsonSchema>,
    pending: BTreeSet<String>,
    referenced_while_pending: BTreeSet<String>,
}

impl SchemaGenerator {
    /// Creates a generator with no definitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a `$ref` to the definition `name`, building its body with `build` the first
    /// time `name` is requested.
    ///
    /// When `name` is already being built (a recursive reference), the `$ref` is returned
    /// without calling `build`.
    pub fn named_subschema(
        &mut self,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        if self.pending.contains(name) {
            self.referenced_while_pending.insert(name.to_string());
        } else if !self.defs.contains_key(name) {
            self.pending.insert(name.to_string());
            let body: JsonSchema = build(self);
            self.pending.remove(name);
            self.defs.insert(name.to_string(), body);
        }
        JsonSchema {
            ref_: Some(format!("#/$defs/{name}")),
            ..Default::default()
        }
    }

    /// Builds the root schema for the named type `name` from `build`.
    ///
    /// The root body is returned inline. It is also placed in `$defs` when a nested type
    /// refers back to it, so that `$ref` resolves.
    #[must_use]
    pub fn into_named_root_schema(
        mut self,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        self.pending.insert(name.to_string());
        let body: JsonSchema = build(&mut self);
        self.pending.remove(name);
        if self.referenced_while_pending.contains(name) {
            self.defs
                .entry(name.to_string())
                .or_insert_with(|| body.clone());
        }
        self.into_root_schema(body)
    }

    /// Attaches the collected `$defs` (if any) to `body` and returns it as the root schema.
    #[must_use]
    pub fn into_root_schema(self, mut body: JsonSchema) -> JsonSchema {
        if !self.defs.is_empty() {
            body.defs
                .get_or_insert_with(BTreeMap::new)
                .extend(self.defs);
        }
        body
    }

    /// Embeds a standalone schema as a subschema: its `$defs` (at any depth) move to the
    /// generator's root `$defs`, and its `$schema` is dropped unless it declares its own
    /// `$id` (`$schema` must not appear in a subschema that is not a resource root).
    pub fn inline_subschema(&mut self, schema: JsonSchema) -> JsonSchema {
        let mut flat: JsonSchema = merge_nested_defs_into_root(schema, &mut self.defs);
        if flat.id.is_none() {
            flat.schema = None;
        }
        flat
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

/// Merges nested `$defs` from `schema` into `root_defs`, returns schema with `defs: None`.
///
/// Recursively flattens so the final result has a single root-level defs map.
/// Uses an explicit stack (no recursion) per the "no literal recursion" design principle.
///
/// When a schema has `defs: { Outer: { ..., defs: { Inner } } }`, this function merges
/// `Inner` and `Outer` into `root_defs` and returns the schema with `defs: None`.
/// The `$ref` values in properties/items already point to `#/$defs/Name` and will
/// resolve against the root.
///
/// # Panics
///
/// Never panics. The `unwrap` on `defs.take()` is safe because we only reach that
/// branch when `schema.defs` is `Some` and non-empty.
pub fn merge_nested_defs_into_root(
    schema: JsonSchema,
    root_defs: &mut BTreeMap<String, JsonSchema>,
) -> JsonSchema {
    let mut stack: Vec<(Option<String>, JsonSchema)> = Vec::new();
    stack.push((None, schema));

    let mut result: Option<JsonSchema> = None;

    while let Some((key_opt, mut s)) = stack.pop() {
        let has_nested_defs: bool = s.defs.as_ref().is_some_and(|m| !m.is_empty());

        if has_nested_defs {
            let defs: BTreeMap<String, JsonSchema> = s.defs.take().unwrap();
            stack.push((key_opt, s));
            for (k, v) in defs.into_iter().rev() {
                stack.push((Some(k), v));
            }
        } else if let Some(k) = key_opt {
            root_defs.entry(k).or_insert(s);
        } else {
            result = Some(s);
        }
    }

    result.expect("root schema must have been processed")
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

    #[test]
    fn named_subschema_returns_ref_and_registers_body() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.named_subschema("Name", |_| string_schema());
        let expected: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([("Name".to_string(), string_schema())])),
            ..ref_to("Name")
        };
        let actual: JsonSchema = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_each_name_once() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let mut builds: u32 = 0;
        let _: JsonSchema = generator.named_subschema("Name", |_| {
            builds += 1;
            string_schema()
        });
        let _: JsonSchema = generator.named_subschema("Name", |_| {
            builds += 1;
            string_schema()
        });
        let expected: u32 = 1;
        let actual: u32 = builds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_returns_ref_for_pending_name_without_building() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.named_subschema("Node", |generator| JsonSchema {
            items: Some(Box::new(generator.named_subschema("Node", |_| {
                panic!("a pending name must not be rebuilt")
            }))),
            ..Default::default()
        });
        let expected: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([(
                "Node".to_string(),
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
    fn into_named_root_schema_without_back_reference_has_no_defs() {
        let expected: JsonSchema = string_schema();
        let actual: JsonSchema =
            SchemaGenerator::new().into_named_root_schema("Name", |_| string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_with_back_reference_also_defines_root() {
        let body: JsonSchema = JsonSchema {
            items: Some(Box::new(ref_to("Node"))),
            ..Default::default()
        };
        let expected: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([("Node".to_string(), body.clone())])),
            ..body
        };
        let actual: JsonSchema =
            SchemaGenerator::new().into_named_root_schema("Node", |generator| JsonSchema {
                items: Some(Box::new(generator.named_subschema("Node", |_| {
                    panic!("a pending name must not be rebuilt")
                }))),
                ..Default::default()
            });
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_defs_and_drops_schema_keyword() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            defs: Some(BTreeMap::from([("Inner".to_string(), string_schema())])),
            ..ref_to("Inner")
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([("Inner".to_string(), string_schema())])),
            ..ref_to("Inner")
        };
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
    fn into_root_schema_without_defs_leaves_defs_none() {
        let expected: JsonSchema = string_schema();
        let actual: JsonSchema = SchemaGenerator::new().into_root_schema(string_schema());
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
                generator.named_subschema("Named", |_| string_schema())
            }
        }
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(ref_to("Named"))),
            defs: Some(BTreeMap::from([("Named".to_string(), string_schema())])),
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

    #[test]
    fn merge_nested_defs_into_root_flattens() {
        use super::merge_nested_defs_into_root;

        let inner_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let mut outer_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        outer_defs.insert("Inner".to_string(), inner_schema.clone());
        let outer_schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "b".to_string(),
                    JsonSchema {
                        ref_: Some("#/$defs/Inner".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            defs: Some(outer_defs),
            ..Default::default()
        };
        let mut schema_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        schema_defs.insert("Outer".to_string(), outer_schema);
        let schema_with_nested: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        ref_: Some("#/$defs/Outer".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            defs: Some(schema_defs),
            ..Default::default()
        };

        let mut root_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        let actual: JsonSchema = merge_nested_defs_into_root(schema_with_nested, &mut root_defs);

        let expected_returned: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            defs: None,
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        ref_: Some("#/$defs/Outer".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            ..Default::default()
        };
        let mut expected_root_defs: BTreeMap<String, JsonSchema> = BTreeMap::new();
        expected_root_defs.insert("Inner".to_string(), inner_schema);
        expected_root_defs.insert(
            "Outer".to_string(),
            JsonSchema {
                type_: Some("object".to_string()),
                defs: None,
                properties: {
                    let mut m = BTreeMap::new();
                    m.insert(
                        "b".to_string(),
                        JsonSchema {
                            ref_: Some("#/$defs/Inner".to_string()),
                            ..Default::default()
                        },
                    );
                    m
                },
                ..Default::default()
            },
        );

        assert_eq!((expected_returned, expected_root_defs), (actual, root_defs));
    }
}
