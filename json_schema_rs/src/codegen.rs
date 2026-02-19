//! Code generation: schema → Rust source written to a writer.

use crate::error::Error;
use crate::schema::Schema;
use std::collections::BTreeSet;
use std::io::Write;

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
    schema: Schema,
}

/// Collect all object schemas that need a struct in topological order (children before parents).
/// Uses an explicit stack to avoid recursion and stack overflow on deep schemas.
fn collect_structs(
    schema: &Schema,
    from_key: Option<&str>,
    out: &mut Vec<StructToEmit>,
    seen: &mut BTreeSet<String>,
) {
    if !schema.is_object_with_properties() {
        return;
    }

    // Phase 1: iterative post-order DFS to collect (schema, from_key) so children come before parents.
    let mut post_order: Vec<(Schema, Option<String>)> = Vec::new();
    let mut stack: Vec<(Schema, Option<String>, usize)> = Vec::new();
    stack.push((schema.clone(), from_key.map(String::from), 0));

    while let Some((schema_node, from_key_opt, index)) = stack.pop() {
        let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
        if index < keys.len() {
            let key: String = keys.get(index).unwrap().clone();
            let child: Schema = schema_node.properties.get(&key).unwrap().clone();
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
fn emit_struct_fields(schema: &Schema, out: &mut impl Write) -> Result<(), Error> {
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

/// Generate Rust source from a parsed schema and write it to `out`.
///
/// Root schema must have `type: "object"` and non-empty `properties`.
///
/// # Errors
///
/// Returns [`Error::RootNotObject`] if the root schema is not an object with properties.
/// Returns [`Error::Io`] on write failure.
pub fn generate_rust(schema: &Schema, out: &mut impl Write) -> Result<(), Error> {
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

#[cfg(test)]
mod tests {
    use super::{generate_rust, sanitize_field_name, to_pascal_case};
    use crate::schema::Schema;
    use std::io::Cursor;

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
        let schema = Schema::default();
        let mut out = Cursor::new(Vec::new());
        let actual = generate_rust(&schema, &mut out).unwrap_err();
        assert!(matches!(actual, crate::error::Error::RootNotObject));
    }

    #[test]
    fn root_object_empty_properties_errors() {
        let schema = Schema {
            type_: Some("object".to_string()),
            ..Default::default()
        };
        let mut out = Cursor::new(Vec::new());
        let actual = generate_rust(&schema, &mut out).unwrap_err();
        assert!(matches!(actual, crate::error::Error::RootNotObject));
    }

    #[test]
    fn single_string_property() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let mut out = Cursor::new(Vec::new());
        generate_rust(&schema, &mut out).unwrap();
        let actual = String::from_utf8(out.into_inner()).unwrap();
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
        let schema: Schema = serde_json::from_str(json).unwrap();
        let mut out = Cursor::new(Vec::new());
        generate_rust(&schema, &mut out).unwrap();
        let actual = String::from_utf8(out.into_inner()).unwrap();
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
        let schema: Schema = serde_json::from_str(json).unwrap();
        let mut out = Cursor::new(Vec::new());
        generate_rust(&schema, &mut out).unwrap();
        let actual = String::from_utf8(out.into_inner()).unwrap();
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
        let schema: Schema = serde_json::from_str(json).unwrap();
        let mut out = Cursor::new(Vec::new());
        generate_rust(&schema, &mut out).unwrap();
        let actual = String::from_utf8(out.into_inner()).unwrap();
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
        let mut inner: Schema = Schema {
            type_: Some("object".to_string()),
            properties: {
                let mut m = std::collections::BTreeMap::new();
                m.insert(
                    "value".to_string(),
                    Schema {
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
            let mut wrap: Schema = Schema {
                type_: Some("object".to_string()),
                properties: std::collections::BTreeMap::new(),
                required: None,
                title: Some(format!("Level{i}")),
            };
            wrap.properties.insert("child".to_string(), inner);
            inner = wrap;
        }
        let mut out = Cursor::new(Vec::new());
        let actual = generate_rust(&inner, &mut out);
        assert!(actual.is_ok(), "deep schema must not overflow");
        let output: String = String::from_utf8(out.into_inner()).unwrap();
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
        let schema: Schema = serde_json::from_str(json).unwrap();
        let mut out = Cursor::new(Vec::new());
        generate_rust(&schema, &mut out).unwrap();
        let actual = String::from_utf8(out.into_inner()).unwrap();
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
}
