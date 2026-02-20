//! Code generation: schema → source in a target language.
//!
//! A [`CodegenBackend`] takes the intermediate [`JsonSchema`] and returns generated
//! source as bytes. The CLI matches on the language argument and calls the
//! appropriate backend (e.g. [`RustBackend::generate`]).

use crate::error::Error;
use crate::json_schema::JsonSchema;
use crate::sanitize::{sanitize_field_name, sanitize_struct_name};
use std::collections::BTreeSet;
use std::io::{Cursor, Write};

/// How to choose the generated struct/type name when both `title` and property key are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelNameSource {
    /// Use `title` first, then property key, then `"Root"` for the root schema. (Current behavior.)
    #[default]
    TitleFirst,
    /// Use property key first, then `title`, then `"Root"` for the root schema.
    PropertyKeyFirst,
}

/// Options for Rust code generation.
#[derive(Debug, Clone)]
pub struct RustCodegenOptions {
    /// Which source to prefer for struct/type names: title or property key.
    pub model_name_source: ModelNameSource,
}

impl Default for RustCodegenOptions {
    fn default() -> Self {
        Self {
            model_name_source: ModelNameSource::TitleFirst,
        }
    }
}

impl RustCodegenOptions {
    /// Prefer property key over title for struct names (fallback to title, then `"Root"`).
    #[must_use]
    pub fn with_property_key_first(mut self) -> Self {
        self.model_name_source = ModelNameSource::PropertyKeyFirst;
        self
    }
}

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
#[derive(Debug, Clone, Default)]
pub struct RustBackend {
    /// Codegen options (model name source, etc.).
    pub options: RustCodegenOptions,
}

impl RustBackend {
    /// Create a backend with the given options.
    #[must_use]
    pub fn new(options: RustCodegenOptions) -> Self {
        Self { options }
    }
}

impl CodegenBackend for RustBackend {
    fn generate(&self, schemas: &[JsonSchema]) -> Result<Vec<Vec<u8>>, Error> {
        let mut results: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            let mut out = Cursor::new(Vec::new());
            emit_rust(schema, &mut out, &self.options).map_err(|e| Error::Batch {
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

/// Compute struct/type name from title, property key, and root fallback per options.
fn struct_name_from(
    title: Option<&str>,
    from_key: Option<&str>,
    is_root: bool,
    options: &RustCodegenOptions,
) -> String {
    let title_trimmed: Option<&str> = title.filter(|t| !t.trim().is_empty()).map(str::trim);
    let from_key_s: Option<&str> = from_key;

    let (first, second) = match options.model_name_source {
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
    options: &RustCodegenOptions,
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
            options,
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
    options: &RustCodegenOptions,
) -> Result<(), Error> {
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
                struct_name_from(prop_schema.title.as_deref(), Some(key), false, options);
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
    options: &RustCodegenOptions,
) -> Result<(), Error> {
    if !schema.is_object_with_properties() {
        return Err(Error::RootNotObject);
    }

    let mut structs: Vec<StructToEmit> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    collect_structs(schema, None, &mut structs, &mut seen, options);

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
        emit_struct_fields(&st.schema, out, options)?;
        writeln!(out, "}}")?;
        writeln!(out)?;
    }

    Ok(())
}

/// Generate Rust source from one or more parsed schemas (default options: title first for struct names).
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
    RustBackend::default().generate(schemas)
}

/// Generate Rust source with the given options (e.g. property key first for struct names).
///
/// Same as [`generate_rust`] but uses the provided [`RustCodegenOptions`].
///
/// # Errors
///
/// Returns [`Error::RootNotObject`] if a root schema is not an object with properties.
/// Returns [`Error::Io`] on write failure.
/// Returns [`Error::Batch`] with index when one schema in the batch fails.
pub fn generate_rust_with_options(
    schemas: &[JsonSchema],
    options: &RustCodegenOptions,
) -> Result<Vec<Vec<u8>>, Error> {
    RustBackend::new(options.clone()).generate(schemas)
}

#[cfg(test)]
mod tests {
    use super::{
        CodegenBackend, RustBackend, RustCodegenOptions, generate_rust, generate_rust_with_options,
    };
    use crate::error::Error;
    use crate::json_schema::JsonSchema;

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
        let expected: Vec<Vec<u8>> = RustBackend::default().generate(&[schema]).unwrap();
        assert_eq!(expected, bytes);
        assert_eq!(1, bytes.len());
    }

    #[test]
    fn property_key_first_uses_key_over_title_for_nested_struct() {
        let json = r#"{"type":"object","properties":{"address":{"type":"object","title":"FooBar","properties":{"city":{"type":"string"}}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let options: RustCodegenOptions = RustCodegenOptions::default().with_property_key_first();
        let bytes = generate_rust_with_options(&[schema], &options).unwrap();
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
        let bytes = generate_rust(&[s1.clone(), s2.clone()]).unwrap();
        let expected = RustBackend::default().generate(&[s1, s2]).unwrap();
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
