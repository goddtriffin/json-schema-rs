//! JSON Schema validation: schema + instance → validation result with all errors.
//!
//! Collects every validation error (no fail-fast) and returns them in a single result.

mod error;
pub use error::{OrderedF64, ValidationError, ValidationResult};

use crate::json_pointer::JsonPointer;
use crate::json_schema::JsonSchema;
use crate::json_schema::json_schema::AdditionalProperties;
use serde_json::Value;

/// Returns the JSON type name of the value for use in "got" error messages.
fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Serializes a JSON value to a string for error display. Never truncates.
fn value_to_display_string(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "?".to_string())
}

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
        if let Some(ref expected) = schema.const_value
            && instance != expected
        {
            let expected_str: String = value_to_display_string(expected);
            let actual_str: String = value_to_display_string(instance);
            errors.push(ValidationError::NotConst {
                instance_path: instance_path.clone(),
                expected: expected_str,
                actual: actual_str,
            });
            continue;
        }
        if let Some(ref allowed) = schema.enum_values
            && !allowed.is_empty()
            && !allowed.iter().any(|a| a == instance)
        {
            let invalid_value: String = value_to_display_string(instance);
            let allowed_strs: Vec<String> = allowed.iter().map(value_to_display_string).collect();
            errors.push(ValidationError::NotInEnum {
                instance_path: instance_path.clone(),
                invalid_value,
                allowed: allowed_strs,
            });
            continue;
        }
        if let Some(ref any_of) = schema.any_of {
            if any_of.is_empty() {
                errors.push(ValidationError::NoSubschemaMatched {
                    instance_path: instance_path.clone(),
                    subschema_count: 0,
                });
            } else {
                let mut at_least_one_passed: bool = false;
                for subschema in any_of {
                    let sub_result: ValidationResult = validate(subschema, instance);
                    if sub_result.is_ok() {
                        at_least_one_passed = true;
                        break;
                    }
                }
                if !at_least_one_passed {
                    errors.push(ValidationError::NoSubschemaMatched {
                        instance_path: instance_path.clone(),
                        subschema_count: any_of.len(),
                    });
                }
            }
            continue;
        }
        if let Some(ref one_of) = schema.one_of {
            if one_of.is_empty() {
                errors.push(ValidationError::NoSubschemaMatched {
                    instance_path: instance_path.clone(),
                    subschema_count: 0,
                });
            } else {
                let mut pass_count: usize = 0;
                for subschema in one_of {
                    let sub_result: ValidationResult = validate(subschema, instance);
                    if sub_result.is_ok() {
                        pass_count += 1;
                    }
                }
                if pass_count == 0 {
                    errors.push(ValidationError::NoSubschemaMatched {
                        instance_path: instance_path.clone(),
                        subschema_count: one_of.len(),
                    });
                } else if pass_count > 1 {
                    errors.push(ValidationError::MultipleSubschemasMatched {
                        instance_path: instance_path.clone(),
                        subschema_count: one_of.len(),
                        match_count: pass_count,
                    });
                }
            }
            continue;
        }
        if let Some(ref all_of) = schema.all_of
            && !all_of.is_empty()
        {
            for subschema in all_of.iter().rev() {
                stack.push((subschema, instance, instance_path.clone()));
            }
            continue;
        }
        let expected_type: Option<&str> = schema.type_.as_deref();
        match expected_type {
            Some("object") => {
                let Some(obj) = instance.as_object() else {
                    errors.push(ValidationError::ExpectedObject {
                        instance_path: instance_path.clone(),
                        got: json_type_name(instance).to_string(),
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
                // additionalProperties: keys not in properties are "additional"
                let additional_keys: Vec<&str> = obj
                    .keys()
                    .filter(|k| !schema.properties.contains_key(*k))
                    .map(String::as_str)
                    .collect();
                if !additional_keys.is_empty() {
                    match schema.additional_properties.as_ref() {
                        None | Some(AdditionalProperties::Allow) => {}
                        Some(AdditionalProperties::Forbid) => {
                            for key in additional_keys {
                                errors.push(ValidationError::DisallowedAdditionalProperty {
                                    instance_path: instance_path.push(key),
                                    property: key.to_string(),
                                });
                            }
                        }
                        Some(AdditionalProperties::Schema(sub_schema)) => {
                            for key in additional_keys {
                                if let Some(value) = obj.get(key) {
                                    let path = instance_path.push(key);
                                    stack.push((sub_schema, value, path));
                                }
                            }
                        }
                    }
                }
            }
            Some("string") => {
                if !instance.is_string() {
                    errors.push(ValidationError::ExpectedString {
                        instance_path: instance_path.clone(),
                        got: json_type_name(instance).to_string(),
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
                            actual_length: char_count,
                        });
                    }
                    if let Some(max_length) = schema.max_length
                        && char_count > max_length
                    {
                        errors.push(ValidationError::TooLong {
                            instance_path: instance_path.clone(),
                            max_length,
                            actual_length: char_count,
                        });
                    }
                    if let Some(ref pattern) = schema.pattern {
                        match regress::Regex::new(pattern) {
                            Ok(re) => {
                                if re.find(s).is_none() {
                                    errors.push(ValidationError::PatternMismatch {
                                        instance_path: instance_path.clone(),
                                        pattern: pattern.clone(),
                                        value: s.to_string(),
                                    });
                                }
                            }
                            Err(_) => {
                                errors.push(ValidationError::InvalidPatternInSchema {
                                    instance_path: instance_path.clone(),
                                    pattern: pattern.clone(),
                                });
                            }
                        }
                    }
                }
                #[cfg(feature = "uuid")]
                if schema.format.as_deref() == Some("uuid") {
                    if let Some(s) = instance.as_str() {
                        if uuid::Uuid::parse_str(s).is_err() {
                            errors.push(ValidationError::InvalidUuidFormat {
                                instance_path: instance_path.clone(),
                                value: s.to_string(),
                            });
                        }
                    }
                }
            }
            Some("integer") => {
                let valid = instance.as_number().is_some_and(|n| n.as_i64().is_some());
                if !valid {
                    errors.push(ValidationError::ExpectedInteger {
                        instance_path: instance_path.clone(),
                        got: json_type_name(instance).to_string(),
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
                            actual: crate::validator::error::OrderedF64(instance_f64),
                        });
                    }
                    if let Some(max) = schema.maximum
                        && instance_f64 > max
                    {
                        errors.push(ValidationError::AboveMaximum {
                            instance_path: instance_path.clone(),
                            maximum: crate::validator::error::OrderedF64(max),
                            actual: crate::validator::error::OrderedF64(instance_f64),
                        });
                    }
                }
            }
            Some("number") => {
                let valid = instance.as_number().is_some();
                if !valid {
                    errors.push(ValidationError::ExpectedNumber {
                        instance_path: instance_path.clone(),
                        got: json_type_name(instance).to_string(),
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
                            actual: crate::validator::error::OrderedF64(instance_f64),
                        });
                    }
                    if let Some(max) = schema.maximum
                        && instance_f64 > max
                    {
                        errors.push(ValidationError::AboveMaximum {
                            instance_path: instance_path.clone(),
                            maximum: crate::validator::error::OrderedF64(max),
                            actual: crate::validator::error::OrderedF64(instance_f64),
                        });
                    }
                }
            }
            Some("array") => {
                let Some(arr) = instance.as_array() else {
                    errors.push(ValidationError::ExpectedArray {
                        instance_path: instance_path.clone(),
                        got: json_type_name(instance).to_string(),
                    });
                    continue;
                };
                let actual_count: u64 = arr.len() as u64;
                if let Some(min_items) = schema.min_items
                    && arr.len() < min_items.try_into().unwrap_or(usize::MAX)
                {
                    errors.push(ValidationError::TooFewItems {
                        instance_path: instance_path.clone(),
                        min_items,
                        actual_count,
                    });
                }
                if let Some(max_items) = schema.max_items
                    && arr.len() > max_items.try_into().unwrap_or(0)
                {
                    errors.push(ValidationError::TooManyItems {
                        instance_path: instance_path.clone(),
                        max_items,
                        actual_count,
                    });
                }
                if schema.unique_items == Some(true) {
                    let mut duplicate_value_opt: Option<String> = None;
                    for i in 0..arr.len() {
                        for j in (i + 1)..arr.len() {
                            if arr[i] == arr[j] {
                                duplicate_value_opt = Some(value_to_display_string(&arr[i]));
                                break;
                            }
                        }
                        if duplicate_value_opt.is_some() {
                            break;
                        }
                    }
                    if let Some(duplicate_value) = duplicate_value_opt {
                        errors.push(ValidationError::DuplicateArrayItems {
                            instance_path: instance_path.clone(),
                            duplicate_value,
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
    use crate::json_schema::json_schema::AdditionalProperties;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema_object_with_required(
        required: Vec<&str>,
        properties: BTreeMap<String, JsonSchema>,
    ) -> JsonSchema {
        JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties,
            additional_properties: None,
            required: Some(required.into_iter().map(String::from).collect()),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        }
    }

    #[test]
    fn valid_object_with_required_and_properties() {
        let schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
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
                    schema: None,
                    id: None,
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
    fn additional_properties_absent_allows_extra_key() {
        let schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        let instance = json!({"a": "x", "extra": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_rejects_one_extra_key() {
        let mut schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(AdditionalProperties::Forbid);
        let instance = json!({"a": "x", "extra": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::DisallowedAdditionalProperty {
            instance_path: JsonPointer::root().push("extra"),
            property: "extra".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_rejects_multiple_extra_keys() {
        let mut schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(AdditionalProperties::Forbid);
        let instance = json!({"a": "x", "extra1": 1, "extra2": 2});
        let actual_result: ValidationResult = validate(&schema, &instance);
        let expected_errors: Vec<ValidationError> = vec![
            ValidationError::DisallowedAdditionalProperty {
                instance_path: JsonPointer::root().push("extra1"),
                property: "extra1".to_string(),
            },
            ValidationError::DisallowedAdditionalProperty {
                instance_path: JsonPointer::root().push("extra2"),
                property: "extra2".to_string(),
            },
        ];
        let mut expected_sorted = expected_errors;
        expected_sorted.sort_by(|a, b| {
            a.instance_path()
                .to_string()
                .cmp(&b.instance_path().to_string())
        });
        let mut actual_errors = actual_result.expect_err("expected validation errors");
        actual_errors.sort_by(|a, b| {
            a.instance_path()
                .to_string()
                .cmp(&b.instance_path().to_string())
        });
        let expected: ValidationResult = Err(expected_sorted);
        let actual: ValidationResult = Err(actual_errors);
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_empty_object_valid() {
        let mut schema = schema_object_with_required(vec![], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(AdditionalProperties::Forbid);
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_only_known_keys_valid() {
        let mut schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(AdditionalProperties::Forbid);
        let instance = json!({"a": "x"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_valid_when_value_passes() {
        let sub = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            additional_properties: Some(AdditionalProperties::Schema(Box::new(sub))),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({"a": "x", "extra": "allowed"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_invalid_when_value_fails() {
        let sub = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            ..Default::default()
        };
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            additional_properties: Some(AdditionalProperties::Schema(Box::new(sub))),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({"a": "x", "extra": "not_an_integer"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("extra"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_all_subschemas_pass() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: Some(vec![
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "a".to_string(),
                            JsonSchema {
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some("integer".to_string()),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
            ]),
            any_of: None,
            one_of: None,
        };
        let instance = json!({"a": "ok", "b": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_one_subschema_fails_required() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: Some(vec![
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: BTreeMap::new(),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("object".to_string()),
                    required: Some(vec!["b".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some("integer".to_string()),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
            ]),
            any_of: None,
            one_of: None,
        };
        let instance = json!({"a": "x"});
        let actual: ValidationResult = validate(&schema, &instance);
        assert!(matches!(
            actual,
            Err(ref e) if e.iter().any(|err| matches!(err, ValidationError::MissingRequired { property, .. } if property == "b"))
        ));
    }

    #[test]
    fn all_of_multiple_subschemas_fail() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: Some(vec![
                JsonSchema {
                    type_: Some("object".to_string()),
                    required: Some(vec!["a".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "a".to_string(),
                            JsonSchema {
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("object".to_string()),
                    required: Some(vec!["b".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some("integer".to_string()),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
            ]),
            any_of: None,
            one_of: None,
        };
        let instance = json!({"a": 1, "b": "x"});
        let result: ValidationResult = validate(&schema, &instance);
        let errs = result.unwrap_err();
        let expected: usize = 2;
        let actual: usize = errs.len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_empty_valid() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: Some(vec![]),
            any_of: None,
            one_of: None,
        };
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_enum_in_one_subschema_fails_when_not_in_enum() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: Some(vec![
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: BTreeMap::new(),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("object".to_string()),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "s".to_string(),
                            JsonSchema {
                                enum_values: Some(vec![
                                    serde_json::Value::String("a".to_string()),
                                    serde_json::Value::String("b".to_string()),
                                ]),
                                ..Default::default()
                            },
                        );
                        m
                    },
                    ..Default::default()
                },
            ]),
            any_of: None,
            one_of: None,
        };
        let instance = json!({"s": "c"});
        let actual: ValidationResult = validate(&schema, &instance);
        assert!(matches!(actual, Err(ref e) if !e.is_empty()));
        let errs = actual.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| matches!(e, ValidationError::NotInEnum { .. }))
        );
    }

    #[test]
    fn any_of_at_least_one_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: Some(vec![
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            ]),
            one_of: None,
        };
        let instance_str = json!("hello");
        let actual_str: ValidationResult = validate(&schema, &instance_str);
        let expected_str: ValidationResult = Ok(());
        assert_eq!(expected_str, actual_str);
        let instance_int = json!(42);
        let actual_int: ValidationResult = validate(&schema, &instance_int);
        let expected_int: ValidationResult = Ok(());
        assert_eq!(expected_int, actual_int);
    }

    #[test]
    fn any_of_no_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: Some(vec![
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            ]),
            one_of: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        let expected: usize = 1;
        let actual_count: usize = errs.len();
        assert_eq!(expected, actual_count);
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 2,
                ..
            }
        )));
    }

    #[test]
    fn any_of_empty_no_subschema_matches() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: Some(vec![]),
            one_of: None,
        };
        let instance = json!(1);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        let expected: usize = 1;
        let actual_count: usize = errs.len();
        assert_eq!(expected, actual_count);
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 0,
                ..
            }
        )));
    }

    #[test]
    fn any_of_single_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: Some(vec![JsonSchema {
                type_: Some("number".to_string()),
                ..Default::default()
            }]),
            one_of: None,
        };
        let instance = json!(std::f64::consts::PI);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn any_of_single_subschema_fails() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: Some(vec![JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            }]),
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        assert_eq!(1, errs.len());
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 1,
                ..
            }
        )));
    }

    #[test]
    fn one_of_exactly_one_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            ]),
        };
        let instance = json!("hello");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_no_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
                JsonSchema {
                    type_: Some("integer".to_string()),
                    ..Default::default()
                },
            ]),
        };
        let instance = json!(1.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        assert_eq!(1, errs.len());
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 2,
                ..
            }
        )));
    }

    #[test]
    fn one_of_multiple_subschemas_pass() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![
                JsonSchema::default(),
                JsonSchema {
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            ]),
        };
        let instance = json!("x");
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        assert_eq!(1, errs.len());
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::MultipleSubschemasMatched {
                subschema_count: 2,
                match_count: 2,
                ..
            }
        )));
    }

    #[test]
    fn one_of_empty_no_subschema_matches() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![]),
        };
        let instance = json!(1);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        assert_eq!(1, errs.len());
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 0,
                ..
            }
        )));
    }

    #[test]
    fn one_of_single_subschema_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![JsonSchema {
                type_: Some("number".to_string()),
                ..Default::default()
            }]),
        };
        let instance = json!(std::f64::consts::PI);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_single_subschema_fails() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: Some(vec![JsonSchema {
                type_: Some("string".to_string()),
                ..Default::default()
            }]),
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.unwrap_err();
        assert_eq!(1, errs.len());
        assert!(errs.iter().any(|e| matches!(
            e,
            ValidationError::NoSubschemaMatched {
                subschema_count: 1,
                ..
            }
        )));
    }

    #[test]
    fn wrong_type_string_instead_of_object() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("not an object");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_description_validates_as_before() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("string".to_string()),
                        properties: BTreeMap::new(),
                        additional_properties: None,
                        required: None,
                        title: None,
                        description: Some("User name".to_string()),
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
                        pattern: None,
                        format: None,
                        default_value: None,
                        all_of: None,
                        any_of: None,
                        one_of: None,
                    },
                );
                m
            },
            additional_properties: None,
            required: None,
            title: None,
            description: Some("Root type".to_string()),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let valid_instance = json!({"name": "Alice"});
        let actual_valid: ValidationResult = validate(&schema, &valid_instance);
        assert_eq!(Ok(()), actual_valid);
        let invalid_instance = json!({"name": 42});
        let actual_invalid: ValidationResult = validate(&schema, &invalid_instance);
        assert!(actual_invalid.is_err());
    }

    #[test]
    fn schema_with_comment_validates_same_as_without() {
        let schema_without: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema_with_comment: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: Some("Editor note".to_string()),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let valid_instance = json!("hello");
        let invalid_instance = json!(42);
        let expected_valid: ValidationResult = validate(&schema_without, &valid_instance);
        let actual_valid: ValidationResult = validate(&schema_with_comment, &valid_instance);
        assert_eq!(expected_valid, actual_valid);
        let expected_invalid: ValidationResult = validate(&schema_without, &invalid_instance);
        let actual_invalid: ValidationResult = validate(&schema_with_comment, &invalid_instance);
        assert_eq!(expected_invalid, actual_invalid);
    }

    #[test]
    #[expect(clippy::too_many_lines)]
    fn schema_with_default_validates_same_as_without() {
        let mut properties_without: BTreeMap<String, JsonSchema> = BTreeMap::new();
        properties_without.insert(
            "opt".to_string(),
            JsonSchema {
                schema: None,
                id: None,
                type_: Some("string".to_string()),
                properties: BTreeMap::new(),
                additional_properties: None,
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
                pattern: None,
                format: None,
                default_value: None,
                all_of: None,
                any_of: None,
                one_of: None,
            },
        );
        let mut properties_with_default: BTreeMap<String, JsonSchema> = BTreeMap::new();
        properties_with_default.insert(
            "opt".to_string(),
            JsonSchema {
                schema: None,
                id: None,
                type_: Some("string".to_string()),
                properties: BTreeMap::new(),
                additional_properties: None,
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
                pattern: None,
                format: None,
                default_value: Some(serde_json::Value::String("defaulted".to_string())),
                all_of: None,
                any_of: None,
                one_of: None,
            },
        );
        let schema_without: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: properties_without,
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema_with_default: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: properties_with_default,
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let empty_instance = json!({});
        let expected: ValidationResult = validate(&schema_without, &empty_instance);
        let actual: ValidationResult = validate(&schema_with_default, &empty_instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_nonempty() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("ok");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({"x": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "object".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_number_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_null_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "null".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_boolean_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "boolean".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_array_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "array".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_valid_instance_in_allowed() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("open");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_invalid_instance_not_in_allowed() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("pending");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::NotInEnum {
            instance_path: JsonPointer::root(),
            invalid_value: "\"pending\"".to_string(),
            allowed: vec!["\"open\"".to_string(), "\"closed\"".to_string()],
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_with_type_string_valid() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("a");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_with_type_string_invalid_not_in_enum() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("c");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::NotInEnum {
            instance_path: JsonPointer::root(),
            invalid_value: "\"c\"".to_string(),
            allowed: vec!["\"a\"".to_string(), "\"b\"".to_string()],
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_valid_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::String("ok".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("ok");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::String("expected".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("actual");
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.as_ref().expect_err("expected NotConst error");
        assert_eq!(errs.len(), 1);
        assert!(matches!(
            &errs[0],
            ValidationError::NotConst {
                expected: e,
                actual: a,
                ..
            } if e == "\"expected\"" && a == "\"actual\""
        ));
    }

    #[test]
    fn const_valid_number() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: Some(serde_json::Value::Number(42_i64.into())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_with_enum_instance_equals_const_valid() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            const_value: Some(serde_json::Value::String("a".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("a");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_with_enum_instance_not_const_not_const_error() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            const_value: Some(serde_json::Value::String("a".to_string())),
            items: None,
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("b");
        let actual: ValidationResult = validate(&schema, &instance);
        let errs = actual.as_ref().expect_err("expected NotConst");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0], ValidationError::NotConst { .. }));
    }

    #[test]
    fn validation_error_not_const_display() {
        let err: ValidationError = ValidationError::NotConst {
            instance_path: JsonPointer::root(),
            expected: "\"foo\"".to_string(),
            actual: "\"bar\"".to_string(),
        };
        let expected: String =
            "root: value \"bar\" does not match const (expected: \"foo\")".to_string();
        let actual: String = err.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_42() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_0() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_negative() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(-1);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_float() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("42");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_null() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "null".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_object() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({"x": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "object".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_array() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "array".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_bool() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "boolean".to_string(),
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
            got: "number".to_string(),
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
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_valid_integer() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("3.14");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_null() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
            got: "null".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_object() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
            got: "object".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_array() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1.0]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
            got: "array".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_bool() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNumber {
            instance_path: JsonPointer::root(),
            got: "boolean".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_minimum_maximum_valid_in_range() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(0.0),
            maximum: Some(255.0),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(100);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(10.0),
            maximum: Some(100.0),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: OrderedF64(10.0),
            actual: OrderedF64(5.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(0.0),
            maximum: Some(10.0),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(20);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: OrderedF64(10.0),
            actual: OrderedF64(20.0),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_no_minimum_maximum_no_extra_errors() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("integer".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_valid_in_range() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(0.5),
            maximum: Some(99.5),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(50.0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(1.0),
            maximum: Some(10.0),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(0.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: OrderedF64(1.0),
            actual: OrderedF64(0.5),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("number".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            minimum: Some(0.0),
            maximum: Some(1.0),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: OrderedF64(1.0),
            actual: OrderedF64(2.5),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    #[expect(clippy::too_many_lines)]
    fn integer_and_number_min_max_violations_collected_from_multiple_properties() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "low".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("integer".to_string()),
                        properties: BTreeMap::new(),
                        additional_properties: None,
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
                        minimum: Some(10.0),
                        maximum: Some(100.0),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                        default_value: None,
                        all_of: None,
                        any_of: None,
                        one_of: None,
                    },
                );
                m.insert(
                    "high".to_string(),
                    JsonSchema {
                        schema: None,
                        id: None,
                        type_: Some("integer".to_string()),
                        properties: BTreeMap::new(),
                        additional_properties: None,
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
                        minimum: Some(10.0),
                        maximum: Some(100.0),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                        default_value: None,
                        all_of: None,
                        any_of: None,
                        one_of: None,
                    },
                );
                m
            },
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!({"low": 5, "high": 200});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![
            ValidationError::AboveMaximum {
                instance_path: JsonPointer::root().push("high"),
                maximum: OrderedF64(100.0),
                actual: OrderedF64(200.0),
            },
            ValidationError::BelowMinimum {
                instance_path: JsonPointer::root().push("low"),
                minimum: OrderedF64(10.0),
                actual: OrderedF64(5.0),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_non_empty() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_invalid_not_array() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("not an array");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedArray {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_with_items_valid() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_with_items_invalid_element() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(["a", 42, "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root().push("1"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_no_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_duplicates_invalid() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: Some(false),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_absent_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Box::new(item_schema)),
            unique_items: None,
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_empty_array_valid() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: Some(true),
            min_items: None,
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(3),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooFewItems {
            instance_path: JsonPointer::root(),
            min_items: 3,
            actual_count: 2,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_edge_len_equals_min() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: None,
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(2),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooManyItems {
            instance_path: JsonPointer::root(),
            max_items: 2,
            actual_count: 3,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_edge_len_equals_max() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: None,
            max_items: Some(2),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_both_pass() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_fail_too_few() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooFewItems {
            instance_path: JsonPointer::root(),
            min_items: 2,
            actual_count: 1,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_fail_too_many() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3, 4, 5, 6]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooManyItems {
            instance_path: JsonPointer::root(),
            max_items: 5,
            actual_count: 6,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_absent_unchanged() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_not_array_expected_array_only() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("array".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: None,
            unique_items: None,
            min_items: Some(2),
            max_items: Some(5),
            minimum: None,
            maximum: None,
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!("not an array");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedArray {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
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
            got: "string".to_string(),
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
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_with_null() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!(null);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
            got: "null".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_with_array() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: BTreeMap::new(),
            additional_properties: None,
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root(),
            got: "array".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn no_type_but_properties_object_instance_valid() {
        let schema: JsonSchema = JsonSchema {
            schema: None,
            id: None,
            type_: None,
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        schema: None,
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            additional_properties: None,
            required: Some(vec!["name".to_string()]),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
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
                    schema: None,
                    id: None,
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "city".to_string(),
                            JsonSchema {
                                schema: None,
                                id: None,
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        inner
                    },
                    additional_properties: None,
                    required: Some(vec!["city".to_string()]),
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
                    pattern: None,
                    format: None,
                    default_value: None,
                    all_of: None,
                    any_of: None,
                    one_of: None,
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
                    schema: None,
                    id: None,
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
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "c".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
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
                    schema: None,
                    id: None,
                    type_: Some("string".to_string()),
                    ..Default::default()
                },
            );
            m.insert(
                "nested".to_string(),
                JsonSchema {
                    schema: None,
                    id: None,
                    type_: Some("object".to_string()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "y".to_string(),
                            JsonSchema {
                                schema: None,
                                id: None,
                                type_: Some("string".to_string()),
                                ..Default::default()
                            },
                        );
                        inner
                    },
                    additional_properties: None,
                    required: Some(vec!["y".to_string()]),
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
                    pattern: None,
                    format: None,
                    default_value: None,
                    all_of: None,
                    any_of: None,
                    one_of: None,
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
            schema: None,
            id: None,
            type_: Some("object".to_string()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    JsonSchema {
                        schema: None,
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            additional_properties: None,
            required: Some(vec!["value".to_string()]),
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
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
        };
        for _ in 0..DEPTH {
            let mut wrap: JsonSchema = JsonSchema {
                schema: None,
                id: None,
                type_: Some("object".to_string()),
                properties: BTreeMap::new(),
                additional_properties: None,
                required: Some(vec!["child".to_string()]),
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
                pattern: None,
                format: None,
                default_value: None,
                all_of: None,
                any_of: None,
                one_of: None,
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
                    schema: None,
                    id: None,
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
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_exact_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            min_length: Some(5),
            ..Default::default()
        };
        let instance = json!("hi");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooShort {
            instance_path: JsonPointer::root(),
            min_length: 5,
            actual_length: 2,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_above_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_exact_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            max_length: Some(3),
            ..Default::default()
        };
        let instance = json!("hello");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooLong {
            instance_path: JsonPointer::root(),
            max_length: 3,
            actual_length: 5,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_max_length_below_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            ..Default::default()
        };
        let instance = json!("this is a very long string with no max length constraint");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_zero_allows_empty() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            max_length: Some(0),
            ..Default::default()
        };
        let instance = json!("x");
        let actual = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::TooLong {
            instance_path: JsonPointer::root(),
            max_length: 0,
            actual_length: 1,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_max_length_zero_empty_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            schema: None,
            id: None,
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
            schema: None,
            id: None,
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
            actual_length: 2,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_above_max_length_with_min_also_set() {
        let schema = JsonSchema {
            schema: None,
            id: None,
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
            actual_length: 11,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_min_length_unicode_code_points() {
        // "日本語" has 3 Unicode code points but 9 UTF-8 bytes.
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            min_length: Some(3),
            max_length: Some(3),
            ..Default::default()
        };
        let instance = json!("日本語");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_pattern_partial_match_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("a".to_string()),
            ..Default::default()
        };
        let instance = json!("cat");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_full_match_passes() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("^[0-9]+$".to_string()),
            ..Default::default()
        };
        let instance = json!("123");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_mismatch_fails() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("^[0-9]+$".to_string()),
            ..Default::default()
        };
        let instance = json!("12a3");
        let expected: ValidationResult = Err(vec![ValidationError::PatternMismatch {
            instance_path: JsonPointer::root(),
            pattern: "^[0-9]+$".to_string(),
            value: "12a3".to_string(),
        }]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_non_string_instance_only_expected_string() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("^[0-9]+$".to_string()),
            ..Default::default()
        };
        let instance = json!(42);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_invalid_regex_in_schema() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("[".to_string()),
            ..Default::default()
        };
        let instance = json!("x");
        let expected: ValidationResult = Err(vec![ValidationError::InvalidPatternInSchema {
            instance_path: JsonPointer::root(),
            pattern: "[".to_string(),
        }]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_and_max_length_multiple_errors() {
        let schema = JsonSchema {
            schema: None,
            id: None,
            type_: Some("string".to_string()),
            pattern: Some("^[0-9]+$".to_string()),
            max_length: Some(2),
            ..Default::default()
        };
        let instance = json!("12a");
        let expected: ValidationResult = Err(vec![
            ValidationError::TooLong {
                instance_path: JsonPointer::root(),
                max_length: 2,
                actual_length: 3,
            },
            ValidationError::PatternMismatch {
                instance_path: JsonPointer::root(),
                pattern: "^[0-9]+$".to_string(),
                value: "12a".to_string(),
            },
        ]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn validate_uuid_format_valid_uuid() {
        let schema: JsonSchema =
            serde_json::from_str(r#"{"type":"string","format":"uuid"}"#).unwrap();
        let instance = json!("550e8400-e29b-41d4-a716-446655440000");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn validate_uuid_format_invalid_string() {
        let schema: JsonSchema =
            serde_json::from_str(r#"{"type":"string","format":"uuid"}"#).unwrap();
        let instance = json!("not-a-uuid");
        let expected: ValidationResult = Err(vec![ValidationError::InvalidUuidFormat {
            instance_path: JsonPointer::root(),
            value: "not-a-uuid".to_string(),
        }]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }
}
