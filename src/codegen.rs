use crate::error::JsonSchemaGenError;
use crate::schema::JsonSchema;
use std::collections::BTreeMap;
use std::io::Write;

/// Represents a struct to be emitted, with its fields.
struct StructDef {
    name: String,
    fields: Vec<FieldDef>,
}

/// Represents an enum to be emitted, with its variants (`rust_name`, `json_value`).
struct EnumDef {
    name: String,
    variants: Vec<(String, String)>,
}

/// Represents a field within a struct.
enum FieldDef {
    String {
        name: String,
        json_key: String,
        optional: bool,
    },
    Object {
        name: String,
        json_key: String,
        type_name: String,
        optional: bool,
    },
    Enum {
        name: String,
        json_key: String,
        type_name: String,
        optional: bool,
    },
    Boolean {
        name: String,
        json_key: String,
        optional: bool,
    },
    Array {
        name: String,
        json_key: String,
        element_type: String,
        optional: bool,
    },
    Integer {
        name: String,
        json_key: String,
        optional: bool,
    },
    Number {
        name: String,
        json_key: String,
        optional: bool,
    },
}

/// Convert a string to a valid Rust struct/type identifier (`PascalCase`).
/// Splits on any non-alphanumeric character (spaces, underscores, hyphens, etc.),
/// capitalizes each word, and joins with no separator.
/// Examples: `"The Widget_Settings Schema"` -> `"TheWidgetSettingsSchema"`, `"widget_settings"` -> `"WidgetSettings"`
fn to_rust_struct_name(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Sanitize a property key for use as a Rust field name (replace `-` with `_`).
fn sanitize_field_name(key: &str) -> String {
    key.replace('-', "_")
}

/// Generate a struct name from a property key and optional title.
fn struct_name_from_property(property_key: &str, title: Option<&str>) -> String {
    if let Some(t) = title {
        let trimmed: &str = t.trim();
        if !trimmed.is_empty() {
            return to_rust_struct_name(trimmed);
        }
    }
    to_rust_struct_name(property_key)
}

/// Convert a JSON enum value to a valid Rust enum variant identifier (`PascalCase`).
/// First char uppercase, rest lowercase per word. Prefixes with `E` (short for Enum)
/// if result is empty or starts with a digit.
fn to_rust_variant_name(s: &str) -> String {
    let base: String = s
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars: std::str::Chars<'_> = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
            }
        })
        .collect();
    if base.is_empty() || base.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("E{base}")
    } else {
        base
    }
}

/// Build enum variants from JSON Schema enum values: sort, deduplicate, handle collisions.
/// Returns `Vec<(rust_name, json_value)>`.
fn build_enum_variants(enum_values: &[String]) -> Vec<(String, String)> {
    // Deduplicate and sort for determinism
    let mut unique: Vec<String> = enum_values.to_vec();
    unique.sort();
    unique.dedup();

    // Group by base Rust name to detect collisions
    let base_names: Vec<String> = unique.iter().map(|s| to_rust_variant_name(s)).collect();
    let mut name_counts: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for base in &base_names {
        *name_counts.entry(base.clone()).or_insert(0) += 1;
    }

    let mut result: Vec<(String, String)> = Vec::with_capacity(unique.len());
    let mut name_indices: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();

    for (json_val, base_name) in unique.iter().zip(base_names.iter()) {
        let rust_name: String = if *name_counts.get(base_name).unwrap_or(&0) > 1 {
            let idx: usize = *name_indices.get(base_name).unwrap_or(&0);
            name_indices.insert(base_name.clone(), idx + 1);
            format!("{base_name}_{idx}")
        } else {
            base_name.clone()
        };
        result.push((rust_name, json_val.clone()));
    }
    result
}

/// Resolve the Rust element type for an array's `items` schema.
/// Returns `None` if items type is unsupported or missing.
fn resolve_array_item_type(property_key: &str, items_schema: &JsonSchema) -> Option<String> {
    let item_type: &str = items_schema.r#type.as_deref().unwrap_or("");

    // Check for string enum first (same order as property handling)
    if let Some(ref enum_vals) = items_schema.r#enum {
        let string_values: Option<Vec<String>> = enum_vals
            .iter()
            .map(|v| v.as_str().map(String::from))
            .collect();
        if string_values.is_some() && !enum_vals.is_empty() {
            return Some(struct_name_from_property(
                property_key,
                items_schema.title.as_deref(),
            ));
        }
    }

    match item_type {
        "string" => Some("String".to_string()),
        "boolean" => Some("bool".to_string()),
        "integer" => Some("i64".to_string()),
        "number" => Some("f64".to_string()),
        "object" => Some(struct_name_from_property(
            property_key,
            items_schema.title.as_deref(),
        )),
        _ => None,
    }
}

/// Recursively collect all structs and enums from a schema.
/// Uses `BTreeMap` for deterministic struct and field ordering (alphabetical by key).
#[expect(clippy::too_many_lines)]
fn collect_structs(
    schema: &JsonSchema,
    struct_name: &str,
    collected: &mut BTreeMap<String, StructDef>,
    collected_enums: &mut BTreeMap<String, EnumDef>,
) {
    let mut fields: Vec<FieldDef> = Vec::new();

    if let Some(ref properties) = schema.properties {
        for (key, prop_schema) in properties {
            let prop_type = prop_schema.r#type.as_deref().unwrap_or("");
            let field_rust_name: String = sanitize_field_name(key);
            let is_required: bool = schema.required.as_ref().is_some_and(|r| r.contains(key));
            let optional: bool = !is_required;

            // Check for enum before type match
            if let Some(ref enum_vals) = prop_schema.r#enum {
                let string_values: Option<Vec<String>> = enum_vals
                    .iter()
                    .map(|v| v.as_str().map(String::from))
                    .collect();
                if let Some(vals) = string_values
                    && !vals.is_empty()
                {
                    let enum_name: String =
                        struct_name_from_property(key, prop_schema.title.as_deref());
                    let variants: Vec<(String, String)> = build_enum_variants(&vals);
                    collected_enums.insert(
                        enum_name.clone(),
                        EnumDef {
                            name: enum_name.clone(),
                            variants: variants.clone(),
                        },
                    );
                    fields.push(FieldDef::Enum {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        type_name: enum_name,
                        optional,
                    });
                    continue;
                }
            }

            match prop_type {
                "string" => {
                    fields.push(FieldDef::String {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                    });
                }
                "boolean" => {
                    fields.push(FieldDef::Boolean {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                    });
                }
                "integer" => {
                    fields.push(FieldDef::Integer {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                    });
                }
                "number" => {
                    fields.push(FieldDef::Number {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                    });
                }
                "object" => {
                    let nested_name: String =
                        struct_name_from_property(key, prop_schema.title.as_deref());
                    fields.push(FieldDef::Object {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        type_name: nested_name.clone(),
                        optional,
                    });
                    if let Some(ref nested_props) = prop_schema.properties
                        && !nested_props.is_empty()
                    {
                        collect_structs(prop_schema, &nested_name, collected, collected_enums);
                    }
                }
                "array" => {
                    let Some(ref items_schema) = prop_schema.items else {
                        continue;
                    };
                    let Some(element_type) = resolve_array_item_type(key, items_schema) else {
                        continue;
                    };
                    // Collect nested struct or enum for object/enum items
                    if items_schema.r#type.as_deref() == Some("object") {
                        if let Some(ref nested_props) = items_schema.properties
                            && !nested_props.is_empty()
                        {
                            collect_structs(
                                items_schema,
                                &element_type,
                                collected,
                                collected_enums,
                            );
                        }
                    } else if let Some(ref enum_vals) = items_schema.r#enum {
                        let string_values: Option<Vec<String>> = enum_vals
                            .iter()
                            .map(|v| v.as_str().map(String::from))
                            .collect();
                        if let Some(ref vals) = string_values
                            && !vals.is_empty()
                        {
                            let variants: Vec<(String, String)> = build_enum_variants(vals);
                            collected_enums.insert(
                                element_type.clone(),
                                EnumDef {
                                    name: element_type.clone(),
                                    variants,
                                },
                            );
                        }
                    }
                    fields.push(FieldDef::Array {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        element_type,
                        optional,
                    });
                }
                _ => {
                    // Ignore other types (null, etc.) for now
                }
            }
        }
    }

    if !fields.is_empty() {
        collected.insert(
            struct_name.to_string(),
            StructDef {
                name: struct_name.to_string(),
                fields,
            },
        );
    }
}

/// Escape a string for use inside a Rust double-quoted attribute.
fn escape_for_rust_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Emit a single enum to the writer.
fn emit_enum<W: Write>(enum_def: &EnumDef, writer: &mut W) -> std::io::Result<()> {
    writeln!(
        writer,
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]"
    )?;
    writeln!(writer, "pub enum {} {{", enum_def.name)?;
    for (rust_name, json_value) in &enum_def.variants {
        let escaped: String = escape_for_rust_attr(json_value);
        writeln!(writer, "    #[serde(rename = \"{escaped}\")]")?;
        writeln!(writer, "    {rust_name},")?;
    }
    writeln!(writer, "}}")?;
    writeln!(writer)?;
    Ok(())
}

/// Emit a single struct to the writer.
#[expect(clippy::too_many_lines)]
fn emit_struct<W: Write>(struct_def: &StructDef, writer: &mut W) -> std::io::Result<()> {
    writeln!(
        writer,
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]"
    )?;
    writeln!(writer, "pub struct {} {{", struct_def.name)?;
    for field in &struct_def.fields {
        match field {
            FieldDef::String {
                name,
                json_key,
                optional,
            } => {
                let type_str: &str = if *optional {
                    "Option<String>"
                } else {
                    "String"
                };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
            FieldDef::Boolean {
                name,
                json_key,
                optional,
            } => {
                let type_str: &str = if *optional { "Option<bool>" } else { "bool" };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
            FieldDef::Integer {
                name,
                json_key,
                optional,
            } => {
                let type_str: &str = if *optional { "Option<i64>" } else { "i64" };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
            FieldDef::Number {
                name,
                json_key,
                optional,
            } => {
                let type_str: &str = if *optional { "Option<f64>" } else { "f64" };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
            FieldDef::Object {
                name,
                json_key,
                type_name,
                optional,
            }
            | FieldDef::Enum {
                name,
                json_key,
                type_name,
                optional,
            } => {
                let type_str: String = if *optional {
                    format!("Option<{type_name}>")
                } else {
                    type_name.clone()
                };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
            FieldDef::Array {
                name,
                json_key,
                element_type,
                optional,
            } => {
                let type_str: String = if *optional {
                    format!("Option<Vec<{element_type}>>")
                } else {
                    format!("Vec<{element_type}>")
                };
                if name == json_key {
                    writeln!(writer, "    pub {name}: {type_str},")?;
                } else {
                    writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
                    writeln!(writer, "    pub {name}: {type_str},")?;
                }
            }
        }
    }
    writeln!(writer, "}}")?;
    writeln!(writer)?;
    Ok(())
}

/// Determine emission order: nested structs before their parents.
fn emission_order(struct_defs: &BTreeMap<String, StructDef>, root_name: &str) -> Vec<String> {
    fn visit(
        name: &str,
        struct_defs: &BTreeMap<String, StructDef>,
        order: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
    ) {
        if visited.contains(name) {
            return;
        }
        visited.insert(name.to_string());
        if let Some(def) = struct_defs.get(name) {
            for field in &def.fields {
                match field {
                    FieldDef::Object { type_name, .. } if struct_defs.contains_key(type_name) => {
                        visit(type_name, struct_defs, order, visited);
                    }
                    FieldDef::Array { element_type, .. }
                        if struct_defs.contains_key(element_type) =>
                    {
                        visit(element_type, struct_defs, order, visited);
                    }
                    _ => {}
                }
            }
        }
        order.push(name.to_string());
    }

    let mut order: Vec<String> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();

    visit(root_name, struct_defs, &mut order, &mut visited);

    // Add any structs not reachable from root (shouldn't happen in our traversal, but be safe)
    for name in struct_defs.keys() {
        if !visited.contains(name) {
            visit(name, struct_defs, &mut order, &mut visited);
        }
    }

    order
}

/// Generate Rust structs from a JSON Schema string and write to `writer`.
pub fn generate_to_writer<W: Write>(
    schema_json: &str,
    writer: &mut W,
) -> Result<(), JsonSchemaGenError> {
    let schema: JsonSchema = serde_json::from_str(schema_json)?;

    let root_type: Option<&str> = schema.r#type.as_deref();
    if root_type != Some("object") {
        return Err(JsonSchemaGenError::GenericError(
            "Root schema must have type \"object\"".to_string(),
        ));
    }

    let root_name: String = schema
        .title
        .as_ref()
        .map(|t| to_rust_struct_name(t.trim()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Root".to_string());

    let mut collected: BTreeMap<String, StructDef> = BTreeMap::new();
    let mut collected_enums: BTreeMap<String, EnumDef> = BTreeMap::new();
    collect_structs(&schema, &root_name, &mut collected, &mut collected_enums);

    if collected.is_empty() {
        return Err(JsonSchemaGenError::GenericError(
            "No structs to generate (root object has no supported properties)".to_string(),
        ));
    }

    writeln!(
        writer,
        "//! Generated by json-schema-rs. Do not edit manually."
    )?;
    writeln!(writer)?;
    writeln!(writer, "use serde::{{Deserialize, Serialize}};")?;
    writeln!(writer)?;

    // Emit enums first (alphabetically), then structs (topological order)
    for enum_name in collected_enums.keys() {
        if let Some(enum_def) = collected_enums.get(enum_name) {
            emit_enum(enum_def, writer)?;
        }
    }
    let order: Vec<String> = emission_order(&collected, &root_name);
    for name in order {
        if let Some(struct_def) = collected.get(&name) {
            emit_struct(struct_def, writer)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_rust_struct_name_sanitizes_spaces() {
        let actual: String = to_rust_struct_name("The Widget Schema");
        let expected: &str = "TheWidgetSchema";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_struct_name_sanitizes_underscores_and_spaces() {
        let actual: String = to_rust_struct_name("The Foo_Bar Schema");
        let expected: &str = "TheFooBarSchema";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_struct_name_snake_case_property_key() {
        let actual: String = to_rust_struct_name("widget");
        let expected: &str = "Widget";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_struct_name_single_word() {
        let actual: String = to_rust_struct_name("Metadata");
        let expected: &str = "Metadata";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_struct_name_with_hyphens() {
        let actual: String = to_rust_struct_name("foo-bar-baz");
        let expected: &str = "FooBarBaz";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_variant_name_hyphenated() {
        let actual: String = to_rust_variant_name("blackjack-a");
        let expected: &str = "BlackjackA";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_variant_name_numeric_prefix_gets_e_prefix() {
        let actual: String = to_rust_variant_name("123");
        let expected: &str = "E123";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_variant_name_simple() {
        let actual: String = to_rust_variant_name("plain");
        let expected: &str = "Plain";
        assert_eq!(expected, actual);
    }

    #[test]
    fn to_rust_variant_name_uppercase() {
        let actual: String = to_rust_variant_name("PENDING");
        let expected: &str = "Pending";
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_enum_variants_json_schema_duplicate_deduplicates() {
        let input: Vec<String> = vec!["a".to_string(), "a".to_string()];
        let actual: Vec<(String, String)> = build_enum_variants(&input);
        let expected: Vec<(String, String)> = vec![("A".to_string(), "a".to_string())];
        assert_eq!(
            expected, actual,
            "duplicate JSON enum values must deduplicate to one variant"
        );
    }

    #[test]
    fn build_enum_variants_rust_output_collision_disambiguates() {
        let input: Vec<String> = vec![
            "PENDING".to_string(),
            "pending".to_string(),
            "Pending".to_string(),
        ];
        let actual: Vec<(String, String)> = build_enum_variants(&input);
        let expected: Vec<(String, String)> = vec![
            ("Pending_0".to_string(), "PENDING".to_string()),
            ("Pending_1".to_string(), "Pending".to_string()),
            ("Pending_2".to_string(), "pending".to_string()),
        ];
        assert_eq!(
            expected, actual,
            "Rust variant name collision must produce Pending_0, Pending_1, Pending_2 with correct serde mapping"
        );
    }

    #[test]
    fn generate_schema_with_spaces_in_title_produces_valid_rust() {
        let schema_json: &str = r#"{
            "title": "WidgetFile",
            "type": "object",
            "properties": {
                "widget": {
                    "title": "The Widget Schema",
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string" },
                        "name": { "type": "string" },
                        "version": { "type": "string" }
                    }
                }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TheWidgetSchema {
    pub name: Option<String>,
    pub owner: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WidgetFile {
    pub widget: Option<TheWidgetSchema>,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(expected, actual, "expected output to match exactly");
    }

    #[test]
    fn generate_schema_required_fields_emit_non_option() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "RequiredOnly",
            "required": ["x"],
            "properties": {
                "x": { "type": "string" }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RequiredOnly {
    pub x: String,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(expected, actual, "expected output to match exactly");
    }

    #[test]
    fn generate_schema_optional_fields_emit_option() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "OptionalOnly",
            "properties": {
                "x": { "type": "string" }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OptionalOnly {
    pub x: Option<String>,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(expected, actual, "expected output to match exactly");
    }

    #[test]
    fn generate_schema_mixed_required_optional() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "Mixed",
            "required": ["req"],
            "properties": {
                "opt": { "type": "string" },
                "req": { "type": "string" }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Mixed {
    pub opt: Option<String>,
    pub req: String,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(expected, actual, "expected output to match exactly");
    }

    #[test]
    fn generate_schema_empty_required_all_optional() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "EmptyRequired",
            "required": [],
            "properties": {
                "a": { "type": "string" },
                "b": { "type": "string" }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmptyRequired {
    pub a: Option<String>,
    pub b: Option<String>,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(expected, actual, "expected output to match exactly");
    }
}
