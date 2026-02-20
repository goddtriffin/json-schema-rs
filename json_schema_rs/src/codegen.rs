//! Code generation: schema → source in a target language.
//!
//! A [`CodegenBackend`] takes the intermediate [`JsonSchema`] and returns generated
//! source as bytes. The CLI matches on the language argument and calls the
//! appropriate backend (e.g. [`RustBackend::generate`]).

use crate::error::Error;
use crate::json_schema::JsonSchema;
use std::collections::BTreeSet;
use std::io::{Cursor, Write};

/// Contract for a codegen backend: schemas in, one generated source buffer per schema out.
pub trait CodegenBackend {
    /// Generate model source for each schema. Returns one UTF-8 encoded byte buffer per schema.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RootNotObject`] if a root schema is not an object with properties.
    /// Returns [`Error::Io`] on write failure.
    /// Returns [`Error::Batch`] with index when one schema in the batch fails.
    fn generate(&self, schemas: &[JsonSchema]) -> Result<Vec<Vec<u8>>, Error>;
}

/// Backend that emits Rust structs (serde-compatible).
#[derive(Debug, Clone, Copy, Default)]
pub struct RustBackend;

impl CodegenBackend for RustBackend {
    fn generate(&self, schemas: &[JsonSchema]) -> Result<Vec<Vec<u8>>, Error> {
        let mut results: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            let mut out = Cursor::new(Vec::new());
            emit_rust(schema, &mut out).map_err(|e| Error::Batch {
                index,
                source: Box::new(e),
            })?;
            results.push(out.into_inner());
        }
        Ok(results)
    }
}

/// Sanitize a JSON property key to a Rust field identifier (`snake_case`; replace `-` with `_`).
/// Does not change case; only replaces invalid characters. Result is safe for use as a field name.
fn sanitize_field_name(key: &str) -> String {
    let s: String = key
        .chars()
        .map(|c| if c == '-' { '_' } else { c })
        .collect();
    if s.is_empty() {
        return "empty".to_string();
    }
    if s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return format!("field_{s}");
    }
    if s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()) {
        return s;
    }
    s.chars()
        .map(|c| {
            if c == '_' || c.is_ascii_alphanumeric() {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert a name to `PascalCase` for struct names (e.g. "address" -> "Address").
fn to_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut capitalize_next = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == ' ' {
            capitalize_next = true;
        } else if capitalize_next {
            out.extend(c.to_uppercase());
            capitalize_next = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "Unnamed".to_string()
    } else if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{out}")
    } else {
        out
    }
}

/// Ensure a struct name is a valid Rust identifier (`PascalCase`; prefix if starts with digit).
fn sanitize_struct_name(s: &str) -> String {
    let pascal = to_pascal_case(s);
    if pascal.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{pascal}")
    } else {
        pascal
    }
}

/// One struct to emit: name and the object schema (root or nested).
struct StructToEmit {
    name: String,
    schema: JsonSchema,
}

/// Collect all object schemas that need a struct in topological order (children before parents).
/// Uses an explicit stack to avoid recursion and stack overflow on deep schemas.
fn collect_structs(
    schema: &JsonSchema,
    from_key: Option<&str>,
    out: &mut Vec<StructToEmit>,
    seen: &mut BTreeSet<String>,
) {
    if !schema.is_object_with_properties() {
        return;
    }

    // Phase 1: iterative post-order DFS to collect (schema, from_key) so children come before parents.
    let mut post_order: Vec<(JsonSchema, Option<String>)> = Vec::new();
    let mut stack: Vec<(JsonSchema, Option<String>, usize)> = Vec::new();
    stack.push((schema.clone(), from_key.map(String::from), 0));

    while let Some((schema_node, from_key_opt, index)) = stack.pop() {
        let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
        if index < keys.len() {
            let key: String = keys.get(index).unwrap().clone();
            let child: JsonSchema = schema_node.properties.get(&key).unwrap().clone();
            stack.push((schema_node, from_key_opt, index + 1));
            if child.is_object_with_properties() {
                stack.push((child, Some(key), 0));
            }
        } else {
            post_order.push((schema_node, from_key_opt));
        }
    }

    // Phase 2: emit in post-order, dedupe by name (first occurrence wins).
    for (schema_node, from_key_opt) in post_order {
        let name: String = schema_node
            .title
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .map(sanitize_struct_name)
            .or_else(|| from_key_opt.as_deref().map(sanitize_struct_name))
            .unwrap_or_else(|| "Root".to_string());

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
fn emit_struct_fields(schema: &JsonSchema, out: &mut impl Write) -> Result<(), Error> {
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
            let nested_name = prop_schema
                .title
                .as_deref()
                .filter(|t| !t.trim().is_empty())
                .map_or_else(|| to_pascal_case(key), sanitize_struct_name);
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
fn emit_rust(schema: &JsonSchema, out: &mut impl Write) -> Result<(), Error> {
    if !schema.is_object_with_properties() {
        return Err(Error::RootNotObject);
    }

    let mut structs: Vec<StructToEmit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    collect_structs(schema, None, &mut structs, &mut seen);

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
        emit_struct_fields(&st.schema, out)?;
        writeln!(out, "}}")?;
        writeln!(out)?;
    }

    Ok(())
}

/// Generate Rust source from one or more parsed schemas.
///
/// Returns one byte buffer per schema; each buffer is UTF-8 Rust source. Root of each schema
/// must have `type: "object"` and non-empty `properties`.
///
/// # Errors
///
/// Returns [`Error::RootNotObject`] if a root schema is not an object with properties.
/// Returns [`Error::Io`] on write failure.
/// Returns [`Error::Batch`] with index when one schema in the batch fails.
pub fn generate_rust(schemas: &[JsonSchema]) -> Result<Vec<Vec<u8>>, Error> {
    RustBackend.generate(schemas)
}

#[cfg(test)]
mod tests {
    use super::{CodegenBackend, RustBackend, generate_rust, sanitize_field_name, to_pascal_case};
    use crate::error::Error;
    use crate::json_schema::JsonSchema;

    #[test]
    fn sanitize_field_name_replaces_hyphen() {
        let expected = "foo_bar";
        let actual = sanitize_field_name("foo-bar");
        assert_eq!(expected, actual);
    }

    #[test]
    fn sanitize_field_name_unchanged_valid() {
        let expected = "first_name";
        let actual = sanitize_field_name("first_name");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_address() {
        let expected = "Address";
        let actual = to_pascal_case("address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_pascal_case_street_address() {
        let expected = "StreetAddress";
        let actual = to_pascal_case("street_address");
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_not_object_errors() {
        let schema: JsonSchema = JsonSchema::default();
        let actual = generate_rust(&[schema]).unwrap_err();
        assert!(matches!(actual, Error::Batch { index: 0, .. }));
        if let Error::Batch { source, .. } = actual {
            assert!(matches!(*source, Error::RootNotObject));
        }
    }

    #[test]
    fn root_object_empty_properties_errors() {
        let schema: JsonSchema = JsonSchema {
            type_: Some("object".to_string()),
            ..Default::default()
        };
        let actual = generate_rust(&[schema]).unwrap_err();
        assert!(matches!(actual, Error::Batch { index: 0, .. }));
        if let Error::Batch { source, .. } = actual {
            assert!(matches!(*source, Error::RootNotObject));
        }
    }

    #[test]
    fn single_string_property() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let bytes = generate_rust(&[schema]).unwrap();
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
        let bytes = generate_rust(&[schema]).unwrap();
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
        let bytes = generate_rust(&[schema]).unwrap();
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
        let bytes = generate_rust(&[schema]).unwrap();
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
        let actual = generate_rust(&[inner]);
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
        let bytes = generate_rust(&[schema]).unwrap();
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
        let bytes = generate_rust(std::slice::from_ref(&schema)).unwrap();
        let expected: Vec<Vec<u8>> = RustBackend.generate(&[schema]).unwrap();
        assert_eq!(expected, bytes);
        assert_eq!(1, bytes.len());
    }

    #[test]
    fn generate_rust_two_schemas_returns_two_buffers() {
        let json1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let json2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json1).unwrap();
        let s2: JsonSchema = serde_json::from_str(json2).unwrap();
        let bytes = generate_rust(&[s1.clone(), s2.clone()]).unwrap();
        let expected = RustBackend.generate(&[s1, s2]).unwrap();
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
        let actual = generate_rust(&[s1, invalid]).unwrap_err();
        assert!(matches!(actual, Error::Batch { index: 1, .. }));
    }
}
