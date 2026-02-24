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
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
        }
    }
}

impl ToJsonSchema for bool {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("boolean".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
        }
    }
}

fn integer_schema() -> JsonSchema {
    JsonSchema {
        type_: Some("integer".to_string()),
        properties: BTreeMap::new(),
        required: None,
        title: None,
        description: None,
        enum_values: None,
        items: None,
    }
}

impl ToJsonSchema for i8 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for u8 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for i16 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for u16 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for i32 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for u32 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for i64 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

impl ToJsonSchema for u64 {
    fn json_schema() -> JsonSchema {
        integer_schema()
    }
}

fn number_schema() -> JsonSchema {
    JsonSchema {
        type_: Some("number".to_string()),
        properties: BTreeMap::new(),
        required: None,
        title: None,
        description: None,
        enum_values: None,
        items: None,
    }
}

impl ToJsonSchema for f32 {
    fn json_schema() -> JsonSchema {
        number_schema()
    }
}

impl ToJsonSchema for f64 {
    fn json_schema() -> JsonSchema {
        number_schema()
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
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(T::json_schema())),
        }
    }
}

/// Minimal hand-written struct implementing [`ToJsonSchema`] (used to validate trait shape).
#[derive(Debug, Clone)]
pub struct HandWrittenExample;

impl ToJsonSchema for HandWrittenExample {
    fn json_schema() -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: Some("HandWrittenExample".to_string()),
            description: None,
            enum_values: None,
            items: None,
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
            type_: Some("string".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
        };
        let actual: JsonSchema = String::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("boolean".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
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
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
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
    }

    #[test]
    fn u32_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u32::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn u64_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u64::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn i8_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = i8::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn u8_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u8::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn i16_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = i16::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn u16_json_schema() {
        let expected_type: Option<&str> = Some("integer");
        let actual: JsonSchema = u16::json_schema();
        assert_eq!(expected_type, actual.type_.as_deref());
    }

    #[test]
    fn f64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
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
    }

    #[test]
    fn hand_written_example_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: Some("HandWrittenExample".to_string()),
            description: None,
            enum_values: None,
            items: None,
        };
        let actual: JsonSchema = super::HandWrittenExample::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_string_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(String::json_schema())),
        };
        let actual: JsonSchema = Vec::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_i64_json_schema() {
        let expected: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: std::collections::BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(i64::json_schema())),
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
}
