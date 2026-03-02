//! Reverse codegen: Rust types to JSON Schema.
//!
//! Types implement [`ToJsonSchema`] to produce a [`JsonSchema`] value that can be
//! serialized via [`TryFrom`] to String or `Vec<u8>`.

use crate::json_schema::JsonSchema;
use crate::json_schema::json_schema::AdditionalProperties;
use std::collections::BTreeMap;

/// Trait for types that can produce a JSON Schema.
///
/// Implemented for primitive/standard types (e.g. `String`, `Option<T>`) and for
/// structs via `#[derive(ToJsonSchema)]` with optional container/field attributes.
pub trait ToJsonSchema {
    /// Returns the JSON Schema for this type.
    fn json_schema() -> JsonSchema;
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
        T::json_schema()
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Vec<T> {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(T::json_schema())),
            ..Default::default()
        }
    }
}

#[expect(clippy::implicit_hasher)]
impl<T: ToJsonSchema + std::hash::Hash + Eq> ToJsonSchema for std::collections::HashSet<T> {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("array".to_string()),
            items: Some(Box::new(T::json_schema())),
            unique_items: Some(true),
            ..Default::default()
        }
    }
}

impl<V: ToJsonSchema> ToJsonSchema for BTreeMap<String, V> {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            additional_properties: Some(AdditionalProperties::Schema(Box::new(V::json_schema()))),
            ..Default::default()
        }
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Box<T> {
    fn json_schema() -> JsonSchema {
        T::json_schema()
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
    use super::ToJsonSchema;
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
