//! JSON Schema validation: schema + instance → validation result with all errors.
//!
//! Collects every validation error (no fail-fast) and returns them in a single result.

use crate::json_pointer::JsonPointer;
use crate::schema::Schema;
use serde_json::Value;
use std::fmt;

/// A single validation failure: kind and instance location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Schema had `type: "object"` but the instance was not an object.
    ExpectedObject {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// Schema had `type: "string"` but the instance was not a string.
    ExpectedString {
        /// JSON Pointer to the instance that failed.
        instance_path: JsonPointer,
    },
    /// A property listed in `required` was absent.
    MissingRequired {
        /// JSON Pointer to the object (parent of the missing property).
        instance_path: JsonPointer,
        /// The required property name that was missing.
        property: String,
    },
}

impl ValidationError {
    /// Returns the instance path for this error.
    #[must_use]
    pub fn instance_path(&self) -> &JsonPointer {
        match self {
            ValidationError::ExpectedObject { instance_path }
            | ValidationError::ExpectedString { instance_path }
            | ValidationError::MissingRequired { instance_path, .. } => instance_path,
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let location = self.instance_path().display_root_or_path();
        match self {
            ValidationError::ExpectedObject { .. } => {
                write!(f, "{location}: expected object")
            }
            ValidationError::ExpectedString { .. } => {
                write!(f, "{location}: expected string")
            }
            ValidationError::MissingRequired { property, .. } => {
                write!(f, "{location}: missing required property \"{property}\"")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Result of validation: `Ok(())` when valid, `Err(errors)` when invalid.
pub type ValidationResult = Result<(), Vec<ValidationError>>;

/// Validates a JSON instance against a schema. Collects **all** validation errors
/// and returns them in a single result (no fail-fast).
///
/// Validates using the `type` (object, string), `required`, and `properties`
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
/// use json_schema_rs::{Schema, validate};
/// use serde_json::json;
///
/// let schema: Schema = serde_json::from_str(r#"{"type":"object","properties":{"name":{"type":"string"}}}"#).unwrap();
/// let instance = json!({"name": "Alice"});
/// let result = validate(&schema, &instance);
/// assert!(result.is_ok());
/// ```
pub fn validate(schema: &Schema, instance: &Value) -> ValidationResult {
    let mut errors: Vec<ValidationError> = Vec::new();
    let mut stack: Vec<(&Schema, &Value, JsonPointer)> = Vec::new();
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
                let mut pending: Vec<(&Schema, &Value, JsonPointer)> = Vec::new();
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
                    let mut pending: Vec<(&Schema, &Value, JsonPointer)> = Vec::new();
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
    use super::{ValidationError, validate};
    use crate::json_pointer::JsonPointer;
    use crate::schema::Schema;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema_object_with_required(
        required: Vec<&str>,
        properties: BTreeMap<String, Schema>,
    ) -> Schema {
        Schema {
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
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"a": "x", "b": "y"});
        let result = validate(&schema, &instance);
        assert!(result.is_ok());
    }

    #[test]
    fn missing_required_property() {
        let schema = schema_object_with_required(vec!["name"], {
            let mut m = BTreeMap::new();
            m.insert(
                "name".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let result = validate(&schema, &instance);
        let expected = vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("name"),
            property: "name".to_string(),
        }];
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn wrong_type_string_instead_of_object() {
        let schema = Schema {
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let instance = json!("not an object");
        let result = validate(&schema, &instance);
        let expected = vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
        }];
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn wrong_type_object_instead_of_string() {
        let schema = Schema {
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            required: None,
            title: None,
        };
        let instance = json!({"x": 1});
        let result = validate(&schema, &instance);
        let expected = vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
        }];
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn nested_object_validation() {
        let schema = schema_object_with_required(vec!["address"], {
            let mut m = BTreeMap::new();
            m.insert(
                "address".to_string(),
                Schema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "city".to_string(),
                            Schema {
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
        let result = validate(&schema, &instance);
        let expected = vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("address").push("city"),
            property: "city".to_string(),
        }];
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), expected);
    }

    #[test]
    fn optional_property_absent_valid() {
        let schema = schema_object_with_required(vec![], {
            let mut m = BTreeMap::new();
            m.insert(
                "opt".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let result = validate(&schema, &instance);
        assert!(result.is_ok());
    }

    #[test]
    fn multiple_failures_all_errors_returned() {
        let schema = schema_object_with_required(vec!["a", "b", "c"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "c".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({});
        let result = validate(&schema, &instance);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(
            errs.len(),
            3,
            "must collect all three required-property errors"
        );
        assert!(
            matches!(errs[0], ValidationError::MissingRequired { property: ref p, .. } if p == "a")
        );
        assert_eq!(errs[0].instance_path().as_str(), "/a");
        assert!(
            matches!(errs[1], ValidationError::MissingRequired { property: ref p, .. } if p == "b")
        );
        assert_eq!(errs[1].instance_path().as_str(), "/b");
        assert!(
            matches!(errs[2], ValidationError::MissingRequired { property: ref p, .. } if p == "c")
        );
        assert_eq!(errs[2].instance_path().as_str(), "/c");
    }

    #[test]
    fn multiple_failures_type_and_required_and_nested() {
        let schema = schema_object_with_required(vec!["x", "nested"], {
            let mut m = BTreeMap::new();
            m.insert(
                "x".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "nested".to_string(),
                Schema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "y".to_string(),
                            Schema {
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
        let result = validate(&schema, &instance);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(
            errs.len(),
            2,
            "missing required x and missing required nested.y"
        );
        let paths: Vec<&str> = errs.iter().map(|e| e.instance_path().as_str()).collect();
        assert!(paths.contains(&"/x"));
        assert!(paths.contains(&"/nested/y"));
    }

    #[test]
    fn deeply_nested_instance_does_not_stack_overflow() {
        const DEPTH: usize = 200;
        let mut inner: Schema = Schema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    Schema {
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
            let mut wrap: Schema = Schema {
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
        let result = validate(&inner, &instance_value);
        assert!(
            result.is_ok(),
            "deep validation must not overflow: {result:?}"
        );
    }

    #[test]
    fn pointer_escaping() {
        let schema = schema_object_with_required(vec!["a/b"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a/b".to_string(),
                Schema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"a/b": 123});
        let result = validate(&schema, &instance);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].instance_path().as_str(), "/a~1b");
        assert!(matches!(errs[0], ValidationError::ExpectedString { .. }));
    }
}
