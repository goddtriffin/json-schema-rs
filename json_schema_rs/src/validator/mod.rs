//! JSON Schema validation: schema + instance → validation result with all errors.
//!
//! A schema is compiled once into a [`CompiledSchema`] (an arena of nodes with every `$ref`
//! resolved and every regular expression compiled), which then validates any number of
//! instances. [`validate`] and [`validate_with_registry`] compile and validate in one call.

mod compiled;
mod error;
mod eval;
mod format;
#[cfg(test)]
mod tests_keywords;

pub use compiled::{CompileError, CompileErrors, CompiledSchema};
pub use error::{ValidationError, ValidationResult};

use crate::json_pointer::JsonPointer;
use crate::json_schema::{Registry, RegistryError, SchemaRef};

/// Validates a JSON instance against a schema. Collects **all** validation errors
/// and returns them in a single result (no fail-fast).
///
/// The schema is a schema object (`&JsonSchema`) or a whole document that may be a boolean
/// schema (`&Schema`): `true` accepts every instance and `false` none. Boolean schemas are
/// honoured at every subschema position too. Type-specific keywords (`required`, `minLength`,
/// `items`, `minimum`, ...) apply to instances of their kind whether or not `type` is present.
///
/// `$ref` is resolved through a [`Registry`] built for this call with the defaults of
/// [`Registry::new`] (in-document references, `$id` / `$anchor` resources, the embedded
/// standard metaschemas; no retrieval), and the schema is compiled for this call. A schema whose
/// registry cannot be built (a dangling `$ref`, a duplicate `$id`, ...) or that cannot be
/// compiled (an invalid `pattern`) fails every instance with one error per problem. To validate
/// many instances, build the registry and the [`CompiledSchema`] once.
///
/// # Errors
///
/// Returns `Err(errors)` when the instance does not conform to the schema, with
/// one or more [`ValidationError`] values describing each failure.
///
/// # Example
///
/// ```
/// use json_schema_rs::{JsonSchema, Schema, validate};
/// use serde_json::json;
///
/// let schema: JsonSchema = serde_json::from_str(r#"{"type":"object","properties":{"name":{"type":"string"}}}"#).unwrap();
/// let instance = json!({"name": "Alice"});
/// assert!(validate(&schema, &instance).is_ok());
///
/// let never: Schema = Schema::try_from("false").unwrap();
/// assert!(validate(&never, &instance).is_err());
/// ```
pub fn validate<'a>(
    schema: impl Into<SchemaRef<'a>>,
    instance: &serde_json::Value,
) -> ValidationResult {
    match Registry::new(schema) {
        Ok(registry) => validate_with_registry(&registry, instance),
        Err(errors) => Err(errors.iter().map(registry_error).collect()),
    }
}

/// Validates `instance` against the root schema of `registry`, resolving every `$ref` through
/// it. Compiles the schema for this call; to validate many instances, compile it once with
/// [`CompiledSchema::new`]. See [`validate`] for what is checked.
///
/// # Errors
///
/// Returns `Err(errors)` when the instance does not conform to the schema.
///
/// # Example
///
/// ```
/// use json_schema_rs::{CompiledSchema, JsonSchema, Registry, validate_with_registry};
/// use serde_json::json;
///
/// let schema: JsonSchema = JsonSchema::try_from(
///     r#"{"$id":"https://example.com/root.json","items":{"$ref":"item.json"},
///        "$defs":{"item":{"$id":"item.json","type":"integer"}}}"#,
/// )
/// .unwrap();
/// let registry: Registry<'_> = Registry::new(&schema).unwrap();
/// assert!(validate_with_registry(&registry, &json!([1, 2])).is_ok());
///
/// let compiled: CompiledSchema = CompiledSchema::new(&registry).unwrap();
/// assert!(!compiled.is_valid(&json!(["a"])));
/// ```
pub fn validate_with_registry(
    registry: &Registry<'_>,
    instance: &serde_json::Value,
) -> ValidationResult {
    match CompiledSchema::new(registry) {
        Ok(compiled) => compiled.validate(instance),
        Err(errors) => Err(errors.iter().map(compile_error).collect()),
    }
}

/// The error every instance gets for one problem that kept the schema's registry from being
/// built (one per problem).
fn registry_error(error: &RegistryError) -> ValidationError {
    let instance_path: JsonPointer = JsonPointer::root();
    match error.ref_str() {
        Some(ref_str) => ValidationError::InvalidRef {
            instance_path,
            ref_str: ref_str.to_string(),
            reason: error.to_string(),
        },
        None => ValidationError::InvalidSchema {
            instance_path,
            reason: error.to_string(),
        },
    }
}

/// The error every instance gets for one problem that kept the schema from compiling (one per
/// problem).
fn compile_error(error: &CompileError) -> ValidationError {
    let instance_path: JsonPointer = JsonPointer::root();
    match error {
        CompileError::InvalidPattern { pattern, .. } => ValidationError::InvalidPatternInSchema {
            instance_path,
            pattern: pattern.clone(),
        },
        CompileError::UnknownRef { ref_str, .. } => ValidationError::InvalidRef {
            instance_path,
            ref_str: ref_str.clone(),
            reason: error.to_string(),
        },
        CompileError::InvalidKeywordValue { .. } | CompileError::UnknownFormat { .. } => {
            ValidationError::InvalidSchema {
                instance_path,
                reason: error.to_string(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ValidationError, ValidationResult, validate};
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::JsonSchema;
    use crate::json_schema::RegistryError;
    use crate::json_schema::{JsonType, Schema, SchemaRef};
    use serde_json::json;
    use std::collections::BTreeMap;

    /// A finite `f64` as a JSON number.
    fn float(v: f64) -> serde_json::Number {
        serde_json::Number::from_f64(v).expect("finite")
    }

    fn schema_object_with_required(
        required: Vec<&str>,
        properties: BTreeMap<String, JsonSchema>,
    ) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: properties
                .into_iter()
                .map(|(key, schema)| (key, Schema::from(schema)))
                .collect(),
            required: Some(required.into_iter().map(String::from).collect()),
            ..Default::default()
        }
    }

    #[test]
    fn validate_ref_to_defs_valid() {
        let schema: JsonSchema = serde_json::from_str(
            r##"{
  "$defs": {
    "Address": {
      "type": "object",
      "properties": { "city": { "type": "string" } },
      "required": ["city"]
    }
  },
  "type": "object",
  "properties": { "address": { "$ref": "#/$defs/Address" } },
  "required": ["address"]
}"##,
        )
        .expect("parse schema");
        let instance = json!({"address": {"city": "NYC"}});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_ref_to_missing_defs_emits_invalid_ref() {
        let schema: JsonSchema = serde_json::from_str(
            r##"{
  "type": "object",
  "properties": { "x": { "$ref": "#/$defs/Missing" } }
}"##,
        )
        .expect("parse schema");
        let instance = json!({"x": "hi"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected_reason: String = RegistryError::UnresolvableRef {
            ref_str: "#/$defs/Missing".to_string(),
            location: "/properties/x".to_string(),
            target: "json-schema:///#/$defs/Missing".to_string(),
            reason: "no such location".to_string(),
        }
        .to_string();
        let expected: ValidationResult = Err(vec![ValidationError::InvalidRef {
            instance_path: JsonPointer::root(),
            ref_str: "#/$defs/Missing".to_string(),
            reason: expected_reason,
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_reports_every_registry_problem() {
        let schema: JsonSchema = serde_json::from_str(
            r##"{
  "$defs": {
    "a": { "$id": "https://example.com/s.json" },
    "b": { "$id": "https://example.com/s.json" }
  },
  "properties": {
    "x": { "$ref": "#/$defs/missing" },
    "y": { "$ref": "#/$defs/gone" }
  }
}"##,
        )
        .expect("parse schema");
        let actual: ValidationResult = validate(&schema, &json!({}));
        let expected: ValidationResult = Err(vec![
            ValidationError::InvalidSchema {
                instance_path: JsonPointer::root(),
                reason: RegistryError::DuplicateId {
                    uri: "https://example.com/s.json".to_string(),
                    first: "/$defs/a".to_string(),
                    second: "/$defs/b".to_string(),
                }
                .to_string(),
            },
            ValidationError::InvalidRef {
                instance_path: JsonPointer::root(),
                ref_str: "#/$defs/missing".to_string(),
                reason: RegistryError::UnresolvableRef {
                    ref_str: "#/$defs/missing".to_string(),
                    location: "/properties/x".to_string(),
                    target: "json-schema:///#/$defs/missing".to_string(),
                    reason: "no such location".to_string(),
                }
                .to_string(),
            },
            ValidationError::InvalidRef {
                instance_path: JsonPointer::root(),
                ref_str: "#/$defs/gone".to_string(),
                reason: RegistryError::UnresolvableRef {
                    ref_str: "#/$defs/gone".to_string(),
                    location: "/properties/y".to_string(),
                    target: "json-schema:///#/$defs/gone".to_string(),
                    reason: "no such location".to_string(),
                }
                .to_string(),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_ref_cycle_in_defs_emits_ref_cycle() {
        let schema: JsonSchema = serde_json::from_str(
            r##"{
  "$defs": {
    "A": { "$ref": "#/$defs/B" },
    "B": { "$ref": "#/$defs/A" }
  },
  "type": "object",
  "properties": { "x": { "$ref": "#/$defs/A" } }
}"##,
        )
        .expect("parse schema");
        let instance = json!({"x": 42});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::RefCycle {
            instance_path: JsonPointer::root().push("x"),
            ref_str: "#/$defs/A".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn valid_object_with_required_and_properties() {
        let schema = schema_object_with_required(vec!["a"], {
            let mut m = BTreeMap::new();
            m.insert(
                "a".to_string(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
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
                    type_: Some(JsonType::String.into()),
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
                    type_: Some(JsonType::String.into()),
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(Schema::Bool(false));
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(Schema::Bool(false));
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(Schema::Bool(false));
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m
        });
        schema.additional_properties = Some(Schema::Bool(false));
        let instance = json!({"a": "x"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_valid_when_value_passes() {
        let sub = JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        };
        let schema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }
                    .into(),
                );
                m
            },
            additional_properties: Some(Schema::from(sub)),
            ..Default::default()
        };
        let instance = json!({"a": "x", "extra": "allowed"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_invalid_when_value_fails() {
        let sub = JsonSchema {
            type_: Some(JsonType::Integer.into()),
            ..Default::default()
        };
        let schema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "a".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }
                    .into(),
                );
                m
            },
            additional_properties: Some(Schema::from(sub)),
            ..Default::default()
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
            all_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "a".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::String.into()),
                                ..Default::default()
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::Integer.into()),
                                ..Default::default()
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
        };
        let instance = json!({"a": "ok", "b": 1});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_one_subschema_fails_required() {
        let schema = JsonSchema {
            all_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    properties: BTreeMap::new(),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    required: Some(vec!["b".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::Integer.into()),
                                ..Default::default()
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            all_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    required: Some(vec!["a".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "a".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::String.into()),
                                ..Default::default()
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    required: Some(vec!["b".to_string()]),
                    properties: {
                        let mut m = BTreeMap::new();
                        m.insert(
                            "b".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::Integer.into()),
                                ..Default::default()
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            all_of: Some(vec![]),
            ..Default::default()
        };
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_enum_in_one_subschema_fails_when_not_in_enum() {
        let schema = JsonSchema {
            all_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    properties: BTreeMap::new(),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
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
                            }
                            .into(),
                        );
                        m
                    },
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            any_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Integer.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            any_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Integer.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            any_of: Some(vec![]),
            ..Default::default()
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
            any_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Number.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
        };
        let instance = json!(std::f64::consts::PI);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn any_of_single_subschema_fails() {
        let schema = JsonSchema {
            any_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            one_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Integer.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
        };
        let instance = json!("hello");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_no_subschema_passes() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
                JsonSchema {
                    type_: Some(JsonType::Integer.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            one_of: Some(vec![
                JsonSchema::default().into(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            one_of: Some(vec![]),
            ..Default::default()
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
            one_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::Number.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
        };
        let instance = json!(std::f64::consts::PI);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_single_subschema_fails() {
        let schema = JsonSchema {
            one_of: Some(vec![
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                }
                .into(),
            ]),
            ..Default::default()
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
            type_: Some(JsonType::Object.into()),
            ..Default::default()
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
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::String.into()),
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
                        ..Default::default()
                    }
                    .into(),
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
            ..Default::default()
        };
        let valid_instance = json!({"name": "Alice"});
        let actual_valid: ValidationResult = validate(&schema, &valid_instance);
        assert_eq!(Ok(()), actual_valid);
        let invalid_instance = json!({"name": 42});
        let actual_invalid: ValidationResult = validate(&schema, &invalid_instance);
        assert!(actual_invalid.is_err());
    }

    #[test]
    fn schema_with_examples_valid_instance_passes() {
        let schema: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":[{"x":"foo"}]}"#,
        )
        .expect("parse schema");
        let instance = json!({"x": "valid"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_deprecated_property_valid_instance_passes() {
        let schema: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"legacy":{"type":"string","deprecated":true}}}"#,
        )
        .expect("parse schema");
        let instance = json!({"legacy": "still-valid"});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_examples_invalid_instance_same_errors_as_without() {
        let schema: JsonSchema = serde_json::from_str(
            r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":[{"x":"foo"}]}"#,
        )
        .expect("parse schema");
        let instance = json!({});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("x"),
            property: "x".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_comment_validates_same_as_without() {
        let schema_without: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema_with_comment: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
        let mut properties_without: BTreeMap<String, Schema> = BTreeMap::new();
        properties_without.insert(
            "opt".to_string(),
            JsonSchema {
                type_: Some(JsonType::String.into()),
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
                ..Default::default()
            }
            .into(),
        );
        let mut properties_with_default: BTreeMap<String, Schema> = BTreeMap::new();
        properties_with_default.insert(
            "opt".to_string(),
            JsonSchema {
                type_: Some(JsonType::String.into()),
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
                ..Default::default()
            }
            .into(),
        );
        let schema_without: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
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
            ..Default::default()
        };
        let schema_with_default: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
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
            ..Default::default()
        };
        let empty_instance = json!({});
        let expected: ValidationResult = validate(&schema_without, &empty_instance);
        let actual: ValidationResult = validate(&schema_with_default, &empty_instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_nonempty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let instance = json!("ok");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_string_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let instance = json!("");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_object_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
    fn root_type_boolean_valid_true() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        };
        let instance = json!(true);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_boolean_valid_false() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        };
        let instance = json!(false);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_string_instead_of_boolean() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        };
        let instance = json!("hello");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedBoolean {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn wrong_type_array_instead_of_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
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
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            ..Default::default()
        };
        let instance = json!("open");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_invalid_instance_not_in_allowed() {
        let schema: JsonSchema = JsonSchema {
            enum_values: Some(vec![
                serde_json::Value::String("open".to_string()),
                serde_json::Value::String("closed".to_string()),
            ]),
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            ..Default::default()
        };
        let instance = json!("a");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_with_type_string_invalid_not_in_enum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
            enum_values: Some(vec![
                serde_json::Value::String("a".to_string()),
                serde_json::Value::String("b".to_string()),
            ]),
            ..Default::default()
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
            const_value: Some(serde_json::Value::String("ok".to_string())),
            ..Default::default()
        };
        let instance = json!("ok");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            const_value: Some(serde_json::Value::String("expected".to_string())),
            ..Default::default()
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
            const_value: Some(serde_json::Value::Number(42_i64.into())),
            ..Default::default()
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_with_enum_instance_equals_const_valid() {
        let schema: JsonSchema = JsonSchema {
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
            ..Default::default()
        };
        let instance = json!("a");
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_with_enum_instance_not_const_not_const_error() {
        let schema: JsonSchema = JsonSchema {
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_0() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
        };
        let instance = json!(0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_valid_negative() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
        };
        let instance = json!(-1);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_integer_invalid_float() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
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
                    type_: Some(JsonType::Integer.into()),
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
                    type_: Some(JsonType::Integer.into()),
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
                    type_: Some(JsonType::Integer.into()),
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
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_valid_integer() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_number_invalid_string() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Number.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Integer.into()),
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
            minimum: Some(0_u64.into()),
            maximum: Some(255_u64.into()),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(100);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            minimum: Some(10_u64.into()),
            maximum: Some(100_u64.into()),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: 10_u64.into(),
            actual: 5_u64.into(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            minimum: Some(0_u64.into()),
            maximum: Some(10_u64.into()),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(20);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: 10_u64.into(),
            actual: 20_u64.into(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_no_minimum_maximum_no_extra_errors() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Integer.into()),
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
            ..Default::default()
        };
        let instance = json!(42);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_valid_in_range() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
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
            minimum: Some(float(0.5)),
            maximum: Some(float(99.5)),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(50.0);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_below_minimum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
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
            minimum: Some(1_u64.into()),
            maximum: Some(10_u64.into()),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(0.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: 1_u64.into(),
            actual: float(0.5),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_above_maximum() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Number.into()),
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
            minimum: Some(0_u64.into()),
            maximum: Some(1_u64.into()),
            min_length: None,
            max_length: None,
            pattern: None,
            format: None,
            default_value: None,
            all_of: None,
            any_of: None,
            one_of: None,
            ..Default::default()
        };
        let instance = json!(2.5);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: 1_u64.into(),
            actual: float(2.5),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    #[expect(clippy::too_many_lines)]
    fn integer_and_number_min_max_violations_collected_from_multiple_properties() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "low".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::Integer.into()),
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
                        minimum: Some(10_u64.into()),
                        maximum: Some(100_u64.into()),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                        default_value: None,
                        all_of: None,
                        any_of: None,
                        one_of: None,
                        ..Default::default()
                    }
                    .into(),
                );
                m.insert(
                    "high".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::Integer.into()),
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
                        minimum: Some(10_u64.into()),
                        maximum: Some(100_u64.into()),
                        min_length: None,
                        max_length: None,
                        pattern: None,
                        format: None,
                        default_value: None,
                        all_of: None,
                        any_of: None,
                        one_of: None,
                        ..Default::default()
                    }
                    .into(),
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
            ..Default::default()
        };
        let instance = json!({"low": 5, "high": 200});
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Err(vec![
            ValidationError::AboveMaximum {
                instance_path: JsonPointer::root().push("high"),
                maximum: 100_u64.into(),
                actual: 200_u64.into(),
            },
            ValidationError::BelowMinimum {
                instance_path: JsonPointer::root().push("low"),
                minimum: 10_u64.into(),
                actual: 5_u64.into(),
            },
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_valid_non_empty() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_invalid_not_array() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_array_with_items_invalid_element() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
        };
        let instance = json!(["a", "b", "c"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_duplicates_invalid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
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
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_absent_duplicates_valid() {
        let item_schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            ..Default::default()
        };
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
            properties: BTreeMap::new(),
            additional_properties: None,
            required: None,
            title: None,
            description: None,
            comment: None,
            enum_values: None,
            const_value: None,
            items: Some(Schema::from(item_schema)),
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
            ..Default::default()
        };
        let instance = json!(["a", "a"]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_true_empty_array_valid() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_only_fail() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_both_pass() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_fail_too_few() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
        };
        let instance = json!([1, 2, 3]);
        let actual: ValidationResult = validate(&schema, &instance);
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_max_items_not_array_expected_array_only() {
        let schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::Array.into()),
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
            ..Default::default()
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
                    type_: Some(JsonType::Number.into()),
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
                    type_: Some(JsonType::Number.into()),
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
                    type_: Some(JsonType::Number.into()),
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
            type_: Some(JsonType::Object.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Object.into()),
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
            ..Default::default()
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
            type_: Some(JsonType::Object.into()),
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
            ..Default::default()
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
            type_: None,
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "name".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }
                    .into(),
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
            ..Default::default()
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
                    type_: Some(JsonType::Object.into()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "city".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::String.into()),
                                ..Default::default()
                            }
                            .into(),
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
                    ..Default::default()
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
                    type_: Some(JsonType::String.into()),
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m.insert(
                "b".to_string(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m.insert(
                "c".to_string(),
                JsonSchema {
                    type_: Some(JsonType::String.into()),
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
                    type_: Some(JsonType::String.into()),
                    ..Default::default()
                },
            );
            m.insert(
                "nested".to_string(),
                JsonSchema {
                    type_: Some(JsonType::Object.into()),
                    properties: {
                        let mut inner = BTreeMap::new();
                        inner.insert(
                            "y".to_string(),
                            JsonSchema {
                                type_: Some(JsonType::String.into()),
                                ..Default::default()
                            }
                            .into(),
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
                    ..Default::default()
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
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    JsonSchema {
                        type_: Some(JsonType::String.into()),
                        ..Default::default()
                    }
                    .into(),
                );
                m
            },
            required: Some(vec!["value".to_string()]),
            ..Default::default()
        };
        for _ in 0..DEPTH {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some(JsonType::Object.into()),
                properties: BTreeMap::new(),
                required: Some(vec!["child".to_string()]),
                ..Default::default()
            };
            wrap.properties.insert("child".to_string(), inner.into());
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
                    type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            min_length: Some(3),
            ..Default::default()
        };
        let instance = json!("abc");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_below_fails() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            min_length: Some(2),
            ..Default::default()
        };
        let instance = json!("hello");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_absent_any_length_passes() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_exact_passes() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
            max_length: Some(5),
            ..Default::default()
        };
        let instance = json!("hello");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_above_fails() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            max_length: Some(10),
            ..Default::default()
        };
        let instance = json!("hi");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_absent_any_length_passes() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        };
        let instance = json!("this is a very long string with no max length constraint");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_min_length_zero_allows_empty() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
            min_length: Some(0),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_max_length_zero_requires_empty() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            max_length: Some(0),
            ..Default::default()
        };
        let instance = json!("");
        assert_eq!(Ok(()), validate(&schema, &instance));
    }

    #[test]
    fn validate_string_both_constraints_within_range_passes() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
            pattern: Some("a".to_string()),
            ..Default::default()
        };
        let instance = json!("cat");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_uses_unicode_property_escapes() {
        let schema = JsonSchema {
            pattern: Some(r"\p{Letter}cole".to_string()),
            ..Default::default()
        };
        let instance = json!("l'école");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_unicode_property_escape_rejects_non_letter() {
        let schema = JsonSchema {
            pattern: Some(r"^\p{Letter}$".to_string()),
            ..Default::default()
        };
        let instance = json!("1");
        let expected: ValidationResult = Err(vec![ValidationError::PatternMismatch {
            instance_path: JsonPointer::root(),
            pattern: r"^\p{Letter}$".to_string(),
            value: "1".to_string(),
        }]);
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_string_pattern_full_match_passes() {
        let schema = JsonSchema {
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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
            type_: Some(JsonType::String.into()),
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

    #[test]
    fn validate_uuid_format_valid_uuid() {
        let schema: JsonSchema =
            serde_json::from_str(r#"{"type":"string","format":"uuid"}"#).unwrap();
        let instance = json!("550e8400-e29b-41d4-a716-446655440000");
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(&schema, &instance);
        assert_eq!(expected, actual);
    }

    // --- `format` follows the dialect: asserted in drafts 4/6/7, annotation in 2019-09/2020-12 ---

    /// Validates the string `"not-valid"`, which no format accepts, against `schema_json`.
    fn validate_not_valid(
        schema_json: &str,
        settings: &crate::JsonSchemaSettings,
    ) -> ValidationResult {
        let schema: JsonSchema = JsonSchema::new_from_str(schema_json, settings).expect("parse");
        validate(&schema, &json!("not-valid"))
    }

    fn invalid_format_errors(format: &str) -> ValidationResult {
        Err(vec![ValidationError::InvalidFormat {
            instance_path: JsonPointer::root(),
            format: format.to_string(),
            value: "not-valid".to_string(),
        }])
    }

    #[test]
    fn validate_uuid_format_is_annotation_by_default_in_2020_12() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_not_valid(
            r#"{"type":"string","format":"uuid"}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uuid_format_is_annotation_by_default_in_2019_09() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"https://json-schema.org/draft/2019-09/schema","type":"string","format":"uuid"}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_format_asserted_by_default_in_draft7() {
        let expected: ValidationResult = invalid_format_errors("ipv4");
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"string","format":"ipv4"}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_format_asserted_by_default_in_draft4() {
        let expected: ValidationResult = invalid_format_errors("ipv4");
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"string","format":"ipv4"}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uuid_format_asserted_in_2020_12_with_assertion_override() {
        let settings: crate::JsonSchemaSettings = crate::JsonSchemaSettings::builder()
            .format_mode(crate::FormatMode::Assertion)
            .build();
        let expected: ValidationResult = invalid_format_errors("uuid");
        let actual: ValidationResult =
            validate_not_valid(r#"{"type":"string","format":"uuid"}"#, &settings);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_uuid_format_is_annotation_in_draft7_with_annotation_override() {
        let settings: crate::JsonSchemaSettings = crate::JsonSchemaSettings::builder()
            .format_mode(crate::FormatMode::Annotation)
            .build();
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"string","format":"uuid"}"#,
            &settings,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_format_asserted_when_spec_version_pinned_to_draft7() {
        let expected: ValidationResult = invalid_format_errors("ipv4");
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"string","format":"ipv4"}"#,
            &crate::SpecVersion::Draft07.default_schema_settings(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_format_asserted_inside_any_of_in_draft7() {
        let expected: ValidationResult = Err(vec![ValidationError::NoSubschemaMatched {
            instance_path: JsonPointer::root(),
            subschema_count: 1,
        }]);
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","anyOf":[{"type":"string","format":"ipv4"}]}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_format_ignored_in_a_draft_that_does_not_define_it() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_not_valid(
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"string","format":"uuid"}"#,
            &crate::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    // --- Exact numbers: integer rule, bounds, multipleOf, mathematical equality ---

    /// Parses `schema_json` with `settings` and validates `instance_json` against it.
    fn validate_text(
        schema_json: &str,
        instance_json: &str,
        settings: &crate::json_schema::JsonSchemaSettings,
    ) -> ValidationResult {
        let schema: JsonSchema = JsonSchema::new_from_str(schema_json, settings).expect("schema");
        let instance: serde_json::Value = serde_json::from_str(instance_json).expect("instance");
        validate(&schema, &instance)
    }

    /// Whether `instance_json` is valid against `schema_json` under default settings.
    fn is_valid(schema_json: &str, instance_json: &str) -> bool {
        validate_text(
            schema_json,
            instance_json,
            &crate::json_schema::JsonSchemaSettings::default(),
        )
        .is_ok()
    }

    /// A JSON number parsed from `text`, exactly as `serde_json` reads it.
    fn num(text: &str) -> serde_json::Number {
        serde_json::from_str(text).expect("number")
    }

    const DRAFT4: &str = "http://json-schema.org/draft-04/schema#";

    #[test]
    fn min_length_integer_valued_decimal_rejects_shorter_string() {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"string","minLength":2.0}"#, r#""a""#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_length_integer_valued_decimal_accepts_long_enough_string() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"string","minLength":2.0}"#, r#""ab""#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn max_items_integer_valued_decimal_rejects_longer_array() {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"array","maxItems":2.0}"#, "[1,2,3]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_accepts_zero_fraction_float() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"integer"}"#, "1.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_accepts_u64_above_i64_max() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"integer"}"#, "18446744073709551615");
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_accepts_bignum() {
        let expected: bool = true;
        let actual: bool = is_valid(
            r#"{"type":"integer"}"#,
            "12345678910111213141516171819202122232425",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_rejects_fraction() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"type":"integer"}"#,
            "1.5",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_integer_rejects_zero_fraction_float() {
        let expected: bool = false;
        let actual: bool = is_valid(
            &format!(r#"{{"$schema":"{DRAFT4}","type":"integer"}}"#),
            "1.0",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_integer_accepts_u64_above_i64_max() {
        let expected: bool = true;
        let actual: bool = is_valid(
            &format!(r#"{{"$schema":"{DRAFT4}","type":"integer"}}"#),
            "18446744073709551615",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_semantics_setting_overrides_draft() {
        let settings: crate::json_schema::JsonSchemaSettings =
            crate::json_schema::JsonSchemaSettings::builder()
                .integer_semantics(crate::json_schema::IntegerSemantics::Numeric)
                .build();
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_text(
            &format!(r#"{{"$schema":"{DRAFT4}","type":"integer"}}"#),
            "1.0",
            &settings,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_type_error_and_bound_error_are_both_reported() {
        let expected: ValidationResult = Err(vec![
            ValidationError::ExpectedInteger {
                instance_path: JsonPointer::root(),
                got: "number".to_string(),
            },
            ValidationError::BelowMinimum {
                instance_path: JsonPointer::root(),
                minimum: num("2"),
                actual: num("1.5"),
            },
        ]);
        let actual: ValidationResult = validate_text(
            r#"{"type":"integer","minimum":2}"#,
            "1.5",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn minimum_beyond_f64_precision_rejects_smaller_integer() {
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root(),
            minimum: num("9007199254740993"),
            actual: num("9007199254740992"),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"type":"integer","minimum":9007199254740993}"#,
            "9007199254740992",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn maximum_u64_max_accepts_u64_max() {
        let expected: bool = true;
        let actual: bool = is_valid(
            r#"{"type":"integer","maximum":18446744073709551615}"#,
            "18446744073709551615",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn maximum_integer_rejects_float_just_above() {
        let expected: ValidationResult = Err(vec![ValidationError::AboveMaximum {
            instance_path: JsonPointer::root(),
            maximum: num("300"),
            actual: num("300.5"),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"maximum":300}"#,
            "300.5",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn minimum_applies_without_type() {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"minimum":1.1}"#, "0.6");
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_keywords_ignore_non_numbers() {
        let expected: bool = true;
        let actual: bool = is_valid(
            r#"{"minimum":1,"maximum":0,"exclusiveMinimum":1,"exclusiveMaximum":0,"multipleOf":7}"#,
            r#""x""#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_minimum_rejects_boundary() {
        let expected: ValidationResult = Err(vec![ValidationError::NotAboveExclusiveMinimum {
            instance_path: JsonPointer::root(),
            exclusive_minimum: num("1.1"),
            actual: num("1.1"),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"exclusiveMinimum":1.1}"#,
            "1.1",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_minimum_accepts_above() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"exclusiveMinimum":1.1}"#, "1.2");
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_maximum_rejects_boundary() {
        let expected: ValidationResult = Err(vec![ValidationError::NotBelowExclusiveMaximum {
            instance_path: JsonPointer::root(),
            exclusive_maximum: num("3"),
            actual: num("3.0"),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"exclusiveMaximum":3}"#,
            "3.0",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_maximum_accepts_below() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"exclusiveMaximum":3}"#, "2.9");
        assert_eq!(expected, actual);
    }

    #[test]
    fn exclusive_maximum_equal_large_float_is_invalid() {
        let expected: bool = false;
        let actual: bool = is_valid(
            r#"{"exclusiveMaximum":9.727837981879871e+26}"#,
            "9.727837981879871e+26",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_boolean_exclusive_minimum_rejects_boundary() {
        let expected: ValidationResult = Err(vec![ValidationError::NotAboveExclusiveMinimum {
            instance_path: JsonPointer::root(),
            exclusive_minimum: num("1.1"),
            actual: num("1.1"),
        }]);
        let actual: ValidationResult = validate_text(
            &format!(r#"{{"$schema":"{DRAFT4}","minimum":1.1,"exclusiveMinimum":true}}"#),
            "1.1",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_false_exclusive_maximum_keeps_inclusive_boundary() {
        let expected: bool = true;
        let actual: bool = is_valid(
            &format!(r#"{{"$schema":"{DRAFT4}","maximum":3.0,"exclusiveMaximum":false}}"#),
            "3.0",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_rejects_non_multiple() {
        let expected: ValidationResult = Err(vec![ValidationError::NotMultipleOf {
            instance_path: JsonPointer::root(),
            multiple_of: num("1.5"),
            actual: num("35"),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"multipleOf":1.5}"#,
            "35",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_accepts_small_decimal_multiple() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"multipleOf":0.0001}"#, "0.0075");
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_integer_accepts_huge_whole_float() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"type":"integer","multipleOf":0.5}"#, "1e308");
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_matches_mathematically_equal_number() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"enum":[0]}"#, "0.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_matches_mathematically_equal_nested_number() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"enum":[[1]]}"#, "[1.0]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_number_does_not_match_boolean() {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"enum":[0]}"#, "false");
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_matches_mathematically_equal_number() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"const":-2.0}"#, "-2");
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_null_rejects_zero() {
        let expected: ValidationResult = Err(vec![ValidationError::NotConst {
            instance_path: JsonPointer::root(),
            expected: "null".to_string(),
            actual: "0".to_string(),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"const":null}"#,
            "0",
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_null_accepts_null() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"const":null}"#, "null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_rejects_mathematically_equal_numbers() {
        let expected: bool = false;
        let actual: bool = is_valid(r#"{"type":"array","uniqueItems":true}"#, "[1, 1.0]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_applies_without_type() {
        let expected: ValidationResult = Err(vec![ValidationError::DuplicateArrayItems {
            instance_path: JsonPointer::root(),
            duplicate_value: r#"{"a":1,"b":2}"#.to_string(),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"uniqueItems":true}"#,
            r#"[{"a": 1, "b": 2}, {"b": 2, "a": 1}]"#,
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_accepts_distinct_values() {
        let expected: bool = true;
        let actual: bool = is_valid(
            r#"{"uniqueItems":true}"#,
            r#"[0, false, null, "0", [0], {"a": 0}, 1.5]"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_rejects_duplicate_in_long_array() {
        let mut items: Vec<String> = (0..40).map(|i| i.to_string()).collect();
        items.push("17.0".to_string());
        let expected: ValidationResult = Err(vec![ValidationError::DuplicateArrayItems {
            instance_path: JsonPointer::root(),
            duplicate_value: "17.0".to_string(),
        }]);
        let actual: ValidationResult = validate_text(
            r#"{"uniqueItems":true}"#,
            &format!("[{}]", items.join(",")),
            &crate::json_schema::JsonSchemaSettings::default(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_accepts_distinct_long_array() {
        let items: Vec<String> = (0..40).map(|i| format!("{i}.5")).collect();
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"uniqueItems":true}"#, &format!("[{}]", items.join(",")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_false_allows_duplicates() {
        let expected: bool = true;
        let actual: bool = is_valid(r#"{"uniqueItems":false}"#, "[1, 1]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_error_messages_show_exact_values() {
        let errors: Vec<ValidationError> = validate_text(
            r#"{"minimum":9007199254740993,"exclusiveMaximum":0,"multipleOf":2}"#,
            "9007199254740991",
            &crate::json_schema::JsonSchemaSettings::default(),
        )
        .expect_err("invalid");
        let expected: Vec<String> = vec![
            "root: value 9007199254740991 is below minimum 9007199254740993".to_string(),
            "root: value 9007199254740991 is not below exclusiveMaximum 0".to_string(),
            "root: value 9007199254740991 is not a multiple of 2".to_string(),
        ];
        let actual: Vec<String> = errors.iter().map(ToString::to_string).collect();
        assert_eq!(expected, actual);
    }

    /// Validates `instance_json` against the schema document `schema_json` (which may be a
    /// boolean schema) under default settings.
    fn validate_document(schema_json: &str, instance_json: &str) -> ValidationResult {
        let schema: Schema = Schema::try_from(schema_json).expect("schema");
        let instance: serde_json::Value = serde_json::from_str(instance_json).expect("instance");
        validate(&schema, &instance)
    }

    /// `type: ["integer", "string"]`.
    fn integer_or_string() -> crate::json_schema::TypeSet {
        [JsonType::Integer, JsonType::String].into_iter().collect()
    }

    #[test]
    fn type_array_accepts_listed_integer() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"type":["integer","string"]}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_array_accepts_listed_string() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult =
            validate_document(r#"{"type":["integer","string"]}"#, r#""x""#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_array_rejects_unlisted_type() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedType {
            instance_path: JsonPointer::root(),
            expected: integer_or_string(),
            got: "boolean".to_string(),
        }]);
        let actual: ValidationResult =
            validate_document(r#"{"type":["integer","string"]}"#, "true");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_null_accepts_null() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"type":"null"}"#, "null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_null_rejects_string() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNull {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        }]);
        let actual: ValidationResult = validate_document(r#"{"type":"null"}"#, r#""x""#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_null_rejects_number() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedNull {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        let actual: ValidationResult = validate_document(r#"{"type":"null"}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn empty_type_set_rejects_null() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedType {
            instance_path: JsonPointer::root(),
            expected: crate::json_schema::TypeSet::empty(),
            got: "null".to_string(),
        }]);
        let actual: ValidationResult = validate_document(r#"{"type":[]}"#, "null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn empty_type_set_rejects_object() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedType {
            instance_path: JsonPointer::root(),
            expected: crate::json_schema::TypeSet::empty(),
            got: "object".to_string(),
        }]);
        let actual: ValidationResult = validate_document(r#"{"type":[]}"#, "{}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_array_with_integer_rejects_zero_fraction_float_under_draft4() {
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedType {
            instance_path: JsonPointer::root(),
            expected: integer_or_string(),
            got: "number".to_string(),
        }]);
        let actual: ValidationResult = validate_document(
            &format!(r#"{{"$schema":"{DRAFT4}","type":["integer","string"]}}"#),
            "1.0",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_array_with_integer_accepts_zero_fraction_float_under_2020_12() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(
            r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":["integer","string"]}"#,
            "1.0",
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_true_schema_accepts_anything() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document("true", r#"{"a":[1,null]}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_false_schema_rejects_at_root() {
        let expected: ValidationResult = Err(vec![ValidationError::FalseSchema {
            instance_path: JsonPointer::root(),
        }]);
        let actual: ValidationResult = validate_document("false", "null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_true_schema_ref_accepts_anything() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate(SchemaRef::Bool(true), &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_false_schema_ref_rejects_at_root() {
        let expected: ValidationResult = Err(vec![ValidationError::FalseSchema {
            instance_path: JsonPointer::root(),
        }]);
        let actual: ValidationResult = validate(SchemaRef::Bool(false), &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn false_property_schema_rejects_present_property() {
        let expected: ValidationResult = Err(vec![ValidationError::FalseSchema {
            instance_path: JsonPointer::root().push("a"),
        }]);
        let actual: ValidationResult =
            validate_document(r#"{"properties":{"a":false}}"#, r#"{"a":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn false_property_schema_accepts_absent_property() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult =
            validate_document(r#"{"properties":{"a":false}}"#, r#"{"b":1}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn false_items_rejects_each_element() {
        let expected: ValidationResult = Err(vec![
            ValidationError::FalseSchema {
                instance_path: JsonPointer::root().push("0"),
            },
            ValidationError::FalseSchema {
                instance_path: JsonPointer::root().push("1"),
            },
        ]);
        let actual: ValidationResult = validate_document(r#"{"items":false}"#, "[1,2]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn false_items_accepts_empty_array() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"items":false}"#, "[]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn true_items_accepts_any_elements() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"items":true}"#, r#"[1,"a",null]"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_true_allows_extra_key() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(
            r#"{"properties":{"a":{"type":"string"}},"additionalProperties":true}"#,
            r#"{"a":"x","b":1}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_false_def_rejects() {
        let expected: ValidationResult = Err(vec![ValidationError::FalseSchema {
            instance_path: JsonPointer::root().push("a"),
        }]);
        let actual: ValidationResult = validate_document(
            r##"{"$defs":{"never":false},"properties":{"a":{"$ref":"#/$defs/never"}}}"##,
            r#"{"a":1}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_true_def_accepts() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(
            r##"{"$defs":{"any":true},"properties":{"a":{"$ref":"#/$defs/any"}}}"##,
            r#"{"a":1}"#,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn any_of_false_then_true_passes() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"anyOf":[false,true]}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_true_and_false_fails() {
        let expected: ValidationResult = Err(vec![ValidationError::FalseSchema {
            instance_path: JsonPointer::root(),
        }]);
        let actual: ValidationResult = validate_document(r#"{"allOf":[true,false]}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_two_true_schemas_fails() {
        let expected: ValidationResult = Err(vec![ValidationError::MultipleSubschemasMatched {
            instance_path: JsonPointer::root(),
            subschema_count: 2,
            match_count: 2,
        }]);
        let actual: ValidationResult = validate_document(r#"{"oneOf":[true,true]}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_length_applies_without_type() {
        let expected: ValidationResult = Err(vec![ValidationError::TooShort {
            instance_path: JsonPointer::root(),
            min_length: 2,
            actual_length: 1,
        }]);
        let actual: ValidationResult = validate_document(r#"{"minLength":2}"#, r#""a""#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_length_ignores_non_string_without_type() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult = validate_document(r#"{"minLength":2}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn min_items_applies_without_type() {
        let expected: ValidationResult = Err(vec![ValidationError::TooFewItems {
            instance_path: JsonPointer::root(),
            min_items: 2,
            actual_count: 1,
        }]);
        let actual: ValidationResult = validate_document(r#"{"minItems":2}"#, "[1]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_applies_with_object_or_null_type() {
        let expected: ValidationResult = Err(vec![ValidationError::MissingRequired {
            instance_path: JsonPointer::root().push("a"),
            property: "a".to_string(),
        }]);
        let actual: ValidationResult =
            validate_document(r#"{"type":["object","null"],"required":["a"]}"#, "{}");
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_ignores_null_with_object_or_null_type() {
        let expected: ValidationResult = Ok(());
        let actual: ValidationResult =
            validate_document(r#"{"type":["object","null"],"required":["a"]}"#, "null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_mismatch_reports_other_keywords_too() {
        let expected: ValidationResult = Err(vec![
            ValidationError::ExpectedString {
                instance_path: JsonPointer::root(),
                got: "number".to_string(),
            },
            ValidationError::BelowMinimum {
                instance_path: JsonPointer::root(),
                minimum: num("5"),
                actual: num("1"),
            },
        ]);
        let actual: ValidationResult = validate_document(r#"{"type":"string","minimum":5}"#, "1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn validation_error_false_schema_display() {
        let err: ValidationError = ValidationError::FalseSchema {
            instance_path: JsonPointer::root().push("a"),
        };
        let expected: String = "/a: no value is allowed here (schema is false)".to_string();
        let actual: String = err.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn validation_error_expected_null_display() {
        let err: ValidationError = ValidationError::ExpectedNull {
            instance_path: JsonPointer::root(),
            got: "string".to_string(),
        };
        let expected: String = "root: expected null, got string".to_string();
        let actual: String = err.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn validation_error_expected_type_display() {
        let err: ValidationError = ValidationError::ExpectedType {
            instance_path: JsonPointer::root(),
            expected: integer_or_string(),
            got: "boolean".to_string(),
        };
        let expected: String = "root: expected one of [integer, string], got boolean".to_string();
        let actual: String = err.to_string();
        assert_eq!(expected, actual);
    }
}
