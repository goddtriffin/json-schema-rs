//! Checks that merging self-validating types never changes what they validate.
//!
//! Deduplication merges two types when their targets validate the same (design.md,
//! *Self-validating types*, *Identity*). These tests check the merges against the validator
//! itself, independently of how the equivalence is computed: for every pair of nodes deduplication
//! merged, each with its own target, every instance and every value nested in it is validated at
//! both targets with the library validator, and the two results (verdict and every error) must be
//! equal. The inputs are the official JSON Schema Test Suite (each file's
//! schemas generated together as separate documents, so types merge within and across documents,
//! and validated against every instance of the file) and the benchmark fixtures.

use super::input::{CachingRetriever, CodeGenInput};
use super::rust_backend::prepare_graph;
use super::settings::CodeGenSettings;
use super::type_graph::{TypeGraph, TypeId, ValidationTarget};
use crate::json_schema::{FormatMode, JsonSchemaSettings, Registry, Schema, SpecVersion};
use crate::validator::{
    CompiledEntries, CompiledSchema, NodeId, ValidationResult, validation_classes,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The repository root.
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Every `.json` file under `dir`, sorted by path (iteratively).
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn read_json(path: &Path) -> Value {
    let text: String = std::fs::read_to_string(path).expect("readable test file");
    serde_json::from_str(&text).expect("test file is JSON")
}

/// `values` and every value nested in them, each once.
fn with_nested(values: &[Value]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack: Vec<&Value> = values.iter().rev().collect();
    while let Some(value) = stack.pop() {
        if seen.insert(value.to_string()) {
            out.push(value.clone());
        }
        match value {
            Value::Array(items) => stack.extend(items.iter().rev()),
            Value::Object(members) => stack.extend(members.values().rev()),
            _ => {}
        }
    }
    out
}

/// What one check found.
#[derive(Debug, Default)]
struct Report {
    /// Merged pairs checked.
    pairs: usize,
    /// Differences: (where, which instance, the two results).
    mismatches: Vec<String>,
}

/// Generates `schemas` together (as separate input documents, with `retriever` serving the
/// documents their `$ref`s name), then checks every merged pair of targets against `instances`.
/// A generation that fails is not checked (the suite holds schemas codegen rejects); returns
/// whether it was.
/// Whether `schemas` generate together.
fn generates(
    schemas: &[Schema],
    retriever: &BTreeMap<String, Value>,
    settings: &JsonSchemaSettings,
) -> bool {
    check_generation(
        "",
        schemas,
        retriever,
        settings,
        &[],
        &mut Report::default(),
    )
}

fn check_generation(
    name: &str,
    schemas: &[Schema],
    retriever: &BTreeMap<String, Value>,
    settings: &JsonSchemaSettings,
    instances: &[Value],
    report: &mut Report,
) -> bool {
    let input: CodeGenInput<'_> = schemas
        .iter()
        .fold(CodeGenInput::new(), CodeGenInput::schema)
        .retriever(retriever.clone())
        .settings(settings.clone());
    let all: Vec<&Schema> = input.schemas().collect();
    let caching: CachingRetriever<'_> = input.caching_retriever();
    let registries: Result<Vec<Registry<'_>>, ()> = all
        .iter()
        .enumerate()
        .map(|(index, schema)| {
            input
                .registry_builder(index, &caching)
                .build(*schema)
                .map_err(|_| ())
        })
        .collect();
    let Ok(registries) = registries else {
        return false;
    };
    let Ok((graph, Some(_), merged)) = prepare_graph(
        &all,
        &registries,
        input.retrieval_settings(),
        &CodeGenSettings::default(),
    ) else {
        return false;
    };
    // An input's targets as the library validator sees them: the input's registry, built with
    // their URIs as entries, compiled once for all of them.
    let library = |schema: usize, entries: &[Vec<String>]| -> Option<CompiledEntries> {
        let builder = entries
            .iter()
            .flatten()
            .fold(input.registry_builder(schema, &caching), |b, uri| {
                b.entry(uri.as_str())
            });
        let registry: Registry<'_> = builder.build(all[schema]).ok()?;
        let uris: Vec<Vec<&str>> = entries
            .iter()
            .map(|e| e.iter().map(String::as_str).collect())
            .collect();
        let lists: Vec<&[&str]> = uris.iter().map(Vec::as_slice).collect();
        CompiledSchema::with_entries(&registry, &lists).ok()
    };
    check_merges(name, &graph, &merged, library, instances, report);
    true
}

/// One input's targets to compile, and each one's entry index.
type Wanted = (Vec<Vec<String>>, HashMap<Vec<String>, usize>);

/// Compares every merged pair's validation on every instance.
fn check_merges(
    name: &str,
    graph: &TypeGraph,
    merged: &[TypeId],
    library: impl Fn(usize, &[Vec<String>]) -> Option<CompiledEntries>,
    instances: &[Value],
    report: &mut Report,
) {
    let mut pairs: Vec<(&ValidationTarget, &ValidationTarget)> = Vec::new();
    // Per input: the targets to compile, and each one's entry index.
    let mut wanted: BTreeMap<usize, Wanted> = BTreeMap::new();
    for (index, &into) in merged.iter().enumerate() {
        let (Some(member), Some(kept)) = (&graph.targets[index], &graph.targets[into.index()])
        else {
            continue;
        };
        if member == kept {
            continue;
        }
        for target in [member, kept] {
            let (entries, at) = wanted.entry(target.schema).or_default();
            if !at.contains_key(&target.uris) {
                at.insert(target.uris.clone(), entries.len());
                entries.push(target.uris.clone());
            }
        }
        pairs.push((member, kept));
    }
    let mut compiled: HashMap<usize, CompiledEntries> = HashMap::new();
    for (&schema, (entries, _)) in &wanted {
        match library(schema, entries) {
            Some(entries) => {
                compiled.insert(schema, entries);
            }
            None => report.mismatches.push(format!(
                "{name}: the merged targets of input {schema} do not compile"
            )),
        }
    }
    for (member, kept) in pairs {
        let at = |target: &ValidationTarget| {
            let (schema, nodes) = compiled.get(&target.schema)?;
            Some((schema, &nodes[wanted[&target.schema].1[&target.uris]]))
        };
        let (Some((member_schema, member_nodes)), Some((kept_schema, kept_nodes))) =
            (at(member), at(kept))
        else {
            continue;
        };
        report.pairs += 1;
        for instance in instances {
            let a: (ValidationResult, bool) = (
                member_schema.validate_at(member_nodes, instance),
                member_schema.is_valid_at(member_nodes, instance),
            );
            let b: (ValidationResult, bool) = (
                kept_schema.validate_at(kept_nodes, instance),
                kept_schema.is_valid_at(kept_nodes, instance),
            );
            if a != b {
                report.mismatches.push(format!(
                    "{name}: {:?} merged into {:?} validates {instance} differently: {a:?} / {b:?}",
                    member.uris, kept.uris
                ));
            }
        }
    }
}

/// The official test suite's root, its `remotes/` documents by URI, and its draft directories.
fn suite() -> (
    PathBuf,
    BTreeMap<String, Value>,
    [(&'static str, SpecVersion); 5],
) {
    let suite: PathBuf = repository_root()
        .join("research")
        .join("json-schema-test-suite");
    assert!(
        suite.join("tests").is_dir(),
        "JSON Schema Test Suite not found at {}. Run `make vendor_test_suite` to clone it.",
        suite.display()
    );
    let remotes_dir: PathBuf = suite.join("remotes");
    let remotes: BTreeMap<String, Value> = json_files(&remotes_dir)
        .into_iter()
        .map(|path| {
            let relative: String = path
                .strip_prefix(&remotes_dir)
                .expect("under remotes")
                .to_string_lossy()
                .replace('\\', "/");
            (
                format!("http://localhost:1234/{relative}"),
                read_json(&path),
            )
        })
        .collect();
    let drafts: [(&str, SpecVersion); 5] = [
        ("draft4", SpecVersion::Draft04),
        ("draft6", SpecVersion::Draft06),
        ("draft7", SpecVersion::Draft07),
        ("draft2019-09", SpecVersion::Draft201909),
        ("draft2020-12", SpecVersion::Draft202012),
    ];
    (suite, remotes, drafts)
}

/// Every schema of one suite draft (parsed with `format` asserted in `optional/format/`) and
/// every instance of the draft, with the values nested in them.
fn suite_draft(
    suite: &Path,
    draft: &str,
    settings: &JsonSchemaSettings,
) -> (Vec<(String, Schema)>, Vec<Value>) {
    let format_settings: JsonSchemaSettings = JsonSchemaSettings {
        format_mode: Some(FormatMode::Assertion),
        ..settings.clone()
    };
    let mut schemas: Vec<(String, Schema)> = Vec::new();
    let mut data: Vec<Value> = Vec::new();
    for file in json_files(&suite.join("tests").join(draft)) {
        let name: String = file
            .strip_prefix(suite)
            .map_or_else(|_| file.display().to_string(), |p| p.display().to_string());
        let parse_settings: &JsonSchemaSettings = if name.contains("optional/format/") {
            &format_settings
        } else {
            settings
        };
        let Value::Array(groups) = read_json(&file) else {
            continue;
        };
        for group in &groups {
            for test in group["tests"].as_array().into_iter().flatten() {
                data.push(test["data"].clone());
            }
            if let Ok(schema) = Schema::new_from_serde_value(&group["schema"], parse_settings) {
                schemas.push((name.clone(), schema));
            }
        }
    }
    (schemas, with_nested(&data))
}

/// The partition `validation_classes` computes is checked directly, not only through the types
/// it merges (whose Rust shape already keeps many schemas apart): every compiled node of every
/// schema of a suite draft is put in a class, and every node of a class must validate every
/// instance of the draft exactly like the class's first node.
#[test]
fn nodes_of_one_validation_class_validate_alike_on_the_test_suite() {
    let (suite, remotes, drafts) = suite();
    let mut mismatches: Vec<String> = Vec::new();
    let mut compared: usize = 0;
    for (draft, spec_version) in drafts {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .default_spec_version(spec_version)
            .build();
        let (schemas, instances) = suite_draft(&suite, draft, &settings);
        let compiled: Vec<(&str, CompiledSchema)> = schemas
            .iter()
            .filter_map(|(name, schema)| {
                let registry: Registry<'_> = Registry::builder()
                    .retriever(remotes.clone())
                    .settings(settings.clone())
                    .build(schema)
                    .ok()?;
                Some((name.as_str(), CompiledSchema::new(&registry).ok()?))
            })
            .collect();
        let refs: Vec<&CompiledSchema> = compiled.iter().map(|(_, c)| c).collect();
        let classes: Vec<Option<Vec<u32>>> = validation_classes(&refs);
        // Per class, its first node; every later node is compared with it.
        let mut first: HashMap<u32, (usize, NodeId)> = HashMap::new();
        for (at, classes) in classes.iter().enumerate() {
            let Some(classes) = classes else {
                continue;
            };
            for (index, &class) in classes.iter().enumerate() {
                let node: NodeId = NodeId::from_index(index);
                let Some(&(first_at, first_node)) = first.get(&class) else {
                    first.insert(class, (at, node));
                    continue;
                };
                compared += 1;
                let (a_name, a) = &compiled[at];
                let (b_name, b) = &compiled[first_at];
                for instance in &instances {
                    let left: (ValidationResult, bool) = (
                        a.validate_at(&[node], instance),
                        a.is_valid_at(&[node], instance),
                    );
                    let right: (ValidationResult, bool) = (
                        b.validate_at(&[first_node], instance),
                        b.is_valid_at(&[first_node], instance),
                    );
                    if left != right {
                        mismatches.push(format!(
                            "{draft}: node {index} of {a_name} and node {} of {b_name} validate {instance} differently: {left:?} / {right:?}",
                            first_node.index()
                        ));
                        break;
                    }
                }
            }
        }
    }
    let expected: (Vec<String>, bool) = (Vec::new(), true);
    let actual: (Vec<String>, bool) = (mismatches, compared > 0);
    assert_eq!(expected, actual);
}

#[test]
fn merged_types_validate_like_their_own_schemas_on_the_test_suite() {
    let (suite, remotes, drafts) = suite();
    let mut report: Report = Report::default();
    for (draft, spec_version) in drafts {
        let dir: PathBuf = suite.join("tests").join(draft);
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .default_spec_version(spec_version)
            .build();
        let format_settings: JsonSchemaSettings = JsonSchemaSettings {
            format_mode: Some(FormatMode::Assertion),
            ..settings.clone()
        };
        // Per file: its schemas that generate on their own, and its instances.
        let mut files: Vec<(String, Vec<Schema>, Vec<Value>)> = Vec::new();
        for file in json_files(&dir) {
            let name: String = file
                .strip_prefix(&suite)
                .map_or_else(|_| file.display().to_string(), |p| p.display().to_string());
            let parse_settings: &JsonSchemaSettings = if name.contains("optional/format/") {
                &format_settings
            } else {
                &settings
            };
            let Value::Array(groups) = read_json(&file) else {
                continue;
            };
            let mut schemas: Vec<Schema> = Vec::new();
            let mut data: Vec<Value> = Vec::new();
            for group in &groups {
                for test in group["tests"].as_array().into_iter().flatten() {
                    data.push(test["data"].clone());
                }
                if let Ok(schema) = Schema::new_from_serde_value(&group["schema"], parse_settings)
                    && generates(std::slice::from_ref(&schema), &remotes, &settings)
                {
                    schemas.push(schema);
                }
            }
            files.push((name, schemas, data));
        }
        // The whole draft together, so types of different files merge too, checked against
        // every instance of the draft; each file alone when that fails.
        let all: Vec<Schema> = files
            .iter()
            .flat_map(|(_, s, _)| s.iter().cloned())
            .collect();
        let data: Vec<Value> = files
            .iter()
            .flat_map(|(_, _, d)| d.iter().cloned())
            .collect();
        if !check_generation(
            draft,
            &all,
            &remotes,
            &settings,
            &with_nested(&data),
            &mut report,
        ) {
            for (name, schemas, data) in &files {
                check_generation(
                    name,
                    schemas,
                    &remotes,
                    &settings,
                    &with_nested(data),
                    &mut report,
                );
            }
        }
    }
    let expected: (Vec<String>, bool) = (Vec::new(), true);
    let actual: (Vec<String>, bool) = (report.mismatches, report.pairs > 0);
    assert_eq!(expected, actual);
}

#[test]
fn merged_types_validate_like_their_own_schemas_on_the_benchmark_fixtures() {
    let fixtures: PathBuf = repository_root()
        .join("json_schema_rs_benchmark")
        .join("fixtures");
    let mut report: Report = Report::default();
    for schema_path in json_files(&fixtures)
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|n| n == "schema.json"))
    {
        let dir: &Path = schema_path.parent().expect("fixture directory");
        let name: String = dir.display().to_string();
        let settings: JsonSchemaSettings = JsonSchemaSettings::default();
        let schema: Schema = Schema::new_from_serde_value(&read_json(&schema_path), &settings)
            .expect("fixture schema parses");
        // Resources are registered under their root `$id`.
        let resources: BTreeMap<String, Value> = json_files(&dir.join("resources"))
            .into_iter()
            .map(|path| {
                let document: Value = read_json(&path);
                let id: String = document["$id"].as_str().expect("resource $id").to_string();
                (id.trim_end_matches('#').to_string(), document)
            })
            .collect();
        let instances: Vec<Value> = with_nested(
            &json_files(&dir.join("instances"))
                .iter()
                .map(|path| read_json(path))
                .collect::<Vec<Value>>(),
        );
        let checked: bool = check_generation(
            &name,
            std::slice::from_ref(&schema),
            &resources,
            &settings,
            &instances,
            &mut report,
        );
        if !checked {
            report.mismatches.push(format!("{name}: codegen failed"));
        }
    }
    let expected: (Vec<String>, bool) = (Vec::new(), true);
    let actual: (Vec<String>, bool) = (report.mismatches, report.pairs > 0);
    assert_eq!(expected, actual);
}
