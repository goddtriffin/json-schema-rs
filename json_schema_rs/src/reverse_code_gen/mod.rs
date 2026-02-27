//! Reverse codegen: Rust types to JSON Schema.
//!
//! Types implement [`ToJsonSchema`] to produce a [`JsonSchema`] value that can be
//! serialized via [`TryFrom`] to String or `Vec<u8>`.

use crate::json_schema::JsonSchema;
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        }
    }
}

impl ToJsonSchema for bool {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            schema: None,
            id: None,
            type_: Some("boolean".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        }
    }
}

fn integer_schema_with_bounds(min: f64, max: f64) -> JsonSchema {
    JsonSchema {
        schema: None,
        id: None,
        type_: Some("integer".to_string()),
        properties: BTreeMap::new(),
        required: None,
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        const_value: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: Some(min),
        maximum: Some(max),
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
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
        schema: None,
        id: None,
        type_: Some("number".to_string()),
        properties: BTreeMap::new(),
        required: None,
        title: None,
        description: None,
        comment: None,
        enum_values: None,
        const_value: None,
        items: None,
        unique_items: None,
        min_items: None,
        max_items: None,
        minimum: Some(min),
        maximum: Some(max),
        min_length: None,
        max_length: None,
        format: None,
        all_of: None,
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
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(T::json_schema())),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        }
    }
}

#[expect(clippy::implicit_hasher)]
impl<T: ToJsonSchema + std::hash::Hash + Eq> ToJsonSchema for std::collections::HashSet<T> {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(T::json_schema())),
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        }
    }
}

/// Minimal hand-written struct implementing [`ToJsonSchema`] (used to validate trait shape).
#[derive(Debug, Clone)]
pub struct HandWrittenExample;

impl ToJsonSchema for HandWrittenExample {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: Some("HandWrittenExample".to_string()),
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        }
    }
}

#[cfg(feature = "uuid")]
impl ToJsonSchema for uuid::Uuid {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: Some("uuid".to_string()),
            all_of: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ToJsonSchema;
    use crate::json_schema::JsonSchema;

    #[test]
    fn string_json_schema() {
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        };
        let actual: JsonSchema = String::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_json_schema() {
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("boolean".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
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
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            #[expect(clippy::cast_precision_loss)]
            minimum: Some(i64::MIN as f64),
            maximum: Some(9_223_372_036_854_775_807.0_f64),
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
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
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(f64::MIN),
            maximum: Some(f64::MAX),
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
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
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: Some("HandWrittenExample".to_string()),
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        };
        let actual: JsonSchema = super::HandWrittenExample::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_string_json_schema() {
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(String::json_schema())),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
        };
        let actual: JsonSchema = Vec::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_i64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(i64::json_schema())),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            format: None,
            all_of: None,
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
}
