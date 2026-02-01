//! Schema validation for `deny_invalid_unknown_json_schema` mode.
//!
//! Walks the raw JSON Schema (as `serde_json::Value`) and collects all
//! invalid/unsupported issues without panicking.

use crate::error::{SchemaValidationError, SchemaValidationIssue, SchemaValidationIssueKind};
use crate::json_pointer;
use std::collections::BTreeSet;

/// Known JSON Schema keywords we support or explicitly recognize.
/// Keys not in this set and not in UNSUPPORTED are reported as `UnknownKeyword`.
const KNOWN_KEYWORDS: &[&str] = &[
    "title",
    "description",
    "type",
    "properties",
    "required",
    "optional",
    "enum",
    "items",
    "format",
    "additionalProperties",
    "default",
    "minimum",
    "maximum",
];

fn known_keywords_set() -> BTreeSet<&'static str> {
    KNOWN_KEYWORDS.iter().copied().collect()
}

/// Maps unsupported keyword name to issue kind.
fn unsupported_kind(key: &str) -> Option<SchemaValidationIssueKind> {
    let kind = match key {
        "$ref" => SchemaValidationIssueKind::UnsupportedKeywordRef,
        "$defs" => SchemaValidationIssueKind::UnsupportedKeywordDefs,
        "definitions" => SchemaValidationIssueKind::UnsupportedKeywordDefinitions,
        "minLength" => SchemaValidationIssueKind::UnsupportedKeywordMinLength,
        "maxLength" => SchemaValidationIssueKind::UnsupportedKeywordMaxLength,
        "pattern" => SchemaValidationIssueKind::UnsupportedKeywordPattern,
        "oneOf" => SchemaValidationIssueKind::UnsupportedKeywordOneOf,
        "anyOf" => SchemaValidationIssueKind::UnsupportedKeywordAnyOf,
        "allOf" => SchemaValidationIssueKind::UnsupportedKeywordAllOf,
        "$id" => SchemaValidationIssueKind::UnsupportedKeywordId,
        "examples" => SchemaValidationIssueKind::UnsupportedKeywordExamples,
        "const" => SchemaValidationIssueKind::UnsupportedKeywordConst,
        "not" => SchemaValidationIssueKind::UnsupportedKeywordNot,
        "minProperties" => SchemaValidationIssueKind::UnsupportedKeywordMinProperties,
        "maxProperties" => SchemaValidationIssueKind::UnsupportedKeywordMaxProperties,
        "minItems" => SchemaValidationIssueKind::UnsupportedKeywordMinItems,
        "maxItems" => SchemaValidationIssueKind::UnsupportedKeywordMaxItems,
        "uniqueItems" => SchemaValidationIssueKind::UnsupportedKeywordUniqueItems,
        "exclusiveMinimum" => SchemaValidationIssueKind::UnsupportedKeywordExclusiveMinimum,
        "exclusiveMaximum" => SchemaValidationIssueKind::UnsupportedKeywordExclusiveMaximum,
        "multipleOf" => SchemaValidationIssueKind::UnsupportedKeywordMultipleOf,
        "readOnly" => SchemaValidationIssueKind::UnsupportedKeywordReadOnly,
        "writeOnly" => SchemaValidationIssueKind::UnsupportedKeywordWriteOnly,
        "deprecated" => SchemaValidationIssueKind::UnsupportedKeywordDeprecated,
        "propertyNames" => SchemaValidationIssueKind::UnsupportedKeywordPropertyNames,
        "additionalItems" => SchemaValidationIssueKind::UnsupportedKeywordAdditionalItems,
        "optional" => SchemaValidationIssueKind::UnsupportedKeywordOptional,
        _ => return None,
    };
    Some(kind)
}

/// Supported JSON Schema types for properties/items.
const SUPPORTED_TYPES: &[&str] = &["string", "boolean", "integer", "number", "object", "array"];

fn supported_type(ty: &str) -> bool {
    SUPPORTED_TYPES.contains(&ty)
}

/// Validates the schema (parsed as Value). Returns Ok(()) if no issues, or
/// Err(SchemaValidationError) with all collected issues.
pub fn validate_schema(value: &serde_json::Value) -> Result<(), SchemaValidationError> {
    let mut issues: Vec<SchemaValidationIssue> = Vec::new();
    let known = known_keywords_set();

    let serde_json::Value::Object(obj) = value else {
        issues.push(SchemaValidationIssue {
            path: String::new(),
            kind: SchemaValidationIssueKind::RootNotObject,
        });
        return Err(SchemaValidationError { issues });
    };

    // Root-level checks
    let root_type = obj.get("type");
    if root_type.is_none() {
        issues.push(SchemaValidationIssue {
            path: String::new(),
            kind: SchemaValidationIssueKind::RootMissingType,
        });
    } else if let Some(serde_json::Value::String(s)) = root_type {
        if s != "object" {
            issues.push(SchemaValidationIssue {
                path: String::new(),
                kind: SchemaValidationIssueKind::RootNotObject,
            });
        }
    } else if let Some(serde_json::Value::Array(_)) = root_type {
        issues.push(SchemaValidationIssue {
            path: String::new(),
            kind: SchemaValidationIssueKind::TypeArrayNotSupported,
        });
    }

    collect_validation_issues(value, "", &mut issues, true, &known);
    if root_type.as_ref().and_then(|t| t.as_str()) == Some("object") {
        let props = obj.get("properties");
        let has_props = props
            .and_then(|p| p.as_object())
            .is_some_and(|o| !o.is_empty());
        if !has_props {
            issues.push(SchemaValidationIssue {
                path: String::new(),
                kind: SchemaValidationIssueKind::NoStructsToGenerate,
            });
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(SchemaValidationError { issues })
    }
}

fn push_issue(
    issues: &mut Vec<SchemaValidationIssue>,
    path: &str,
    kind: SchemaValidationIssueKind,
) {
    issues.push(SchemaValidationIssue {
        path: path.to_string(),
        kind,
    });
}

fn collect_validation_issues(
    value: &serde_json::Value,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
    _is_root: bool,
    known: &BTreeSet<&'static str>,
) {
    let Some(obj) = value.as_object() else {
        return;
    };

    // type "array" without "items"
    if obj.get("type").and_then(|t| t.as_str()) == Some("array") && obj.get("items").is_none() {
        push_issue(issues, path, SchemaValidationIssueKind::ArrayMissingItems);
    }

    // Check each key: unknown, unsupported, or validate structure
    for (key, val) in obj {
        let key_path = json_pointer::format(path, key);

        if known.contains(key.as_str()) {
            // Validate structure for known keys
            match key.as_str() {
                "type" => validate_type(val, &key_path, issues),
                "required" => validate_required(val, path, obj, &key_path, issues),
                "enum" => validate_enum(val, &key_path, issues),
                "items" => {
                    validate_items(val, path, obj, &key_path, issues);
                    if let Some(item_obj) = val.as_object() {
                        collect_validation_issues(
                            &serde_json::Value::Object(item_obj.clone()),
                            &key_path,
                            issues,
                            false,
                            known,
                        );
                    }
                }
                "properties" => {
                    if let Some(properties) = val.as_object() {
                        for (prop_name, prop_schema) in properties {
                            let prop_path = json_pointer::format(&key_path, prop_name);
                            collect_validation_issues(
                                prop_schema,
                                &prop_path,
                                issues,
                                false,
                                known,
                            );
                        }
                    }
                }
                "additionalProperties" => {
                    validate_additional_properties(val, &key_path, issues, known);
                }
                "default" => validate_default(val, path, obj, &key_path, issues),
                "minimum" | "maximum" => validate_min_max(val, &key_path, issues),
                _ => {}
            }
        } else if let Some(kind) = unsupported_kind(key) {
            push_issue(issues, &key_path, kind);
        } else {
            push_issue(
                issues,
                &key_path,
                SchemaValidationIssueKind::UnknownKeyword(key.clone()),
            );
        }
    }
}

fn validate_type(value: &serde_json::Value, path: &str, issues: &mut Vec<SchemaValidationIssue>) {
    match value {
        serde_json::Value::String(s) => {
            if s == "null" {
                push_issue(
                    issues,
                    path,
                    SchemaValidationIssueKind::NullTypeNotSupported,
                );
            } else if !supported_type(s) {
                push_issue(
                    issues,
                    path,
                    SchemaValidationIssueKind::PropertyWithUnsupportedType,
                );
            }
        }
        serde_json::Value::Array(_) => {
            push_issue(
                issues,
                path,
                SchemaValidationIssueKind::TypeArrayNotSupported,
            );
        }
        _ => {
            push_issue(issues, path, SchemaValidationIssueKind::InvalidTypeValue);
        }
    }
}

fn validate_required(
    value: &serde_json::Value,
    _parent_path: &str,
    parent_obj: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    let Some(arr) = value.as_array() else {
        push_issue(
            issues,
            path,
            SchemaValidationIssueKind::InvalidRequiredFormat,
        );
        return;
    };
    let properties = parent_obj.get("properties").and_then(|p| p.as_object());
    let prop_keys: BTreeSet<&str> = properties
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    for item in arr {
        if let Some(name) = item.as_str() {
            if !prop_keys.contains(name) {
                push_issue(
                    issues,
                    path,
                    SchemaValidationIssueKind::RequiredPropertyNotInProperties,
                );
                break;
            }
        } else {
            push_issue(
                issues,
                path,
                SchemaValidationIssueKind::InvalidRequiredFormat,
            );
            break;
        }
    }
}

fn validate_enum(value: &serde_json::Value, path: &str, issues: &mut Vec<SchemaValidationIssue>) {
    let Some(arr) = value.as_array() else {
        push_issue(issues, path, SchemaValidationIssueKind::InvalidEnumFormat);
        return;
    };
    if arr.is_empty() {
        push_issue(issues, path, SchemaValidationIssueKind::EnumEmpty);
    }
    for v in arr {
        if !v.is_string() {
            push_issue(
                issues,
                path,
                SchemaValidationIssueKind::EnumContainsNonStringValues,
            );
            break;
        }
    }
}

fn validate_items(
    value: &serde_json::Value,
    _parent_path: &str,
    parent_obj: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    let parent_type = parent_obj.get("type").and_then(|t| t.as_str());
    if parent_type == Some("array") && !value.is_object() {
        push_issue(issues, path, SchemaValidationIssueKind::InvalidItemsFormat);
    }
}

fn validate_additional_properties(
    value: &serde_json::Value,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
    known: &BTreeSet<&'static str>,
) {
    if value.is_boolean() {
        return;
    }
    if let Some(o) = value.as_object() {
        let type_val = o.get("type").and_then(|t| t.as_str());
        if type_val.is_none() {
            push_issue(
                issues,
                path,
                SchemaValidationIssueKind::AdditionalPropertiesUnsupportedSchema,
            );
        } else if let Some(ty) = type_val
            && !supported_type(ty)
        {
            push_issue(
                issues,
                path,
                SchemaValidationIssueKind::AdditionalPropertiesUnsupportedSchema,
            );
        }
        collect_validation_issues(
            &serde_json::Value::Object(o.clone()),
            path,
            issues,
            false,
            known,
        );
    } else {
        push_issue(
            issues,
            path,
            SchemaValidationIssueKind::AdditionalPropertiesUnsupportedSchema,
        );
    }
}

fn validate_default(
    value: &serde_json::Value,
    _parent_path: &str,
    _parent_obj: &serde_json::Map<String, serde_json::Value>,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    if value.is_null() {
        return;
    }
    if value.is_object() {
        push_issue(
            issues,
            path,
            SchemaValidationIssueKind::UnsupportedDefaultObject,
        );
        return;
    }
    if let Some(arr) = value.as_array()
        && !arr.is_empty()
    {
        push_issue(
            issues,
            path,
            SchemaValidationIssueKind::UnsupportedDefaultNonEmptyArray,
        );
    }
    // Type-based validation: we could check default matches type, but that's complex; leave InvalidDefaultValue for edge cases if needed
}

fn validate_min_max(
    value: &serde_json::Value,
    path: &str,
    issues: &mut Vec<SchemaValidationIssue>,
) {
    let is_number = value.is_i64() || value.is_u64() || value.is_f64();
    if !is_number {
        push_issue(
            issues,
            path,
            SchemaValidationIssueKind::InvalidMinimumMaximum,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_not_object() {
        let v = serde_json::json!("string");
        let err = validate_schema(&v).unwrap_err();
        assert_eq!(err.issues.len(), 1);
        assert!(matches!(
            err.issues[0].kind,
            SchemaValidationIssueKind::RootNotObject
        ));
    }

    #[test]
    fn root_missing_type() {
        let v = serde_json::json!({});
        let err = validate_schema(&v).unwrap_err();
        assert!(
            err.issues
                .iter()
                .any(|i| matches!(i.kind, SchemaValidationIssueKind::RootMissingType))
        );
    }

    #[test]
    fn root_not_object_type() {
        let v = serde_json::json!({ "type": "string" });
        let err = validate_schema(&v).unwrap_err();
        assert!(
            err.issues
                .iter()
                .any(|i| matches!(i.kind, SchemaValidationIssueKind::RootNotObject))
        );
    }

    #[test]
    fn unsupported_ref() {
        let v = serde_json::json!({
            "type": "object",
            "properties": {},
            "$ref": "#/definitions/Foo"
        });
        let err = validate_schema(&v).unwrap_err();
        assert!(
            err.issues
                .iter()
                .any(|i| matches!(i.kind, SchemaValidationIssueKind::UnsupportedKeywordRef))
        );
    }

    #[test]
    fn unsupported_one_of() {
        let v = serde_json::json!({
            "type": "object",
            "oneOf": [{ "type": "string" }, { "type": "integer" }]
        });
        let err = validate_schema(&v).unwrap_err();
        assert!(
            err.issues
                .iter()
                .any(|i| matches!(i.kind, SchemaValidationIssueKind::UnsupportedKeywordOneOf))
        );
    }

    #[test]
    fn multiple_issues_collected() {
        let v = serde_json::json!({
            "type": "object",
            "properties": {},
            "$ref": "#/Foo",
            "oneOf": []
        });
        let err = validate_schema(&v).unwrap_err();
        assert!(err.issues.len() >= 2);
        let kinds: Vec<_> = err.issues.iter().map(|i| &i.kind).collect();
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, SchemaValidationIssueKind::UnsupportedKeywordRef))
        );
        assert!(
            kinds
                .iter()
                .any(|k| matches!(k, SchemaValidationIssueKind::UnsupportedKeywordOneOf))
        );
    }

    #[test]
    fn valid_minimal_schema_passes() {
        let v = serde_json::json!({
            "type": "object",
            "properties": {
                "foo": { "type": "string" }
            }
        });
        assert!(validate_schema(&v).is_ok());
    }
}
