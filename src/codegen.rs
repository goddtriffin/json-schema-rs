use crate::error::JsonSchemaGenError;
use crate::schema::JsonSchema;
use std::collections::BTreeMap;
use std::io::Write;

/// Represents a struct to be emitted, with its fields.
struct StructDef {
    name: String,
    fields: Vec<FieldDef>,
    deny_unknown_fields: bool,
    description: Option<String>,
}

/// Represents an enum to be emitted, with its variants (`rust_name`, `json_value`).
struct EnumDef {
    name: String,
    variants: Vec<(String, String)>,
    description: Option<String>,
}

/// How a field gets its default value when the JSON key is missing.
enum DefaultSpec {
    /// Use #[serde(default)] - type's Default trait
    UseTypeDefault,
    /// Use #[serde(default = "`fn_name`")] - custom literal
    Custom { fn_name: String, rust_expr: String },
}

/// Represents a field within a struct.
enum FieldDef {
    String {
        name: String,
        json_key: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    Object {
        name: String,
        json_key: String,
        type_name: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    Enum {
        name: String,
        json_key: String,
        type_name: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    Boolean {
        name: String,
        json_key: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    Array {
        name: String,
        json_key: String,
        element_type: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    Integer {
        name: String,
        json_key: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
        integer_type: String,
    },
    Number {
        name: String,
        json_key: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
        number_type: String,
    },
    Uuid {
        name: String,
        json_key: String,
        optional: bool,
        default: Option<DefaultSpec>,
        description: Option<String>,
    },
    AdditionalProperties {
        name: String,
        value_type: String,
    },
}

/// Normalize description: trim and treat empty/whitespace as None.
fn normalize_description(s: Option<&String>) -> Option<String> {
    s.as_ref().and_then(|t| {
        let trimmed: &str = t.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
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

/// Returns true if the format indicates a UUID (uuid, uuid1..uuid8, case-insensitive).
fn is_uuid_format(format: Option<&str>) -> bool {
    let Some(f) = format else {
        return false;
    };
    let lower: &str = &f.to_lowercase();
    matches!(
        lower,
        "uuid" | "uuid1" | "uuid2" | "uuid3" | "uuid4" | "uuid5" | "uuid6" | "uuid7" | "uuid8"
    )
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

/// Choose the smallest Rust integer type that fits the schema's minimum/maximum range.
/// Returns a type name string: "i8", "u8", "i16", "u16", "i32", "u32", "i64", or "u64".
/// Falls back to "i64" when min/max are absent, not integers, or invalid.
pub(crate) fn choose_integer_type(schema: &JsonSchema) -> String {
    let min_val: Option<i64> = schema.minimum.as_ref().and_then(serde_json::Value::as_i64);
    let max_val: Option<i64> = schema.maximum.as_ref().and_then(serde_json::Value::as_i64);
    let (min_val, max_val): (i64, i64) = match (min_val, max_val) {
        (Some(min_v), Some(max_v)) => (min_v, max_v),
        _ => return "i64".to_string(),
    };
    if min_val > max_val {
        return "i64".to_string();
    }
    if min_val >= 0 {
        if max_val <= 255 {
            "u8".to_string()
        } else if max_val <= 65_535 {
            "u16".to_string()
        } else if max_val <= 4_294_967_295 {
            "u32".to_string()
        } else {
            "u64".to_string()
        }
    } else if min_val >= -128 && max_val <= 127 {
        "i8".to_string()
    } else if min_val >= -32_768 && max_val <= 32_767 {
        "i16".to_string()
    } else if min_val >= -2_147_483_648 && max_val <= 2_147_483_647 {
        "i32".to_string()
    } else {
        "i64".to_string()
    }
}

/// Choose f32 or f64 based on whether minimum and maximum fit in f32 range.
/// Returns "f32" only when both bounds are present and within f32 range; otherwise "f64".
pub(crate) fn choose_number_type(schema: &JsonSchema) -> String {
    const F32_MIN: f64 = -3.402_823_5e38;
    const F32_MAX: f64 = 3.402_823_5e38;
    #[expect(clippy::cast_precision_loss)]
    let min_opt: Option<f64> = schema
        .minimum
        .as_ref()
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
    #[expect(clippy::cast_precision_loss)]
    let max_opt: Option<f64> = schema
        .maximum
        .as_ref()
        .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)));
    let (min_val, max_val): (f64, f64) = match (min_opt, max_opt) {
        (Some(min_v), Some(max_v)) => (min_v, max_v),
        _ => return "f64".to_string(),
    };
    if min_val >= F32_MIN && max_val <= F32_MAX {
        "f32".to_string()
    } else {
        "f64".to_string()
    }
}

/// Resolve `DefaultSpec` from a property's default value.
/// Returns None if no default, or if the default is unsupported (object, non-empty array, null for required, etc.).
fn resolve_default_spec(
    default_value: Option<&serde_json::Value>,
    type_kind: DefaultTypeKind,
    struct_name: &str,
    field_name: &str,
    optional: bool,
    enum_variants: Option<&[(String, String)]>,
) -> Option<DefaultSpec> {
    let dv: &serde_json::Value = default_value?;

    // For optional fields, null means use None (i.e. #[serde(default)])
    if dv.is_null() && optional {
        return Some(DefaultSpec::UseTypeDefault);
    }
    if dv.is_null() {
        return None;
    }

    let fn_name: String = format!("default_{struct_name}_{field_name}");

    match type_kind {
        DefaultTypeKind::Bool => {
            let b: bool = dv.as_bool()?;
            if b {
                Some(DefaultSpec::Custom {
                    fn_name,
                    rust_expr: "true".to_string(),
                })
            } else {
                Some(DefaultSpec::UseTypeDefault)
            }
        }
        DefaultTypeKind::Integer { type_name } => {
            let n: i64 = dv.as_i64()?;
            if n == 0 {
                Some(DefaultSpec::UseTypeDefault)
            } else {
                Some(DefaultSpec::Custom {
                    fn_name,
                    rust_expr: format!("{n}{type_name}"),
                })
            }
        }
        DefaultTypeKind::Number { type_name } => {
            #[expect(clippy::cast_precision_loss)]
            let n: f64 = dv.as_f64().or_else(|| dv.as_i64().map(|i| i as f64))?;
            if n == 0.0 {
                Some(DefaultSpec::UseTypeDefault)
            } else {
                Some(DefaultSpec::Custom {
                    fn_name,
                    rust_expr: format!("{n}{type_name}"),
                })
            }
        }
        DefaultTypeKind::String => {
            let s: &str = dv.as_str()?;
            if s.is_empty() {
                Some(DefaultSpec::UseTypeDefault)
            } else {
                let escaped: String = s.replace('\\', "\\\\").replace('"', "\\\"");
                Some(DefaultSpec::Custom {
                    fn_name,
                    rust_expr: format!("\"{escaped}\".to_string()"),
                })
            }
        }
        DefaultTypeKind::Vec => {
            if dv.is_array() && dv.as_array().is_some_and(std::vec::Vec::is_empty) {
                Some(DefaultSpec::UseTypeDefault)
            } else {
                // Non-empty array default not supported
                None
            }
        }
        DefaultTypeKind::Enum { type_name } => {
            let variants: &[(String, String)] = enum_variants?;
            let json_str: &str = dv.as_str()?;
            let (rust_name, _): &(String, String) =
                variants.iter().find(|(_, json_val)| json_val == json_str)?;
            Some(DefaultSpec::Custom {
                fn_name,
                rust_expr: format!("{type_name}::{rust_name}"),
            })
        }
        DefaultTypeKind::Uuid => {
            let s: &str = dv.as_str()?;
            let escaped: String = s.replace('\\', "\\\\").replace('"', "\\\"");
            Some(DefaultSpec::Custom {
                fn_name,
                rust_expr: format!(
                    "Uuid::parse_str(\"{escaped}\").expect(\"invalid default uuid\")"
                ),
            })
        }
        DefaultTypeKind::Object => {
            // Object default not supported
            None
        }
    }
}

/// Type kind for default resolution.
enum DefaultTypeKind {
    Bool,
    Integer { type_name: String },
    Number { type_name: String },
    String,
    Uuid,
    Vec,
    Enum { type_name: String },
    Object,
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
        "string" => {
            if is_uuid_format(items_schema.format.as_deref()) {
                Some("Uuid".to_string())
            } else {
                Some("String".to_string())
            }
        }
        "boolean" => Some("bool".to_string()),
        "integer" => Some(choose_integer_type(items_schema)),
        "number" => Some(choose_number_type(items_schema)),
        "object" => Some(struct_name_from_property(
            property_key,
            items_schema.title.as_deref(),
        )),
        _ => None,
    }
}

/// Resolve the Rust value type for `additionalProperties` when it is a schema object.
/// Returns `None` if the value is not a schema object or is invalid.
/// For nested objects with properties, collects the struct into `collected`.
fn resolve_additional_properties_value_type(
    schema_value: &serde_json::Value,
    struct_name: &str,
    collected: &mut BTreeMap<String, StructDef>,
    collected_enums: &mut BTreeMap<String, EnumDef>,
) -> Option<String> {
    let ap_schema: JsonSchema = serde_json::from_value(schema_value.clone()).ok()?;
    let ap_type: &str = ap_schema.r#type.as_deref().unwrap_or("");

    if let Some(ref enum_vals) = ap_schema.r#enum {
        let string_values: Option<Vec<String>> = enum_vals
            .iter()
            .map(|v| v.as_str().map(String::from))
            .collect();
        if string_values.is_some() && !enum_vals.is_empty() {
            let enum_name: String = format!("{struct_name}Value");
            let vals: Vec<String> = enum_vals
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            let variants: Vec<(String, String)> = build_enum_variants(&vals);
            collected_enums.insert(
                enum_name.clone(),
                EnumDef {
                    name: enum_name.clone(),
                    variants,
                    description: normalize_description(ap_schema.description.as_ref()),
                },
            );
            return Some(enum_name);
        }
    }

    match ap_type {
        "string" => {
            if is_uuid_format(ap_schema.format.as_deref()) {
                Some("Uuid".to_string())
            } else {
                Some("String".to_string())
            }
        }
        "boolean" => Some("bool".to_string()),
        "integer" => Some(choose_integer_type(&ap_schema)),
        "number" => Some(choose_number_type(&ap_schema)),
        "object" => {
            let nested_name: String = format!("{struct_name}Extra");
            if let Some(ref props) = ap_schema.properties
                && !props.is_empty()
            {
                collect_structs(&ap_schema, &nested_name, collected, collected_enums);
                return Some(nested_name);
            }
            Some("serde_json::Value".to_string())
        }
        _ => Some("serde_json::Value".to_string()),
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
    let deny_unknown_fields: bool = schema
        .additional_properties
        .as_ref()
        .and_then(serde_json::Value::as_bool)
        .is_some_and(|b| !b);

    if let Some(ref ap_value) = schema.additional_properties
        && ap_value.is_object()
        && let Some(value_type) = resolve_additional_properties_value_type(
            ap_value,
            struct_name,
            collected,
            collected_enums,
        )
    {
        fields.push(FieldDef::AdditionalProperties {
            name: "additional_properties".to_string(),
            value_type,
        });
    }

    if let Some(ref properties) = schema.properties {
        for (key, prop_schema) in properties {
            let prop_type = prop_schema.r#type.as_deref().unwrap_or("");
            let field_rust_name: String = sanitize_field_name(key);
            // Required/optional from object-level `required` only; per-property `optional` is recognized but explicitly ignored (see schema.rs).
            let is_required: bool = schema.required.as_ref().is_some_and(|r| r.contains(key));
            let optional: bool = !is_required;
            let prop_description: Option<String> =
                normalize_description(prop_schema.description.as_ref());

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
                            description: normalize_description(prop_schema.description.as_ref()),
                        },
                    );
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Enum {
                            type_name: enum_name.clone(),
                        },
                        struct_name,
                        &field_rust_name,
                        optional,
                        Some(&variants),
                    );
                    fields.push(FieldDef::Enum {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        type_name: enum_name,
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
                    });
                    continue;
                }
            }

            match prop_type {
                "string" => {
                    let is_uuid: bool = is_uuid_format(prop_schema.format.as_deref());
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        if is_uuid {
                            DefaultTypeKind::Uuid
                        } else {
                            DefaultTypeKind::String
                        },
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    if is_uuid {
                        fields.push(FieldDef::Uuid {
                            name: field_rust_name.clone(),
                            json_key: key.clone(),
                            optional,
                            default: default_spec,
                            description: prop_description.clone(),
                        });
                    } else {
                        fields.push(FieldDef::String {
                            name: field_rust_name.clone(),
                            json_key: key.clone(),
                            optional,
                            default: default_spec,
                            description: prop_description.clone(),
                        });
                    }
                }
                "boolean" => {
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Bool,
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    fields.push(FieldDef::Boolean {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
                    });
                }
                "integer" => {
                    let integer_type: String = choose_integer_type(prop_schema);
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Integer {
                            type_name: integer_type.clone(),
                        },
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    fields.push(FieldDef::Integer {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
                        integer_type,
                    });
                }
                "number" => {
                    let number_type: String = choose_number_type(prop_schema);
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Number {
                            type_name: number_type.clone(),
                        },
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    fields.push(FieldDef::Number {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
                        number_type,
                    });
                }
                "object" => {
                    let nested_name: String =
                        struct_name_from_property(key, prop_schema.title.as_deref());
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Object,
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    fields.push(FieldDef::Object {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        type_name: nested_name.clone(),
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
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
                                    description: normalize_description(
                                        items_schema.description.as_ref(),
                                    ),
                                },
                            );
                        }
                    }
                    let default_spec: Option<DefaultSpec> = resolve_default_spec(
                        match &prop_schema.default {
                            crate::schema::DefaultKeyword::Present(v) => Some(v),
                            crate::schema::DefaultKeyword::Absent => None,
                        },
                        DefaultTypeKind::Vec,
                        struct_name,
                        &field_rust_name,
                        optional,
                        None,
                    );
                    fields.push(FieldDef::Array {
                        name: field_rust_name.clone(),
                        json_key: key.clone(),
                        element_type,
                        optional,
                        default: default_spec,
                        description: prop_description.clone(),
                    });
                }
                _ => {
                    // Ignore other types (null, etc.) for now
                }
            }
        }
    }

    if !fields.is_empty() || deny_unknown_fields {
        collected.insert(
            struct_name.to_string(),
            StructDef {
                name: struct_name.to_string(),
                fields,
                deny_unknown_fields,
                description: normalize_description(schema.description.as_ref()),
            },
        );
    }
}

/// Escape a string for use inside a Rust double-quoted attribute.
fn escape_for_rust_attr(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Emit a doc comment from a description: each line becomes a `///` line.
/// `line_prefix` is prepended to each line (e.g. `""` for struct/enum, `"    "` for fields).
fn emit_doc_comment<W: Write>(
    writer: &mut W,
    description: Option<&str>,
    line_prefix: &str,
) -> std::io::Result<()> {
    let Some(desc) = description else {
        return Ok(());
    };
    let trimmed: &str = desc.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    for line in trimmed.lines() {
        writeln!(writer, "{line_prefix}/// {line}")?;
    }
    Ok(())
}

/// Emit a single enum to the writer.
fn emit_enum<W: Write>(enum_def: &EnumDef, writer: &mut W) -> std::io::Result<()> {
    emit_doc_comment(writer, enum_def.description.as_deref(), "")?;
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

/// Emit default functions for all Custom default specs in the given structs.
/// Order follows the provided struct names (topological).
#[expect(clippy::too_many_lines)]
fn emit_default_functions<W: Write>(
    struct_defs: &BTreeMap<String, StructDef>,
    order: &[String],
    writer: &mut W,
) -> std::io::Result<()> {
    for struct_name in order {
        let Some(struct_def) = struct_defs.get(struct_name) else {
            continue;
        };
        for field in &struct_def.fields {
            let (default, optional, return_type): (Option<&DefaultSpec>, bool, String) = match field
            {
                FieldDef::String {
                    default, optional, ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        "Option<String>".to_string()
                    } else {
                        "String".to_string()
                    },
                ),
                FieldDef::Uuid {
                    default, optional, ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        "Option<Uuid>".to_string()
                    } else {
                        "Uuid".to_string()
                    },
                ),
                FieldDef::Boolean {
                    default, optional, ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        "Option<bool>".to_string()
                    } else {
                        "bool".to_string()
                    },
                ),
                FieldDef::Integer {
                    default,
                    optional,
                    integer_type,
                    ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        format!("Option<{integer_type}>")
                    } else {
                        integer_type.clone()
                    },
                ),
                FieldDef::Number {
                    default,
                    optional,
                    number_type,
                    ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        format!("Option<{number_type}>")
                    } else {
                        number_type.clone()
                    },
                ),
                FieldDef::Object {
                    default,
                    optional,
                    type_name,
                    ..
                }
                | FieldDef::Enum {
                    default,
                    optional,
                    type_name,
                    ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        format!("Option<{type_name}>")
                    } else {
                        type_name.clone()
                    },
                ),
                FieldDef::Array {
                    default,
                    optional,
                    element_type,
                    ..
                } => (
                    default.as_ref(),
                    *optional,
                    if *optional {
                        format!("Option<Vec<{element_type}>>")
                    } else {
                        format!("Vec<{element_type}>")
                    },
                ),
                FieldDef::AdditionalProperties { .. } => continue,
            };

            if let Some(DefaultSpec::Custom { fn_name, rust_expr }) = default {
                writeln!(writer, "fn {fn_name}() -> {return_type} {{")?;
                if optional {
                    writeln!(writer, "    Some({rust_expr})")?;
                } else {
                    writeln!(writer, "    {rust_expr}")?;
                }
                writeln!(writer, "}}")?;
                writeln!(writer)?;
            }
        }
    }
    Ok(())
}

/// Emit default attribute(s) before a field: doc comment (if any), rename (if needed), then default (if any).
fn emit_field_attrs<W: Write>(
    writer: &mut W,
    name: &str,
    json_key: &str,
    default: Option<&DefaultSpec>,
    description: Option<&str>,
) -> std::io::Result<()> {
    emit_doc_comment(writer, description, "    ")?;
    if name != json_key {
        writeln!(writer, "    #[serde(rename = \"{json_key}\")]")?;
    }
    if let Some(spec) = default {
        match spec {
            DefaultSpec::UseTypeDefault => writeln!(writer, "    #[serde(default)]")?,
            DefaultSpec::Custom { fn_name, .. } => {
                writeln!(writer, "    #[serde(default = \"{fn_name}\")]")?;
            }
        }
    }
    Ok(())
}

/// Emit a single struct to the writer.
#[expect(clippy::too_many_lines)]
fn emit_struct<W: Write>(struct_def: &StructDef, writer: &mut W) -> std::io::Result<()> {
    emit_doc_comment(writer, struct_def.description.as_deref(), "")?;
    writeln!(
        writer,
        "#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]"
    )?;
    if struct_def.deny_unknown_fields {
        writeln!(writer, "#[serde(deny_unknown_fields)]")?;
    }
    writeln!(writer, "pub struct {} {{", struct_def.name)?;
    for field in &struct_def.fields {
        match field {
            FieldDef::String {
                name,
                json_key,
                optional,
                default,
                description,
            } => {
                let type_str: &str = if *optional {
                    "Option<String>"
                } else {
                    "String"
                };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Uuid {
                name,
                json_key,
                optional,
                default,
                description,
            } => {
                let type_str: &str = if *optional { "Option<Uuid>" } else { "Uuid" };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Boolean {
                name,
                json_key,
                optional,
                default,
                description,
            } => {
                let type_str: &str = if *optional { "Option<bool>" } else { "bool" };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Integer {
                name,
                json_key,
                optional,
                default,
                description,
                integer_type,
            } => {
                let type_str: String = if *optional {
                    format!("Option<{integer_type}>")
                } else {
                    integer_type.clone()
                };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Number {
                name,
                json_key,
                optional,
                default,
                description,
                number_type,
            } => {
                let type_str: String = if *optional {
                    format!("Option<{number_type}>")
                } else {
                    number_type.clone()
                };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Object {
                name,
                json_key,
                type_name,
                optional,
                default,
                description,
            }
            | FieldDef::Enum {
                name,
                json_key,
                type_name,
                optional,
                default,
                description,
            } => {
                let type_str: String = if *optional {
                    format!("Option<{type_name}>")
                } else {
                    type_name.clone()
                };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::Array {
                name,
                json_key,
                element_type,
                optional,
                default,
                description,
            } => {
                let type_str: String = if *optional {
                    format!("Option<Vec<{element_type}>>")
                } else {
                    format!("Vec<{element_type}>")
                };
                emit_field_attrs(
                    writer,
                    name,
                    json_key,
                    default.as_ref(),
                    description.as_deref(),
                )?;
                writeln!(writer, "    pub {name}: {type_str},")?;
            }
            FieldDef::AdditionalProperties { name, value_type } => {
                writeln!(writer, "    #[serde(flatten)]")?;
                writeln!(writer, "    pub {name}: BTreeMap<String, {value_type}>,")?;
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
                    FieldDef::AdditionalProperties { value_type, .. }
                        if struct_defs.contains_key(value_type) =>
                    {
                        visit(value_type, struct_defs, order, visited);
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
    let needs_btreemap: bool = collected.values().any(|s| {
        s.fields
            .iter()
            .any(|f| matches!(f, FieldDef::AdditionalProperties { .. }))
    });
    if needs_btreemap {
        writeln!(writer, "use std::collections::BTreeMap;")?;
    }
    let needs_uuid: bool = collected.values().any(|s| {
        s.fields.iter().any(|f| match f {
            FieldDef::Uuid { .. } => true,
            FieldDef::AdditionalProperties { value_type, .. } => value_type == "Uuid",
            FieldDef::Array { element_type, .. } => element_type == "Uuid",
            _ => false,
        })
    });
    if needs_uuid {
        writeln!(writer, "use uuid::Uuid;")?;
    }
    writeln!(writer)?;

    // Emit enums first (alphabetically), then default functions, then structs (topological order)
    for enum_name in collected_enums.keys() {
        if let Some(enum_def) = collected_enums.get(enum_name) {
            emit_enum(enum_def, writer)?;
        }
    }
    let order: Vec<String> = emission_order(&collected, &root_name);
    emit_default_functions(&collected, &order, writer)?;
    for name in &order {
        if let Some(struct_def) = collected.get(name) {
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
    fn is_uuid_format_accepts_uuid_and_versions() {
        assert!(is_uuid_format(Some("uuid")));
        assert!(is_uuid_format(Some("UUID")));
        assert!(is_uuid_format(Some("uuid1")));
        assert!(is_uuid_format(Some("uuid4")));
        assert!(is_uuid_format(Some("uuid7")));
        assert!(is_uuid_format(Some("Uuid4")));
    }

    #[test]
    fn is_uuid_format_rejects_other_formats() {
        assert!(!is_uuid_format(None));
        assert!(!is_uuid_format(Some("")));
        assert!(!is_uuid_format(Some("date-time")));
        assert!(!is_uuid_format(Some("email")));
        assert!(!is_uuid_format(Some("uri")));
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
    fn generate_schema_optional_keyword_is_ignored() {
        // Per-property `optional` is recognized but ignored; required/optional from `required` array only.
        let baseline: &str = r#"{
            "type": "object",
            "title": "IgnoreOptional",
            "required": ["a"],
            "properties": {
                "a": { "type": "string" },
                "b": { "type": "string" }
            }
        }"#;
        let with_optional_true_on_required: &str = r#"{
            "type": "object",
            "title": "IgnoreOptional",
            "required": ["a"],
            "properties": {
                "a": { "type": "string", "optional": true },
                "b": { "type": "string" }
            }
        }"#;
        let with_optional_false_on_optional: &str = r#"{
            "type": "object",
            "title": "IgnoreOptional",
            "required": ["a"],
            "properties": {
                "a": { "type": "string" },
                "b": { "type": "string", "optional": false }
            }
        }"#;
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IgnoreOptional {
    pub a: String,
    pub b: Option<String>,
}

";

        for schema_json in [
            baseline,
            with_optional_true_on_required,
            with_optional_false_on_optional,
        ] {
            let mut output: Vec<u8> = Vec::new();
            generate_to_writer(schema_json, &mut output)
                .expect("generate_to_writer should succeed");
            let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");
            assert_eq!(
                expected, actual,
                "output must be unchanged when optional keyword is present"
            );
        }
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
    fn generate_schema_default_null_required_field_ignored() {
        // default: null on required field is unsupported; no default attribute emitted
        let schema_json: &str = r#"{
            "type": "object",
            "title": "ReqNull",
            "required": ["x"],
            "properties": {
                "x": { "type": "string", "default": null }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReqNull {
    pub x: String,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(
            expected, actual,
            "default null on required field should be ignored"
        );
    }

    #[test]
    fn generate_schema_default_empty_string_uses_serde_default() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "EmptyStr",
            "properties": {
                "name": { "type": "string", "default": "" }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmptyStr {
    #[serde(default)]
    pub name: Option<String>,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(
            expected, actual,
            "default empty string should use #[serde(default)]"
        );
    }

    #[test]
    fn generate_schema_default_float_zero_uses_serde_default() {
        let schema_json: &str = r#"{
            "type": "object",
            "title": "ZeroF64",
            "properties": {
                "ratio": { "type": "number", "default": 0 }
            }
        }"#;

        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ZeroF64 {
    #[serde(default)]
    pub ratio: Option<f64>,
}

";

        let mut output: Vec<u8> = Vec::new();
        generate_to_writer(schema_json, &mut output).expect("generate_to_writer should succeed");

        let actual: String = String::from_utf8(output).expect("output should be valid UTF-8");

        assert_eq!(
            expected, actual,
            "default 0 for number should use #[serde(default)]"
        );
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

    // Exhaustive unit tests for choose_integer_type (one per type path + fallbacks)
    fn schema_with_min_max(min: Option<i64>, max: Option<i64>) -> JsonSchema {
        let mut schema_json = serde_json::json!({ "type": "integer" });
        if let Some(m) = min {
            schema_json["minimum"] = serde_json::json!(m);
        }
        if let Some(m) = max {
            schema_json["maximum"] = serde_json::json!(m);
        }
        serde_json::from_value(schema_json).expect("valid schema")
    }

    fn schema_number_with_min_max(min: Option<f64>, max: Option<f64>) -> JsonSchema {
        let mut schema_json = serde_json::json!({ "type": "number" });
        if let Some(m) = min {
            schema_json["minimum"] = serde_json::json!(m);
        }
        if let Some(m) = max {
            schema_json["maximum"] = serde_json::json!(m);
        }
        serde_json::from_value(schema_json).expect("valid schema")
    }

    #[test]
    fn choose_integer_type_returns_i8() {
        let schema: JsonSchema = schema_with_min_max(Some(-128), Some(127));
        assert_eq!(choose_integer_type(&schema), "i8");
    }

    #[test]
    fn choose_integer_type_returns_u8() {
        let schema: JsonSchema = schema_with_min_max(Some(0), Some(255));
        assert_eq!(choose_integer_type(&schema), "u8");
    }

    #[test]
    fn choose_integer_type_returns_i16() {
        let schema: JsonSchema = schema_with_min_max(Some(-32768), Some(32767));
        assert_eq!(choose_integer_type(&schema), "i16");
    }

    #[test]
    fn choose_integer_type_returns_u16() {
        let schema: JsonSchema = schema_with_min_max(Some(0), Some(65535));
        assert_eq!(choose_integer_type(&schema), "u16");
    }

    #[test]
    fn choose_integer_type_returns_i32() {
        let schema: JsonSchema = schema_with_min_max(Some(-2_147_483_648), Some(2_147_483_647));
        assert_eq!(choose_integer_type(&schema), "i32");
    }

    #[test]
    fn choose_integer_type_returns_u32() {
        let schema: JsonSchema = schema_with_min_max(Some(0), Some(4_294_967_295));
        assert_eq!(choose_integer_type(&schema), "u32");
    }

    #[test]
    fn choose_integer_type_returns_i64() {
        let schema: JsonSchema = schema_with_min_max(Some(-2_147_483_649), Some(2_147_483_647));
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_integer_type_returns_u64() {
        let schema: JsonSchema = schema_with_min_max(Some(0), Some(9_223_372_036_854_775_807));
        assert_eq!(choose_integer_type(&schema), "u64");
    }

    #[test]
    fn choose_integer_type_fallback_when_no_bounds() {
        let schema: JsonSchema = schema_with_min_max(None, None);
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_integer_type_fallback_when_only_minimum() {
        let schema: JsonSchema = schema_with_min_max(Some(0), None);
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_integer_type_fallback_when_only_maximum() {
        let schema: JsonSchema = schema_with_min_max(None, Some(255));
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_integer_type_fallback_when_non_integer_bounds() {
        let schema_json = serde_json::json!({
            "type": "integer",
            "minimum": 1.5,
            "maximum": 255.5
        });
        let schema: JsonSchema = serde_json::from_value(schema_json).expect("valid");
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_integer_type_fallback_when_min_gt_max() {
        let schema: JsonSchema = schema_with_min_max(Some(255), Some(0));
        assert_eq!(choose_integer_type(&schema), "i64");
    }

    #[test]
    fn choose_number_type_returns_f32() {
        let schema: JsonSchema = schema_number_with_min_max(Some(0.0), Some(1.0));
        assert_eq!(choose_number_type(&schema), "f32");
    }

    #[test]
    fn choose_number_type_returns_f64() {
        let schema: JsonSchema = schema_number_with_min_max(Some(0.0), Some(1e40));
        assert_eq!(choose_number_type(&schema), "f64");
    }

    #[test]
    fn choose_number_type_f64_when_bounds_missing() {
        let schema: JsonSchema = schema_number_with_min_max(None, None);
        assert_eq!(choose_number_type(&schema), "f64");
    }

    #[test]
    fn choose_number_type_f64_when_bounds_outside_f32_range() {
        let schema: JsonSchema = schema_number_with_min_max(Some(-4e38), Some(4e38));
        assert_eq!(choose_number_type(&schema), "f64");
    }
}
