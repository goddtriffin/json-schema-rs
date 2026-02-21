//! Rust codegen backend: emits serde-compatible Rust structs from JSON Schema.

use super::CodeGenBackend;
use super::CodeGenError;
use super::CodeGenResult;
use super::settings::{CodeGenSettings, ModelNameSource};
use crate::json_schema::JsonSchema;
use crate::sanitizers::{sanitize_field_name, sanitize_struct_name};
use std::collections::BTreeSet;
use std::io::{Cursor, Write};

/// Backend that emits Rust structs (serde-compatible).
#[derive(Debug, Clone, Default)]
pub struct RustBackend;

impl CodeGenBackend for RustBackend {
    fn generate(
        &self,
        schemas: &[JsonSchema],
        settings: &CodeGenSettings,
    ) -> CodeGenResult<Vec<Vec<u8>>> {
        let mut results: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            let mut out = Cursor::new(Vec::new());
            emit_rust(schema, &mut out, settings).map_err(|e| CodeGenError::Batch {
                index,
                source: Box::new(e),
            })?;
            results.push(out.into_inner());
        }
        Ok(results)
    }
}

/// One struct to emit: name and the object schema (root or nested).
struct StructToEmit {
    name: String,
    schema: JsonSchema,
}

/// Compute struct/type name from title, property key, and root fallback per settings.
fn struct_name_from(
    title: Option<&str>,
    from_key: Option<&str>,
    is_root: bool,
    settings: &CodeGenSettings,
) -> String {
    let title_trimmed: Option<&str> = title.filter(|t| !t.trim().is_empty()).map(str::trim);
    let from_key_s: Option<&str> = from_key;

    let (first, second) = match settings.model_name_source {
        ModelNameSource::TitleFirst => (title_trimmed, from_key_s),
        ModelNameSource::PropertyKeyFirst => (from_key_s, title_trimmed),
    };

    first
        .map(sanitize_struct_name)
        .or_else(|| second.map(sanitize_struct_name))
        .unwrap_or_else(|| {
            if is_root {
                "Root".to_string()
            } else {
                "Unnamed".to_string()
            }
        })
}

/// Collect all object schemas that need a struct in topological order (children before parents).
/// Uses an explicit stack to avoid recursion and stack overflow on deep schemas.
fn collect_structs(
    schema: &JsonSchema,
    from_key: Option<&str>,
    out: &mut Vec<StructToEmit>,
    seen: &mut BTreeSet<String>,
    settings: &CodeGenSettings,
) {
    if !schema.is_object_with_properties() {
        return;
    }

    // Phase 1: iterative post-order DFS to collect (schema, from_key) so children come before parents.
    let mut post_order: Vec<(JsonSchema, Option<String>, bool)> = Vec::new();
    let mut stack: Vec<(JsonSchema, Option<String>, usize, bool)> = Vec::new();
    stack.push((
        schema.clone(),
        from_key.map(String::from),
        0,
        from_key.is_none(),
    ));

    while let Some((schema_node, from_key_opt, index, is_root)) = stack.pop() {
        let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
        if index < keys.len() {
            let key: String = keys.get(index).unwrap().clone();
            let child: JsonSchema = schema_node.properties.get(&key).unwrap().clone();
            stack.push((schema_node, from_key_opt, index + 1, is_root));
            if child.is_object_with_properties() {
                stack.push((child, Some(key.clone()), 0, false));
            }
        } else {
            post_order.push((schema_node, from_key_opt, is_root));
        }
    }

    // Phase 2: emit in post-order, dedupe by name (first occurrence wins).
    for (schema_node, from_key_opt, is_root) in post_order {
        let name: String = struct_name_from(
            schema_node.title.as_deref(),
            from_key_opt.as_deref(),
            is_root,
            settings,
        );

        if seen.contains(&name) {
            continue;
        }
        seen.insert(name.clone());

        out.push(StructToEmit {
            name,
            schema: schema_node,
        });
    }
}

/// Emit a single struct's fields to `out`.
fn emit_struct_fields(
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
) -> CodeGenResult<()> {
    for (key, prop_schema) in &schema.properties {
        let field_name = sanitize_field_name(key);
        let needs_rename = field_name != *key;

        if prop_schema.is_string() {
            let ty = if schema.is_required(key) {
                "String".to_string()
            } else {
                "Option<String>".to_string()
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        } else if prop_schema.is_object_with_properties() {
            let nested_name: String =
                struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings);
            let ty = if schema.is_required(key) {
                nested_name.clone()
            } else {
                format!("Option<{nested_name}>")
            };
            if needs_rename {
                writeln!(out, "    #[serde(rename = \"{key}\")]")?;
            }
            writeln!(out, "    pub {field_name}: {ty},")?;
        }
    }
    Ok(())
}

/// Emit Rust source from a parsed schema to `out`. Used by [`RustBackend::generate`].
fn emit_rust(
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
) -> CodeGenResult<()> {
    if !schema.is_object_with_properties() {
        return Err(CodeGenError::RootNotObject);
    }

    let mut structs: Vec<StructToEmit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    collect_structs(schema, None, &mut structs, &mut seen, settings);

    writeln!(
        out,
        "//! Generated by json-schema-rs. Do not edit manually."
    )?;
    writeln!(out)?;
    writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
    writeln!(out)?;

    for st in &structs {
        writeln!(out, "#[derive(Debug, Clone, Serialize, Deserialize)]")?;
        writeln!(out, "pub struct {} {{", st.name)?;
        emit_struct_fields(&st.schema, out, settings)?;
        writeln!(out, "}}")?;
        writeln!(out)?;
    }

    Ok(())
}

/// Generate Rust source from one or more parsed schemas.
///
/// Callers must pass `settings` (use [`CodeGenSettings::builder`] and call [`CodeGenSettingsBuilder::build`]
/// for all-default settings). Returns one byte buffer per schema; each buffer is UTF-8 Rust source.
/// Root of each schema must have `type: "object"` and non-empty `properties`.
///
/// # Errors
///
/// Returns [`CodeGenError::RootNotObject`] if a root schema is not an object with properties.
/// Returns [`CodeGenError::Io`] on write failure.
/// Returns [`CodeGenError::Batch`] with index when one schema in the batch fails.
pub fn generate_rust(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> CodeGenResult<Vec<Vec<u8>>> {
    RustBackend.generate(schemas, settings)
}

#[cfg(test)]
mod tests {
    use super::CodeGenError;
    use super::{CodeGenBackend, RustBackend, generate_rust};
    use crate::code_gen::settings::{CodeGenSettings, ModelNameSource};
    use crate::json_schema::JsonSchema;

    fn default_settings() -> CodeGenSettings {
        CodeGenSettings::builder().build()
    }

    #[test]
    fn root_not_object_errors() {
        let schema: JsonSchema = JsonSchema::default();
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::RootNotObject));
        }
    }

    #[test]
    fn root_object_empty_properties_errors() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            ..Default::default()
        };
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[schema], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 0, .. }));
        if let CodeGenError::Batch { source, .. } = actual {
            assert!(matches!(*source, CodeGenError::RootNotObject));
        }
    }

    #[test]
    fn single_string_property() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_field_emits_without_option() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_object_and_rename() {
        let json = r#"{
          "type": "object",
          "properties": {
            "first_name": { "type": "string" },
            "address": {
              "type": "object",
              "properties": {
                "street_address": { "type": "string" },
                "city": { "type": "string" }
              }
            }
          }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub city: Option<String>,
    pub street_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub address: Option<Address>,
    pub first_name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn full_example_from_plan() {
        let json = r#"{
          "type": "object",
          "properties": {
            "first_name": { "type": "string" },
            "last_name": { "type": "string" },
            "birthday": { "type": "string" },
            "address": {
              "type": "object",
              "properties": {
                "street_address": { "type": "string" },
                "city": { "type": "string" },
                "state": { "type": "string" },
                "country": { "type": "string" }
              }
            }
          }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        let expected = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub city: Option<String>,
    pub country: Option<String>,
    pub state: Option<String>,
    pub street_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub address: Option<Address>,
    pub birthday: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn deeply_nested_schema_does_not_stack_overflow() {
        const DEPTH: usize = 150;
        let mut inner: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    JsonSchema {
                        type_: Some("string".to_string()),
                        ..Default::default()
                    },
                );
                m
            },
            required: None,
            title: Some("Leaf".to_string()),
        };
        for i in (0..DEPTH).rev() {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some("object".to_string()),
                properties: std::collections::BTreeMap::new(),
                required: None,
                title: Some(format!("Level{i}")),
            };
            wrap.properties.insert("child".to_string(), inner);
            inner = wrap;
        }
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[inner], &settings);
        assert!(actual.is_ok(), "deep schema must not overflow");
        let bytes = actual.unwrap();
        let output: String = String::from_utf8(bytes[0].clone()).unwrap();
        assert!(
            output.contains("pub struct Level0"),
            "output must contain root struct"
        );
        assert!(
            output.contains("pub struct Leaf"),
            "output must contain leaf struct"
        );
    }

    #[test]
    fn field_rename_when_key_differs_from_identifier() {
        let json = r#"{"type":"object","properties":{"foo-bar":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        let expected = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    #[serde(rename = "foo-bar")]
    pub foo_bar: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn generate_rust_one_schema_returns_one_buffer() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(std::slice::from_ref(&schema), &settings).unwrap();
        let expected: Vec<Vec<u8>> = RustBackend.generate(&[schema], &settings).unwrap();
        assert_eq!(expected, bytes);
        assert_eq!(1, bytes.len());
    }

    #[test]
    fn property_key_first_uses_key_over_title_for_nested_struct() {
        let json = r#"{"type":"object","properties":{"address":{"type":"object","title":"FooBar","properties":{"city":{"type":"string"}}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .model_name_source(ModelNameSource::PropertyKeyFirst)
            .build();
        let bytes = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(bytes[0].clone()).unwrap();
        assert!(
            actual.contains("pub struct Address "),
            "with PropertyKeyFirst nested struct should be named from key 'address' -> Address; got: {actual}"
        );
        assert!(
            !actual.contains("struct FooBar "),
            "with PropertyKeyFirst title FooBar should not be used for nested name; got: {actual}"
        );
    }

    #[test]
    fn generate_rust_two_schemas_returns_two_buffers() {
        let json1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let json2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json1).unwrap();
        let s2: JsonSchema = serde_json::from_str(json2).unwrap();
        let settings: CodeGenSettings = default_settings();
        let bytes = generate_rust(&[s1.clone(), s2.clone()], &settings).unwrap();
        let expected = RustBackend.generate(&[s1, s2], &settings).unwrap();
        assert_eq!(expected, bytes);
        assert_eq!(2, bytes.len());
        let out1 = String::from_utf8(bytes[0].clone()).unwrap();
        let out2 = String::from_utf8(bytes[1].clone()).unwrap();
        assert!(out1.contains("pub a: Option<String>") || out1.contains("pub a:"));
        assert!(out2.contains("pub b: Option<String>") || out2.contains("pub b:"));
    }

    #[test]
    fn batch_error_includes_index() {
        let valid = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let invalid: JsonSchema = JsonSchema::default();
        let s1: JsonSchema = serde_json::from_str(valid).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[s1, invalid], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 1, .. }));
    }
}
