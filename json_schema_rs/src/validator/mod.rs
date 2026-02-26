//! JSON Schema validation: schema + instance → validation result with all errors.
//!
//! Collects every validation error (no fail-fast) and returns them in a single result.

mod error;
pub use error::{OrderedF64, ValidationError, ValidationResult};

use crate::json_pointer::JsonPointer;
use crate::json_schema::JsonSchema;
use serde_json::Value;

/// Validates a JSON instance against a schema. Collects **all** validation errors
/// and returns them in a single result (no fail-fast).
///
/// Validates using the `type` (object, string, integer, number), `required`, and `properties`
/// keywords. Does not resolve `$ref` or `$defs`; additional properties are allowed.
///
/// # Errors
///
/// Returns `Err(errors)` when the instance does not conform to the schema, with
/// one or more [`ValidationError`] values describing each failure.
///
/// # Example
///
/// ```
/// use json_schema_rs::{JsonSchema, validate};
/// use serde_json::json;
///
/// let schema: JsonSchema = serde_json::from_str(r#"{"type":"object","properties":{"name":{"type":"string"}}}"#).unwrap();
/// let instance = json!({"name": "Alice"});
/// let result = validate(&schema, &instance);
/// assert!(result.is_ok());
/// ```
#[expect(clippy::too_many_lines)]
pub fn validate(schema: &JsonSchema, instance: &Value) -> ValidationResult {
    let mut errors: Vec<ValidationError> = Vec::new();
    let mut stack: Vec<(&JsonSchema, &Value, JsonPointer)> = Vec::new();
    stack.push((schema, instance, JsonPointer::root()));

    while let Some((schema, instance, instance_path)) = stack.pop() {
        if let Some(ref allowed) = schema.enum_values
            && !allowed.is_empty()
            && !allowed.iter().any(|a| a == instance)
        {
            errors.push(ValidationError::NotInEnum {
                instance_path: instance_path.clone(),
            });
            continue;
        }
        let expected_type: Option<&str> = schema.type_.as_deref();
        match expected_type {
            Some("object") => {
                let Some(obj) = instance.as_object() else {
                    errors.push(ValidationError::ExpectedObject {
                        instance_path: instance_path.clone(),
                    });
                    continue;
                };
                if let Some(ref required) = schema.required {
                    for name in required {
                        if !obj.contains_key(name) {
                            errors.push(ValidationError::MissingRequired {
                                instance_path: instance_path.push(name),
                                property: name.clone(),
                            });
                        }
                    }
                }
                // Push in reverse order so we pop in schema properties order (first key first).
                let mut pending: Vec<(&JsonSchema, &Value, JsonPointer)> = Vec::new();
                for (key, sub_schema) in &schema.properties {
                    if let Some(value) = obj.get(key) {
                        let path = instance_path.push(key);
                        pending.push((sub_schema, value, path));
                    }
                }
                for item in pending.into_iter().rev() {
                    stack.push(item);
                }
            }
            Some("string") => {
                if !instance.is_string() {
                    errors.push(ValidationError::ExpectedString {
                        instance_path: instance_path.clone(),
                    });
                }
                // minLength / maxLength: count Unicode code points (chars), not bytes.
                if let Some(s) = instance.as_str() {
                    let char_count: u64 = s.chars().count() as u64;
                    if let Some(min_length) = schema.min_length
                        && char_count < min_length
                    {
                        errors.push(ValidationError::TooShort {
                            instance_path: instance_path.clone(),
                            min_length,
                        });
                    }
                    if let Some(max_length) = schema.max_length
                        && char_count > max_length
                    {
                        errors.push(ValidationError::TooLong {
                            instance_path: instance_path.clone(),
                            max_length,
                        });
                    }
                }
            }
            Some("integer") => {
                let valid = instance.as_number().is_some_and(|n| n.as_i64().is_some());
                if !valid {
                    errors.push(ValidationError::ExpectedInteger {
                        instance_path: instance_path.clone(),
                    });
                } else if let Some(instance_f64) =
                    instance.as_number().and_then(serde_json::Number::as_f64)
                {
                    if let Some(min) = schema.minimum
                        && instance_f64 < min
                    {
                        errors.push(ValidationError::BelowMinimum {
                            instance_path: instance_path.clone(),
                            minimum: crate::validator::error::OrderedF64(min),
                        });
                    }
                    if let Some(max) = schema.maximum
                        && instance_f64 > max
                    {
                        errors.push(ValidationError::AboveMaximum {
                            instance_path: instance_path.clone(),
                            maximum: crate::validator::error::OrderedF64(max),
                        });
                    }
                }
            }
            Some("number") => {
                let valid = instance.as_number().is_some();
                if !valid {
                    errors.push(ValidationError::ExpectedNumber {
                        instance_path: instance_path.clone(),
                    });
                } else if let Some(instance_f64) =
                    instance.as_number().and_then(serde_json::Number::as_f64)
                {
                    if let Some(min) = schema.minimum
                        && instance_f64 < min
                    {
                        errors.push(ValidationError::BelowMinimum {
                            instance_path: instance_path.clone(),
                            minimum: crate::validator::error::OrderedF64(min),
                        });
                    }
                    if let Some(max) = schema.maximum
                        && instance_f64 > max
                    {
                        errors.push(ValidationError::AboveMaximum {
                            instance_path: instance_path.clone(),
                            maximum: crate::validator::error::OrderedF64(max),
                        });
                    }
                }
            }
            Some("array") => {
                let Some(arr) = instance.as_array() else {
                    errors.push(ValidationError::ExpectedArray {
                        instance_path: instance_path.clone(),
                    });
                    continue;
                };
                if let Some(min_items) = schema.min_items
                    && arr.len() < min_items.try_into().unwrap_or(usize::MAX)
                {
                    errors.push(ValidationError::TooFewItems {
                        instance_path: instance_path.clone(),
                        min_items,
                    });
                }
                if let Some(max_items) = schema.max_items
                    && arr.len() > max_items.try_into().unwrap_or(0)
                {
                    errors.push(ValidationError::TooManyItems {
                        instance_path: instance_path.clone(),
                        max_items,
                    });
                }
                if schema.unique_items == Some(true) {
                    let mut has_duplicate = false;
                    for i in 0..arr.len() {
                        for j in (i + 1)..arr.len() {
                            if arr[i] == arr[j] {
                                has_duplicate = true;
                                break;
                            }
                        }
                        if has_duplicate {
                            break;
                        }
                    }
                    if has_duplicate {
                        errors.push(ValidationError::DuplicateArrayItems {
                            instance_path: instance_path.clone(),
                        });
                    }
                }
                if let Some(ref item_schema) = schema.items {
                    let mut pending: Vec<(&JsonSchema, &Value, JsonPointer)> = Vec::new();
                    for (i, elem) in arr.iter().enumerate() {
                        let path = instance_path.push(&i.to_string());
                        pending.push((item_schema, elem, path));
                    }
                    for item in pending.into_iter().rev() {
                        stack.push(item);
                    }
                }
            }
            None | Some(_) => {
                // Type absent or not enforced: validate required/properties when instance is object
                if let Some(obj) = instance.as_object() {
                    if let Some(ref required) = schema.required {
                        for name in required {
                            if !obj.contains_key(name) {
                                errors.push(ValidationError::MissingRequired {
                                    instance_path: instance_path.push(name),
                                    property: name.clone(),
                                });
                            }
                        }
                    }
                    let mut pending: Vec<(&JsonSchema, &Value, JsonPointer)> = Vec::new();
                    for (key, sub_schema) in &schema.properties {
                        if let Some(value) = obj.get(key) {
                            let path = instance_path.push(key);
                            pending.push((sub_schema, value, path));
                        }
                    }
                    for item in pending.into_iter().rev() {
                        stack.push(item);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::{OrderedF64, ValidationError, ValidationResult, validate};
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::JsonSchema;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema_object_with_required(
        required: Vec<&str>,
        properties: BTreeMap<String, JsonSchema>,
    ) -> JsonSchema {
        JsonSchema {
            type_: Some("object".to_string()),
            properties,
            required: Some(required.into_iter().map(String::from).collect()),
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        }
    }

    #[test]
    fn valid_object_with_required_and_properties() {
        let schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"a": "x", "b": "y"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn missing_required_property() {
        let schema = schema_object_with_required(vec!["name"], {
            let mut m = BTreeMap::new();
            m.insert(
                "name".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("name"),
            property: "name".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_string_instead_of_object() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("not an object");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_description_validates_as_before() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        properties: BTreeMap::new(),
                        required: None,
                        title: None,
                        description: Some("User name".to_string()),
                        enum_values: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: None,
                        maximum: None,
                        min_length: None,
                        max_length: None,
                    },
                );
                m
            },
            required: None,
            title: None,
            description: Some("Root type".to_string()),
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let valid_instance = json!({"name": "Alice"});
        let actual_valid: ValidationResult = validate(&schema, &valid_instance);
        assert_eq!(Ok(()), actual_valid);
        let invalid_instance = json!({"name": 42});
        let actual_invalid: ValidationResult = validate(&schema, &invalid_instance);
        assert!(actual_invalid.is_err());
    }

    #[test]
    fn root_type_string_valid_nonempty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("ok");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!({"x": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_number_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_null_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_boolean_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_array_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_valid_instance_in_allowed() {
        let schema: JsonSchema = JsonSchema {
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("open");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_invalid_instance_not_in_allowed() {
        let schema: JsonSchema = JsonSchema {
            type_: None,
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("pending");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::NotInEnum {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_with_type_string_valid() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("a");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_with_type_string_invalid_not_in_enum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("c");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::NotInEnum {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_42() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_0() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_negative() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(-1);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_float() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("42");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_null() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_object() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!({"x": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_array() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_bool() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_property_type_integer_valid() {
        let schema = schema_object_with_required(vec!["count"], {
            let mut m = BTreeMap::new();
            m.insert(
                "count".to_string(),
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"count": 10});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_property_type_integer_invalid_float() {
        let schema = schema_object_with_required(vec!["count"], {
            let mut m = BTreeMap::new();
            m.insert(
                "count".to_string(),
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"count": 2.5});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("count"),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_required_integer_missing() {
        let schema = schema_object_with_required(vec!["count"], {
            let mut m = BTreeMap::new();
            m.insert(
                "count".to_string(),
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("count"),
            property: "count".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_valid_float() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_valid_integer() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("3.14");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_null() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_object() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_array() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1.0]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_bool() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_minimum_maximum_valid_in_range() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(0.0),
            maximum: Some(255.0),
            min_length: None,
            max_length: None,
        };
        let instance = json!(100);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(10.0),
            maximum: Some(100.0),
            min_length: None,
            max_length: None,
        };
        let instance = json!(5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: OrderedF64(10.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(0.0),
            maximum: Some(10.0),
            min_length: None,
            max_length: None,
        };
        let instance = json!(20);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: OrderedF64(10.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_no_minimum_maximum_no_extra_errors() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_valid_in_range() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(0.5),
            maximum: Some(99.5),
            min_length: None,
            max_length: None,
        };
        let instance = json!(50.0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(1.0),
            maximum: Some(10.0),
            min_length: None,
            max_length: None,
        };
        let instance = json!(0.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: OrderedF64(1.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: Some(0.0),
            maximum: Some(1.0),
            min_length: None,
            max_length: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: OrderedF64(1.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_and_number_min_max_violations_collected_from_multiple_properties() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "low".to_string(),
                    JsonSchema {
                        type_: Some("integer".to_string()),
                        properties: BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        enum_values: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: Some(10.0),
                        maximum: Some(100.0),
                        min_length: None,
                        max_length: None,
                    },
                );
                m.insert(
                    "high".to_string(),
                    JsonSchema {
                        type_: Some("integer".to_string()),
                        properties: BTreeMap::new(),
                        required: None,
                        title: None,
                        description: None,
                        enum_values: None,
                        items: None,
                        unique_items: None,
                        min_items: None,
                        max_items: None,
                        minimum: Some(10.0),
                        maximum: Some(100.0),
                        min_length: None,
                        max_length: None,
                    },
                );
                m
            },
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!({"low": 5, "high": 200});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![
            ValidationError::AboveMaximum {
                instance_path: JsonPointer::root().push("high"),
                maximum: OrderedF64(100.0),
            },
            ValidationError::BelowMinimum {
                instance_path: JsonPointer::root().push("low"),
                minimum: OrderedF64(10.0),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_non_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_invalid_not_array() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("not an array");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedArray {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_with_items_valid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_with_items_invalid_element() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", 42, "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root().push("1"),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_no_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_duplicates_invalid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", "b", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected_has_duplicate_error: bool = actual.as_ref().err().is_some_and(|e| {
            e.iter()
                .any(|err| matches!(err, ValidationError::DuplicateArrayItems { .. }))
        });
        assert!(
            expected_has_duplicate_error,
            "expected DuplicateArrayItems: {actual:?}"
        );
    }

    #[test]
    fn unique_items_false_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(false),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_absent_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_empty_array_valid() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(3),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooFewItems {
            instance_path: JsonPointer::root(),
            min_items: 3,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_edge_len_equals_min() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(2),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooManyItems {
            instance_path: JsonPointer::root(),
            max_items: 2,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_edge_len_equals_max() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(2),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_both_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_fail_too_few() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooFewItems {
            instance_path: JsonPointer::root(),
            min_items: 2,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_fail_too_many() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3, 4, 5, 6]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooManyItems {
            instance_path: JsonPointer::root(),
            max_items: 5,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_absent_unchanged() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_not_array_expected_array_only() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!("not an array");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedArray {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_property_type_number_valid() {
        let schema = schema_object_with_required(vec!["value"], {
            let mut m = BTreeMap::new();
            m.insert(
                "value".to_string(),
                JsonSchema {
                    type_: Some("number".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"value": 2.5});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_property_type_number_invalid_string() {
        let schema = schema_object_with_required(vec!["value"], {
            let mut m = BTreeMap::new();
            m.insert(
                "value".to_string(),
                JsonSchema {
                    type_: Some("number".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"value": "3.14"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root().push("value"),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_required_number_missing() {
        let schema = schema_object_with_required(vec!["value"], {
            let mut m = BTreeMap::new();
            m.insert(
                "value".to_string(),
                JsonSchema {
                    type_: Some("number".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("value"),
            property: "value".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_with_number() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_with_null() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_with_array() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_type_but_properties_object_instance_valid() {
        let schema: JsonSchema = JsonSchema {
            type_: None,
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: Some(vec!["name".to_string()]),
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        let instance = json!({"name": "Alice"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_object_validation() {
        let schema = schema_object_with_required(vec!["address"], {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "city".to_string(),
                            JsonSchema {
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        inner
                    },
                    required: Some(vec!["city".to_string()]),
                    title: None,
                    description: None,
                    enum_values: None,
                    items: None,
                    unique_items: None,
                    min_items: None,
                    max_items: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                },
            );
            m
        });
        let instance = json!({"address": {}});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("address").push("city"),
            property: "city".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_property_absent_valid() {
        let schema = schema_object_with_required(vec![], {
            let mut m = BTreeMap::new();
            m.insert(
                "opt".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_failures_all_errors_returned() {
        let schema = schema_object_with_required(vec!["a", "b", "c"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "c".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![
            ValidationError::MissingRequired {
                instance_path: JsonPointer::root().push("a"),
                property: "a".to_string(),
            },
            ValidationError::MissingRequired {
                instance_path: JsonPointer::root().push("b"),
                property: "b".to_string(),
            },
            ValidationError::MissingRequired {
                instance_path: JsonPointer::root().push("c"),
                property: "c".to_string(),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_failures_type_and_required_and_nested() {
        let schema = schema_object_with_required(vec!["x", "nested"], {
            let mut m = BTreeMap::new();
            m.insert(
                "x".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "nested".to_string(),
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "y".to_string(),
                            JsonSchema {
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        inner
                    },
                    required: Some(vec!["y".to_string()]),
                    title: None,
                    description: None,
                    enum_values: None,
                    items: None,
                    unique_items: None,
                    min_items: None,
                    max_items: None,
                    minimum: None,
                    maximum: None,
                    min_length: None,
                    max_length: None,
                },
            );
            m
        });
        let instance = json!({"nested": {}});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![
            ValidationError::MissingRequired {
                instance_path: JsonPointer::root().push("x"),
                property: "x".to_string(),
            },
            ValidationError::MissingRequired {
                instance_path: JsonPointer::root().push("nested").push("y"),
                property: "y".to_string(),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn deeply_nested_instance_does_not_stack_overflow() {
        const DEPTH: usize = 200;
        let mut inner: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: Some(vec!["value".to_string()]),
            title: None,
            description: None,
            enum_values: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
        };
        for _ in 0..DEPTH {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some("object".to_string()),
                properties: BTreeMap::new(),
                required: Some(vec!["child".to_string()]),
                title: None,
                description: None,
                enum_values: None,
                items: None,
                unique_items: None,
                min_items: None,
                max_items: None,
                minimum: None,
                maximum: None,
                min_length: None,
                max_length: None,
            };
            wrap.properties.insert("child".to_string(), inner);
            inner = wrap;
        }
        let mut instance_value: serde_json::Value = {
            let mut leaf = serde_json::Map::new();
            leaf.insert(
                "value".to_string(),
                serde_json::Value::String("ok".to_string()),
            );
            serde_json::Value::Object(leaf)
        };
        for _ in 0..DEPTH {
            let mut obj = serde_json::Map::new();
            obj.insert("child".to_string(), instance_value);
            instance_value = serde_json::Value::Object(obj);
        }
        let actual: ValidationResult = validate(&inner, &instance_value);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn pointer_escaping() {
        let schema = schema_object_with_required(vec!["a/b"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a/b".to_string(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"a/b": 123});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::try_from("/a~1b").unwrap(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_exact_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(3),
            ..Default::default()
        };
        let instance = json!("abc");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_below_fails() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(5),
            ..Default::default()
        };
        let instance = json!("hi");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooShort {
            instance_path: JsonPointer::root(),
            min_length: 5,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_above_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(2),
            ..Default::default()
        };
        let instance = json!("hello");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_absent_any_length_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_exact_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            max_length: Some(5),
            ..Default::default()
        };
        let instance = json!("hello");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_above_fails() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            max_length: Some(3),
            ..Default::default()
        };
        let instance = json!("hello");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooLong {
            instance_path: JsonPointer::root(),
            max_length: 3,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_max_length_below_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            max_length: Some(10),
            ..Default::default()
        };
        let instance = json!("hi");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_absent_any_length_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let instance = json!("this is a very long string with no max length constraint");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_zero_allows_empty() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(0),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_zero_requires_empty() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            max_length: Some(0),
            ..Default::default()
        };
        let instance = json!("x");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooLong {
            instance_path: JsonPointer::root(),
            max_length: 0,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_max_length_zero_empty_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            max_length: Some(0),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_both_constraints_within_range_passes() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(2),
            max_length: Some(10),
            ..Default::default()
        };
        let instance = json!("hello");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_below_min_length_with_max_also_set() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(5),
            max_length: Some(10),
            ..Default::default()
        };
        let instance = json!("hi");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooShort {
            instance_path: JsonPointer::root(),
            min_length: 5,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_above_max_length_with_min_also_set() {
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(2),
            max_length: Some(4),
            ..Default::default()
        };
        let instance = json!("hello world");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooLong {
            instance_path: JsonPointer::root(),
            max_length: 4,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_unicode_code_points() {
        // "日本語" has 3 Unicode code points but 9 UTF-8 bytes.
        let schema = JsonSchema {
            type_: Some("string".to_string()),
            min_length: Some(3),
            max_length: Some(3),
            ..Default::default()
        };
        let instance = json!("日本語");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }
}
