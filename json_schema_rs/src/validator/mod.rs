//! JSON Schema validation: schema + instance → validation result with all errors.
//!
//! Collects every validation error (no fail-fast) and returns them in a single result.

mod error;
pub use error::{ValidationError, ValidationResult};

use crate::json_pointer::JsonPointer;
use crate::json_schema::JsonSchema;
use serde_json::Value;

/// Validates a JSON instance against a schema. Collects **all** validation errors
/// and returns them in a single result (no fail-fast).
///
/// Validates using the `type` (object, string, integer), `required`, and `properties`
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
pub fn validate(schema: &JsonSchema, instance: &Value) -> ValidationResult {
    let mut errors: Vec<ValidationError> = Vec::new();
    let mut stack: Vec<(&JsonSchema, &Value, JsonPointer)> = Vec::new();
    stack.push((schema, instance, JsonPointer::root()));

    while let Some((schema, instance, instance_path)) = stack.pop() {
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
            }
            Some("integer") => {
                let valid = instance.as_number().is_some_and(|n| n.as_i64().is_some());
                if !valid {
                    errors.push(ValidationError::ExpectedInteger {
                        instance_path: instance_path.clone(),
                    });
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
    use super::{ValidationError, ValidationResult, validate};
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
        };
        let instance = json!("not an object");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_nonempty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
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
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
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
    fn wrong_type_object_with_number() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
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
        };
        for _ in 0..DEPTH {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some("object".to_string()),
                properties: BTreeMap::new(),
                required: Some(vec!["child".to_string()]),
                title: None,
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
}
