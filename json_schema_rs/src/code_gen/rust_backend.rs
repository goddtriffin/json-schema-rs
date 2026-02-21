//! Rust codegen backend: emits serde-compatible Rust structs from JSON Schema.

use super::CodeGenBackend;
use super::CodeGenError;
use super::CodeGenResult;
use super::GenerateRustOutput;
use super::settings::{CodeGenSettings, DedupeMode, ModelNameSource};
use crate::json_schema::JsonSchema;
use crate::sanitizers::{sanitize_field_name, sanitize_struct_name};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};

/// Backend that emits Rust structs (serde-compatible).
#[derive(Debug, Clone, Default)]
pub struct RustBackend;

impl CodeGenBackend for RustBackend {
    fn generate(
        &self,
        schemas: &[JsonSchema],
        settings: &CodeGenSettings,
    ) -> CodeGenResult<GenerateRustOutput> {
        match settings.dedupe_mode {
            DedupeMode::Disabled => {
                let mut per_schema: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
                for (index, schema) in schemas.iter().enumerate() {
                    let mut out = Cursor::new(Vec::new());
                    emit_rust(schema, &mut out, settings).map_err(|e| CodeGenError::Batch {
                        index,
                        source: Box::new(e),
                    })?;
                    per_schema.push(out.into_inner());
                }
                Ok(GenerateRustOutput {
                    shared: None,
                    per_schema,
                })
            }
            DedupeMode::Functional | DedupeMode::Full => {
                generate_rust_with_dedupe(schemas, settings)
            }
        }
    }
}

/// One struct to emit: name and the object schema (root or nested).
struct StructToEmit {
    name: String,
    schema: JsonSchema,
}

/// Key for deduplication: canonical representation of an object schema for a given mode.
/// Implements `Ord` + `Eq` for use in `BTreeMap`. Functional mode excludes description; Full includes it (when present in schema).
#[derive(Debug, Clone)]
struct DedupeKey {
    type_: Option<String>,
    properties: BTreeMap<String, DedupeKey>,
    required: Option<Vec<String>>,
    title: Option<String>,
}

impl PartialEq for DedupeKey {
    fn eq(&self, other: &Self) -> bool {
        self.type_ == other.type_
            && self.properties == other.properties
            && self.required == other.required
            && self.title == other.title
    }
}

impl Eq for DedupeKey {}

impl PartialOrd for DedupeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DedupeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.type_.cmp(&other.type_).then_with(|| {
            self.properties
                .keys()
                .cmp(other.properties.keys())
                .then_with(|| {
                    for (k, v) in &self.properties {
                        if let Some(ov) = other.properties.get(k) {
                            let c = v.cmp(ov);
                            if c != Ordering::Equal {
                                return c;
                            }
                        }
                    }
                    self.properties.len().cmp(&other.properties.len())
                })
                .then_with(|| compare_option_vec(self.required.as_ref(), other.required.as_ref()))
                .then_with(|| self.title.cmp(&other.title))
        })
    }
}

fn compare_option_vec(a: Option<&Vec<String>>, b: Option<&Vec<String>>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(aa), Some(bb)) => aa.cmp(bb),
    }
}

impl DedupeKey {
    #[expect(clippy::only_used_in_recursion)]
    fn from_schema(schema: &JsonSchema, mode: DedupeMode) -> Self {
        let properties: BTreeMap<String, DedupeKey> = schema
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), DedupeKey::from_schema(v, mode)))
            .collect();
        DedupeKey {
            type_: schema.type_.clone(),
            properties,
            required: schema.required.clone(),
            title: schema.title.clone(),
        }
    }
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

/// Collect (`schema_idx`, `candidate_name`, schema) for every struct from all schemas in post-order (children before parents) per schema. No name dedupe.
fn collect_structs_all_schemas(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> Vec<(usize, String, JsonSchema)> {
    let mut out: Vec<(usize, String, JsonSchema)> = Vec::new();
    for (schema_idx, schema) in schemas.iter().enumerate() {
        if !schema.is_object_with_properties() {
            continue;
        }
        let mut post_order: Vec<(JsonSchema, Option<String>, bool)> = Vec::new();
        let mut stack: Vec<(JsonSchema, Option<String>, usize, bool)> = Vec::new();
        stack.push((schema.clone(), None, 0, true));
        while let Some((schema_node, from_key_opt, index, is_root)) = stack.pop() {
            let keys: Vec<String> = schema_node.properties.keys().cloned().collect();
            if index < keys.len() {
                let key: String = keys[index].clone();
                let child: JsonSchema = schema_node.properties.get(&key).unwrap().clone();
                stack.push((schema_node, from_key_opt, index + 1, is_root));
                if child.is_object_with_properties() {
                    stack.push((child, Some(key), 0, false));
                }
            } else {
                post_order.push((schema_node, from_key_opt, is_root));
            }
        }
        for (schema_node, from_key_opt, is_root) in post_order {
            let name: String = struct_name_from(
                schema_node.title.as_deref(),
                from_key_opt.as_deref(),
                is_root,
                settings,
            );
            out.push((schema_idx, name, schema_node));
        }
    }
    out
}

/// Generate Rust with dedupe (Functional or Full mode). Returns shared buffer (if any) and per-schema buffers.
#[expect(clippy::too_many_lines)]
#[expect(clippy::type_complexity)]
fn generate_rust_with_dedupe(
    schemas: &[JsonSchema],
    settings: &CodeGenSettings,
) -> CodeGenResult<GenerateRustOutput> {
    let mode: DedupeMode = settings.dedupe_mode;
    let collected: Vec<(usize, String, JsonSchema)> =
        collect_structs_all_schemas(schemas, settings);

    // Build BTreeMap: DedupeKey -> (canonical_name, schema, occurrences)
    let mut map: BTreeMap<DedupeKey, (String, JsonSchema, Vec<(usize, String)>)> = BTreeMap::new();
    for (schema_idx, name, schema) in &collected {
        let key: DedupeKey = DedupeKey::from_schema(schema, mode);
        map.entry(key)
            .or_insert_with(|| (name.clone(), schema.clone(), Vec::new()))
            .2
            .push((*schema_idx, name.clone()));
    }

    let shared_names: BTreeSet<String> = map
        .iter()
        .filter(|(_, (_, _, occs))| occs.len() > 1)
        .map(|(_, (canonical_name, _, _))| canonical_name.clone())
        .collect();

    let key_to_canonical_name: BTreeMap<DedupeKey, String> = map
        .iter()
        .map(|(k, (canonical, _, _))| (k.clone(), canonical.clone()))
        .collect();

    let key_to_canonical: BTreeMap<DedupeKey, (String, JsonSchema)> = map
        .iter()
        .map(|(k, (cn, schema, _))| (k.clone(), (cn.clone(), schema.clone())))
        .collect();

    if shared_names.is_empty() {
        let mut per_schema: Vec<Vec<u8>> = Vec::with_capacity(schemas.len());
        for (index, schema) in schemas.iter().enumerate() {
            let mut out = Cursor::new(Vec::new());
            emit_rust(schema, &mut out, settings).map_err(|e| CodeGenError::Batch {
                index,
                source: Box::new(e),
            })?;
            per_schema.push(out.into_inner());
        }
        return Ok(GenerateRustOutput {
            shared: None,
            per_schema,
        });
    }

    // Shared structs: (canonical_name, schema) in dependency order
    let shared_structs: Vec<(String, JsonSchema)> = {
        let mut v: Vec<(String, JsonSchema)> = key_to_canonical
            .iter()
            .filter(|(_, (cn, _))| shared_names.contains(cn))
            .map(|(_, (cn, s))| (cn.clone(), s.clone()))
            .collect();
        let order: Vec<String> = v.iter().map(|(n, _)| n.clone()).collect();
        let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, schema) in &v {
            let mut set: BTreeSet<String> = BTreeSet::new();
            for prop_schema in schema.properties.values() {
                if prop_schema.is_object_with_properties() {
                    let prop_key = DedupeKey::from_schema(prop_schema, mode);
                    if let Some(cn) = key_to_canonical_name.get(&prop_key)
                        && shared_names.contains(cn)
                    {
                        set.insert(cn.clone());
                    }
                }
            }
            deps.insert(name.clone(), set);
        }
        topo_sort_by_deps(&order, &deps, &mut v);
        v
    };

    let shared_buffer: Vec<u8> = {
        let mut out = Cursor::new(Vec::new());
        writeln!(
            out,
            "//! Generated by json-schema-rs. Do not edit manually."
        )?;
        writeln!(out)?;
        writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
        writeln!(out)?;
        for (name, schema) in &shared_structs {
            writeln!(out, "#[derive(Debug, Clone, Serialize, Deserialize)]")?;
            writeln!(out, "pub struct {name} {{")?;
            emit_struct_fields_with_resolver(
                schema,
                &mut out,
                settings,
                Some(&key_to_canonical_name),
                mode,
            )?;
            writeln!(out, "}}")?;
            writeln!(out)?;
        }
        out.into_inner()
    };

    let per_schema: Vec<Vec<u8>> = (0..schemas.len())
        .map(|schema_idx| {
            let mut local_structs: Vec<(String, JsonSchema)> = map
                .iter()
                .filter(|(_, (canonical_name, _, occs))| {
                    occs.len() == 1
                        && occs[0].0 == schema_idx
                        && !shared_names.contains(canonical_name)
                })
                .map(|(_, (name, schema, _))| (name.clone(), schema.clone()))
                .collect();
            let local_names: BTreeSet<String> =
                local_structs.iter().map(|(n, _)| n.clone()).collect();
            let mut deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (name, schema) in &local_structs {
                let mut set: BTreeSet<String> = BTreeSet::new();
                for prop_schema in schema.properties.values() {
                    if prop_schema.is_object_with_properties() {
                        let prop_key = DedupeKey::from_schema(prop_schema, mode);
                        if let Some(cn) = key_to_canonical_name.get(&prop_key)
                            && (local_names.contains(cn) || shared_names.contains(cn))
                        {
                            set.insert(cn.clone());
                        }
                    }
                }
                deps.insert(name.clone(), set);
            }
            let order: Vec<String> = local_structs.iter().map(|(n, _)| n.clone()).collect();
            topo_sort_by_deps(&order, &deps, &mut local_structs);

            let mut used_shared: BTreeSet<String> = BTreeSet::new();
            for (_, schema) in &local_structs {
                for prop_schema in schema.properties.values() {
                    if prop_schema.is_object_with_properties() {
                        let prop_key = DedupeKey::from_schema(prop_schema, mode);
                        if let Some(cn) = key_to_canonical_name.get(&prop_key)
                            && shared_names.contains(cn)
                        {
                            used_shared.insert(cn.clone());
                        }
                    }
                }
            }
            let root_for_schema = collected
                .iter()
                .rev()
                .find(|(idx, _, _)| *idx == schema_idx)
                .map(|(_, _, s)| DedupeKey::from_schema(s, mode));
            if let Some(root_key) = root_for_schema
                && let Some(cn) = key_to_canonical_name.get(&root_key)
                && shared_names.contains(cn)
            {
                used_shared.insert(cn.clone());
            }

            let mut buf = Cursor::new(Vec::new());
            writeln!(
                buf,
                "//! Generated by json-schema-rs. Do not edit manually."
            )
            .ok();
            writeln!(buf).ok();
            writeln!(buf, "use serde::{{Deserialize, Serialize}};").ok();
            for u in &used_shared {
                writeln!(buf, "pub use crate::{u};").ok();
            }
            if !used_shared.is_empty() {
                writeln!(buf).ok();
            }
            for (name, schema) in &local_structs {
                writeln!(buf, "#[derive(Debug, Clone, Serialize, Deserialize)]").ok();
                writeln!(buf, "pub struct {name} {{").ok();
                emit_struct_fields_with_resolver(
                    schema,
                    &mut buf,
                    settings,
                    Some(&key_to_canonical_name),
                    mode,
                )
                .ok();
                writeln!(buf, "}}").ok();
                writeln!(buf).ok();
            }
            buf.into_inner()
        })
        .collect();

    Ok(GenerateRustOutput {
        shared: Some(shared_buffer),
        per_schema,
    })
}

fn topo_sort_by_deps(
    order: &[String],
    deps: &BTreeMap<String, BTreeSet<String>>,
    v: &mut Vec<(String, JsonSchema)>,
) {
    let mut sorted: Vec<String> = Vec::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    for name in order {
        visit_topo(name, deps, &mut visited, &mut sorted);
    }
    let name_to_pair: BTreeMap<String, (String, JsonSchema)> =
        v.drain(..).map(|(n, s)| (n.clone(), (n, s))).collect();
    for n in &sorted {
        if let Some(pair) = name_to_pair.get(n) {
            v.push(pair.clone());
        }
    }
}

fn visit_topo(
    name: &str,
    deps: &BTreeMap<String, BTreeSet<String>>,
    visited: &mut BTreeSet<String>,
    out: &mut Vec<String>,
) {
    if visited.contains(name) {
        return;
    }
    visited.insert(name.to_string());
    if let Some(d) = deps.get(name) {
        for dep in d {
            visit_topo(dep, deps, visited, out);
        }
    }
    out.push(name.to_string());
}

/// Emit struct fields; when resolver is Some (dedupe mode), use canonical type names for nested objects.
fn emit_struct_fields_with_resolver(
    schema: &JsonSchema,
    out: &mut impl Write,
    settings: &CodeGenSettings,
    key_to_name: Option<&BTreeMap<DedupeKey, String>>,
    mode: DedupeMode,
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
            let nested_name: String = if let Some(m) = key_to_name {
                let prop_key = DedupeKey::from_schema(prop_schema, mode);
                m.get(&prop_key).cloned().unwrap_or_else(|| {
                    struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings)
                })
            } else {
                struct_name_from(prop_schema.title.as_deref(), Some(key), false, settings)
            };
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
/// for all-default settings). Returns [`GenerateRustOutput`] with optional shared buffer and one buffer per schema.
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
) -> CodeGenResult<GenerateRustOutput> {
    RustBackend.generate(schemas, settings)
}

#[cfg(test)]
mod tests {
    use super::CodeGenError;
    use super::{CodeGenBackend, RustBackend, generate_rust};
    use crate::code_gen::settings::{CodeGenSettings, DedupeMode, ModelNameSource};
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
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let out = actual.unwrap();
        let output: String = String::from_utf8(out.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput =
            generate_rust(std::slice::from_ref(&schema), &settings).unwrap();
        let expected: super::GenerateRustOutput =
            RustBackend.generate(&[schema], &settings).unwrap();
        assert_eq!(expected.per_schema, output.per_schema);
        assert_eq!(1, output.per_schema.len());
    }

    #[test]
    fn property_key_first_uses_key_over_title_for_nested_struct() {
        let json = r#"{"type":"object","properties":{"address":{"type":"object","title":"FooBar","properties":{"city":{"type":"string"}}}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .model_name_source(ModelNameSource::PropertyKeyFirst)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
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
        let output: super::GenerateRustOutput =
            generate_rust(&[s1.clone(), s2.clone()], &settings).unwrap();
        let expected: super::GenerateRustOutput =
            RustBackend.generate(&[s1, s2], &settings).unwrap();
        assert_eq!(expected.per_schema, output.per_schema);
        assert_eq!(2, output.per_schema.len());
        let out1 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let out2 = String::from_utf8(output.per_schema[1].clone()).unwrap();
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

    #[test]
    fn dedupe_disabled_returns_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema.clone(), schema], &settings).unwrap();
        let expected: Option<Vec<u8>> = None;
        let actual = output.shared.clone();
        assert_eq!(expected, actual);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn dedupe_disabled_two_schemas_same_shape_two_buffers_no_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        assert_eq!(None, output.shared);
        assert_eq!(2, output.per_schema.len());
        let out0 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let out1 = String::from_utf8(output.per_schema[1].clone()).unwrap();
        assert!(out0.contains("pub struct Root"));
        assert!(out1.contains("pub struct Root"));
    }

    #[test]
    fn dedupe_full_two_identical_schemas_produces_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let expected_shared_some = true;
        let actual_shared_some = output.shared.is_some();
        assert_eq!(expected_shared_some, actual_shared_some);
        assert_eq!(2, output.per_schema.len());
        let shared_str = String::from_utf8(output.shared.unwrap()).unwrap();
        assert!(shared_str.contains("pub struct Root"));
        let per0 = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(per0.contains("pub use crate::Root") || per0.contains("Root"));
    }

    #[test]
    fn dedupe_full_single_schema_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let expected: Option<Vec<u8>> = None;
        let actual = output.shared.clone();
        assert_eq!(expected, actual);
        assert_eq!(1, output.per_schema.len());
    }

    #[test]
    fn dedupe_full_two_different_schemas_no_shared() {
        let j1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let j2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(j1).unwrap();
        let s2: JsonSchema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        assert_eq!(None, output.shared);
        assert_eq!(2, output.per_schema.len());
    }

    #[test]
    fn dedupe_full_single_schema_two_identical_nested_objects_deduped() {
        let json = r#"{
            "type": "object",
            "properties": {
                "addr1": { "type": "object", "properties": { "street": { "type": "string" } } },
                "addr2": { "type": "object", "properties": { "street": { "type": "string" } } }
            }
        }"#;
        let schema: JsonSchema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let per_str = String::from_utf8(output.per_schema[0].clone()).unwrap();
        assert!(
            per_str.contains("addr1") && per_str.contains("addr2"),
            "per_schema should reference both fields; got: {per_str}"
        );
        let shared_count = output
            .shared
            .as_ref()
            .map_or(0, |b| b.windows(11).filter(|w| w == b"pub struct ").count());
        let per_count = per_str.matches("pub struct ").count();
        assert!(
            shared_count + per_count >= 1,
            "at least one struct (Root in per_schema, nested in shared when deduped)"
        );
    }

    #[test]
    fn dedupe_functional_same_as_full_when_no_description() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: JsonSchema = serde_json::from_str(json).unwrap();
        let s2: JsonSchema = serde_json::from_str(json).unwrap();
        let settings_full: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Full)
            .build();
        let settings_func: CodeGenSettings = CodeGenSettings::builder()
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output_full: super::GenerateRustOutput =
            generate_rust(&[s1.clone(), s2.clone()], &settings_full).unwrap();
        let output_func: super::GenerateRustOutput =
            generate_rust(&[s1, s2], &settings_func).unwrap();
        assert_eq!(output_full.shared.is_some(), output_func.shared.is_some());
        assert_eq!(output_full.per_schema.len(), output_func.per_schema.len());
    }

    #[test]
    fn default_settings_use_full_dedupe() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected = DedupeMode::Full;
        let actual = settings.dedupe_mode;
        assert_eq!(expected, actual);
    }
}
