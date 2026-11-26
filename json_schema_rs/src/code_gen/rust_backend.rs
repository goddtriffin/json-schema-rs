//! Rust codegen backend: lowers the [`TypeGraph`] to serde-compatible Rust items and renders
//! them (see design.md, *Codegen backends*).
//!
//! Pipeline: one `$ref` registry per input schema → [`type_graph::build`] → passes (variant
//! order, dedup, cycle breaking) → placement (shared vs per-schema output) → name allocation →
//! item order → rendering. Every step is a pure function of the input schemas, so the output is
//! byte-identical across runs.

use super::CodeGenBackend;
use super::CodeGenError;
use super::CodeGenResult;
use super::GenerateRustOutput;
use super::doc_text;
use super::input::{CachingRetriever, CodeGenInput};
use super::rust_validators::{self, BufferValidators, EmbeddedSet, Entry, UnionEntries};
use super::schema_defaults::schema_defaults;
use super::settings::{CodeGenSettings, DedupeMode, MapType, TypePatch, UniqueItemsType};
use super::type_graph::passes;
use super::type_graph::{
    self, Annotations, EnumNode, Field, IntWidth, NameProposal, NameTier, Newtype,
    NumberConstraints, ObjectNode, StringFormat, TargetSources, TypeGraph, TypeId, TypeNode,
    UnionKind, UnionNode,
};
use super::variant_names::lint_clean_variant_names;
use crate::json_schema::{IntegerSemantics, JsonSchemaSettings, Registry, Schema};
use crate::number;
use crate::sanitizers::{
    RenameRule, enum_variant_name_from_value, is_rust_keyword, sanitize_field_name,
    sanitize_struct_name, to_pascal_case,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::{self, Write as _};
use std::io::Write;

/// Backend that emits Rust structs (serde-compatible).
#[derive(Debug, Clone, Default)]
pub struct RustBackend;

impl CodeGenBackend for RustBackend {
    fn generate(
        &self,
        input: &CodeGenInput<'_>,
        settings: &CodeGenSettings,
    ) -> CodeGenResult<GenerateRustOutput> {
        let schemas: Vec<&Schema> = input.schemas().collect();
        let retriever: CachingRetriever<'_> = input.caching_retriever();
        let registries: Vec<Registry<'_>> = schemas
            .iter()
            .enumerate()
            .map(|(index, schema)| {
                input
                    .registry_builder(index, &retriever)
                    .build(*schema)
                    .map_err(|e| CodeGenError::Batch {
                        index,
                        source: Box::new(CodeGenError::Registry(e)),
                    })
            })
            .collect::<CodeGenResult<Vec<Registry<'_>>>>()?;
        let (mut graph, sets, _): (TypeGraph, Option<Vec<EmbeddedSet>>, Vec<TypeId>) =
            prepare_graph(&schemas, &registries, input.retrieval_settings(), settings)?;
        let mut plan: Plan = Plan::new(&mut graph, settings)?;
        if settings.schema_defaults {
            let read: Vec<EmbeddedSet>;
            let sets: &[EmbeddedSet] = if let Some(sets) = &sets {
                sets
            } else {
                read = rust_validators::embedded_sets(&registries, input.retrieval_settings())?;
                &read
            };
            plan.defaults = schema_defaults(&graph, sets, plan.items.keys().copied());
        }
        let shared: Option<Vec<u8>> = if plan.has_shared() {
            Some(render_buffer(&graph, &plan, Home::Shared, sets.as_deref())?)
        } else {
            None
        };
        let per_schema: Vec<Vec<u8>> = (0..schemas.len())
            .map(|index| render_buffer(&graph, &plan, Home::Schema(index), sets.as_deref()))
            .collect::<CodeGenResult<Vec<Vec<u8>>>>()?;
        let root_types: Vec<String> = (0..schemas.len())
            .map(|index| plan.root_type(&graph, index))
            .collect();
        Ok(GenerateRustOutput {
            shared,
            per_schema,
            root_types,
        })
    }
}

/// The type graph of `schemas` (one registry each) after every pass, the embedded documents of
/// each input when types validate, and per node the node it was merged into (its own id when it
/// was not merged; empty without deduplication).
pub(crate) fn prepare_graph(
    schemas: &[&Schema],
    registries: &[Registry<'_>],
    retrieval_settings: &JsonSchemaSettings,
    settings: &CodeGenSettings,
) -> CodeGenResult<(TypeGraph, Option<Vec<EmbeddedSet>>, Vec<TypeId>)> {
    let raw: type_graph::RawSubschemas =
        type_graph::RawSubschemas::collect(schemas, registries, settings.honours_rust_types())?;
    let (mut graph, sources): (TypeGraph, TargetSources<'_>) =
        type_graph::build(schemas, registries, &raw, settings)?;
    passes::collapse_wrapper_cycles(&mut graph);
    passes::order_variants(&mut graph);
    let sets: Option<Vec<EmbeddedSet>> = if settings.validates() {
        Some(rust_validators::embedded_sets(
            registries,
            retrieval_settings,
        )?)
    } else {
        None
    };
    let local: Vec<u32> = passes::local_classes(&graph, settings.dedupe_mode);
    let validation_keys: Vec<Option<String>> = match &sets {
        Some(sets) if !local.is_empty() => {
            let candidates: Vec<bool> = passes::merge_candidates(&graph, &local);
            rust_validators::validation_keys(&graph, registries, &sources, sets, &candidates)?
        }
        _ => Vec::new(),
    };
    let merged: Vec<TypeId> = passes::dedup(&mut graph, &local, &validation_keys);
    passes::break_cycles(&mut graph);
    passes::box_large_values(&mut graph);
    Ok((graph, sets, merged))
}

/// Generate Rust source from one or more parsed schemas: a slice of schemas, or a
/// [`CodeGenInput`] with retrieval URIs and a retriever for cross-document `$ref`s.
///
/// Callers pass `settings` (use [`CodeGenSettings::builder`] and call
/// [`CodeGenSettingsBuilder::build`](super::CodeGenSettingsBuilder::build) for all-default
/// settings). Returns [`GenerateRustOutput`] with an optional shared buffer and one buffer per
/// schema. Per-schema buffers import shared items as `super::shared::Name`, so the shared module
/// must be a sibling module named `shared` (or be re-exported as `shared` in the parent).
///
/// # Errors
///
/// Returns [`CodeGenError::Batch`] with the schema's index when one schema fails (an
/// unresolvable `$ref`, a document that cannot be retrieved, a registry problem), and
/// [`CodeGenError::Io`] on write failure.
pub fn generate_rust<'a>(
    input: impl Into<CodeGenInput<'a>>,
    settings: &CodeGenSettings,
) -> CodeGenResult<GenerateRustOutput> {
    RustBackend.generate(&input.into(), settings)
}

/// Which output buffer an item goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Home {
    Shared,
    Schema(usize),
}

/// Names the Rust prelude, std collections or the generated code's imports already use: an
/// item with one of these names would shadow it.
const RESERVED_NAMES: [&str; 18] = [
    "BTreeMap",
    "BTreeSet",
    "Box",
    "Deserialize",
    "Err",
    "HashMap",
    "HashSet",
    "IndexMap",
    "IndexSet",
    "None",
    "Ok",
    "Option",
    "Result",
    "Serialize",
    "Some",
    "String",
    "Uuid",
    "Vec",
];

/// Each named item's home and name, and each patch, keyed the way [`Plan`] keeps them.
struct Placement {
    items: HashMap<TypeId, (Home, String)>,
    root_aliases: Vec<Option<String>>,
    node_patches: HashMap<TypeId, TypePatch>,
    alias_patches: Vec<Option<TypePatch>>,
}

impl Placement {
    /// Places each candidate under its allocated name, with its patch (`names` and `patched`
    /// are in `candidates`' order).
    fn new(
        schemas: usize,
        candidates: Vec<(NameProposal, ItemKey, Home)>,
        names: Vec<String>,
        patched: &[Option<&TypePatch>],
    ) -> Placement {
        let mut placed: Placement = Placement {
            items: HashMap::new(),
            root_aliases: vec![None; schemas],
            node_patches: HashMap::new(),
            alias_patches: vec![None; schemas],
        };
        for (((_, key, home), name), patch) in candidates.into_iter().zip(names).zip(patched) {
            match key {
                ItemKey::Node(id) => {
                    placed.items.insert(id, (home, name));
                    if let Some(patch) = patch {
                        placed.node_patches.insert(id, (*patch).clone());
                    }
                }
                ItemKey::RootAlias(schema) => {
                    placed.root_aliases[schema] = Some(name);
                    placed.alias_patches[schema] = patch.cloned();
                }
            }
        }
        placed
    }
}

/// An item that needs a name: a named node, or the type alias for a schema's root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ItemKey {
    Node(TypeId),
    RootAlias(usize),
}

/// Where every item goes and what it is called.
#[derive(Debug)]
struct Plan {
    /// Named node → (home, name).
    items: HashMap<TypeId, (Home, String)>,
    /// Per schema, the name of the type alias for a root that is not a named item.
    root_aliases: Vec<Option<String>>,
    /// The set type for `uniqueItems` arrays, `None` for `Vec`.
    set_kind: Option<SetKind>,
    /// The map type's name and `use` path for objects' string-keyed entries (`MapType`).
    map: (&'static str, &'static str),
    /// Per node, whether it can be a set item (all `false` without a set type).
    set_item: Vec<bool>,
    /// The structs and unions that derive `Hash`, `Eq` and `Ord`.
    ord_items: BTreeSet<TypeId>,
    /// Per node, whether its Rust type can derive `Eq`: it reaches no float.
    eq_nodes: Vec<bool>,
    /// Whether structs, enums and unions validate themselves (`self_validating`).
    self_validating: bool,
    /// Whether every struct gets a builder (`builders`).
    builders: bool,
    /// Whether string enums get `as_str`, `Display` and `FromStr` (`string_enum_impls`).
    string_enum_impls: bool,
    /// The visibility of every item, field and re-export (`visibility`): `pub`, `pub(crate)`.
    vis: &'static str,
    /// The path a per-schema buffer reaches the shared items through (`shared_path`).
    shared_path: String,
    /// The text written as `//` comments at the top of every buffer (`header`).
    header: Option<String>,
    /// The path the generated code reaches the library through (`crate_path`).
    runtime: String,
    /// The path the generated code reaches the derive macros through (`macro_crate_path`).
    macros: String,
    /// The `#[json_schema(crate = ...)]` attribute every derived item carries when the library
    /// is reached through another path, so the derives find it there too.
    crate_attr: Option<String>,
    /// Per item with an `impl Default` from its schema `default` (`schema_defaults`), the
    /// default as compact JSON.
    defaults: HashMap<TypeId, String>,
    /// The settings' derives and attributes for every item (`extra_derives`, `extra_attrs`).
    extra_derives: Vec<String>,
    extra_attrs: Vec<String>,
    /// The patch of each patched named node, and of each schema's patched root alias.
    node_patches: HashMap<TypeId, TypePatch>,
    alias_patches: Vec<Option<TypePatch>>,
    /// Per conversion in the settings, the name of its schema marker in `converted_schemas` and
    /// the schema it stands for, as compact JSON (see [`render_converted_schemas`]).
    converted: Vec<(String, String)>,
}

impl Plan {
    /// The name of schema `schema`'s root type: its root alias, else the named item its root
    /// peels to.
    fn root_type(&self, graph: &TypeGraph, schema: usize) -> String {
        self.root_aliases[schema].clone().unwrap_or_else(|| {
            let root: TypeId = graph.peel(graph.schemas[schema].root);
            self.items
                .get(&root)
                .map(|(_, name)| name.clone())
                .expect("a root without an alias is a named item")
        })
    }

    fn new(graph: &mut TypeGraph, settings: &CodeGenSettings) -> CodeGenResult<Plan> {
        let (users, shared, uses_shared) = users_and_shared(graph, settings.dedupe_mode);
        let home_of = |id: TypeId| -> Home {
            if shared.contains(&id) {
                Home::Shared
            } else {
                Home::Schema(*users[&id].first().expect("a named node has a user"))
            }
        };
        // Every item that wants a name: (best proposal, key, home).
        let mut candidates: Vec<Candidate> = Vec::new();
        for &id in users.keys() {
            candidates.push((best_proposal(graph, id), ItemKey::Node(id), home_of(id)));
        }
        for (schema, roots) in graph.schemas.iter().enumerate() {
            if !graph.is_named(graph.peel(roots.root)) {
                candidates.push((
                    root_alias_proposal(graph, schema, roots.root),
                    ItemKey::RootAlias(schema),
                    Home::Schema(schema),
                ));
            }
        }
        candidates.sort();
        let wanted: Vec<(String, Home)> = candidates
            .iter()
            .map(|(proposal, key, home)| {
                let base: String = match key {
                    ItemKey::Node(id) => item_base_name(graph, *id, proposal),
                    ItemKey::RootAlias(_) if proposal.tier == NameTier::Given => {
                        proposal.text.clone()
                    }
                    ItemKey::RootAlias(_) => sanitize_struct_name(&proposal.text),
                };
                (base, *home)
            })
            .collect();
        // Two items clash when they share a module, or one is shared and the other's schema
        // uses the shared module.
        let clash = |a: Home, b: Home| -> bool {
            match (a, b) {
                (Home::Shared, Home::Shared) => true,
                (Home::Schema(x), Home::Schema(y)) => x == y,
                (Home::Shared, Home::Schema(s)) | (Home::Schema(s), Home::Shared) => uses_shared[s],
            }
        };
        let wanted: Vec<(String, Home)> = qualify_collisions(&wanted, &candidates, &clash);
        let given: Vec<bool> = candidates
            .iter()
            .map(|(proposal, _, _)| proposal.tier == NameTier::Given)
            .collect();
        let names: Vec<String> = allocate_names(&wanted, &given, clash);
        let (candidates, mut names): (Vec<Candidate>, Vec<String>) =
            if settings.replacements.is_empty() {
                (candidates, names)
            } else {
                apply_replacements(graph, settings, candidates, names)?
            };
        let graph: &TypeGraph = graph;
        let homes: Vec<Home> = candidates.iter().map(|(_, _, home)| *home).collect();
        let patched: Vec<Option<&TypePatch>> = apply_patches(settings, &homes, &mut names, clash)?;
        let placed: Placement = Placement::new(graph.schemas.len(), candidates, names, &patched);
        let set_kind: Option<SetKind> = SetKind::from_setting(settings.unique_items);
        let set_item: Vec<bool> = if set_kind.is_some() {
            set_item_nodes(graph, settings.map_type)
        } else {
            vec![false; graph.nodes.len()]
        };
        let ord_items: BTreeSet<TypeId> = ord_items(graph, &set_item);
        let eq_nodes: Vec<bool> = eq_nodes(graph);
        let (runtime, macros, crate_attr) = crate_paths(settings);
        Ok(Plan {
            items: placed.items,
            root_aliases: placed.root_aliases,
            set_kind,
            map: map_names(settings.map_type),
            set_item,
            ord_items,
            eq_nodes,
            self_validating: settings.self_validating,
            builders: settings.builders,
            string_enum_impls: settings.string_enum_impls,
            vis: settings.visibility.keyword(),
            shared_path: settings
                .shared_path
                .clone()
                .unwrap_or_else(|| "super::shared".to_string()),
            header: settings.header.clone(),
            runtime,
            macros,
            crate_attr,
            defaults: HashMap::new(),
            extra_derives: settings.extra_derives.clone(),
            extra_attrs: settings.extra_attrs.clone(),
            node_patches: placed.node_patches,
            alias_patches: placed.alias_patches,
            converted: converted_markers(settings),
        })
    }

    fn has_shared(&self) -> bool {
        self.items.values().any(|(home, _)| *home == Home::Shared)
    }

    fn name(&self, id: TypeId) -> &str {
        &self.items[&id].1
    }

    fn home(&self, id: TypeId) -> Home {
        self.items[&id].0
    }
}

/// The named items in the shared module: those two or more schemas use (`users`), and every
/// named item such an item reaches.
fn shared_items(graph: &TypeGraph, users: &BTreeMap<TypeId, BTreeSet<usize>>) -> BTreeSet<TypeId> {
    let mut shared: BTreeSet<TypeId> = BTreeSet::new();
    let mut stack: Vec<TypeId> = users
        .iter()
        .filter(|(_, s)| s.len() > 1)
        .map(|(id, _)| *id)
        .collect();
    let mut seen: BTreeSet<TypeId> = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if graph.is_named(id) {
            shared.insert(id);
        }
        stack.extend(graph.node(id).children());
    }
    shared
}

/// An item that wants a name: its best proposal, its key and its module.
type Candidate = (NameProposal, ItemKey, Home);

/// Applies `settings.replacements` to the allocated `names` of `candidates`: each replaced
/// node becomes the user's type, and the items only it reached are dropped, while every other
/// item keeps its name. Every problem is collected: a key that names no item or a schema's root
/// type, and a replacement that is not a Rust type.
fn apply_replacements(
    graph: &mut TypeGraph,
    settings: &CodeGenSettings,
    candidates: Vec<Candidate>,
    names: Vec<String>,
) -> CodeGenResult<(Vec<Candidate>, Vec<String>)> {
    let mut problems: Vec<String> = Vec::new();
    let mut replaced: Vec<(TypeId, &str)> = Vec::new();
    for (key, rust_type) in &settings.replacements {
        if !is_rust_type(rust_type) {
            problems.push(format!(
                "replacement for `{key}`: `{rust_type}` is not a Rust type"
            ));
        }
        let Some(index) = names.iter().position(|name: &String| name == key) else {
            problems.push(format!(
                "replacement for `{key}`: no generated type has that name"
            ));
            continue;
        };
        match candidates[index].1 {
            ItemKey::Node(id)
                if !graph
                    .schemas
                    .iter()
                    .any(|roots| graph.peel(roots.root) == id) =>
            {
                replaced.push((id, rust_type));
            }
            _ => problems.push(format!(
                "replacement for `{key}`: a schema's root type cannot be replaced"
            )),
        }
    }
    if !problems.is_empty() {
        return Err(CodeGenError::Customization(problems));
    }
    for (id, rust_type) in replaced {
        graph.nodes[id.index()] = TypeNode::External {
            path: rust_type.to_string(),
            conversion: None,
            parameters: Vec::new(),
        };
        graph.newtypes[id.index()] = None;
    }
    let reach: Vec<BTreeSet<TypeId>> = passes::reachable(graph);
    let kept = |key: &ItemKey| -> bool {
        match key {
            ItemKey::Node(id) => graph.is_named(*id) && reach.iter().any(|ids| ids.contains(id)),
            ItemKey::RootAlias(_) => true,
        }
    };
    Ok(candidates
        .into_iter()
        .zip(names)
        .filter(|((_, key, _), _)| kept(key))
        .unzip())
}

/// Which schemas use each named node, the nodes the shared module holds, and whether each
/// schema uses the shared module.
fn users_and_shared(
    graph: &TypeGraph,
    dedupe_mode: DedupeMode,
) -> (
    BTreeMap<TypeId, BTreeSet<usize>>,
    BTreeSet<TypeId>,
    Vec<bool>,
) {
    let reach: Vec<BTreeSet<TypeId>> = passes::reachable(graph);
    let mut users: BTreeMap<TypeId, BTreeSet<usize>> = BTreeMap::new();
    for (schema, ids) in reach.iter().enumerate() {
        for &id in ids {
            if graph.is_named(id) {
                users.entry(id).or_default().insert(schema);
            }
        }
    }
    let shared: BTreeSet<TypeId> = if dedupe_mode == DedupeMode::Disabled {
        BTreeSet::new()
    } else {
        shared_items(graph, &users)
    };
    let uses_shared: Vec<bool> = reach
        .iter()
        .map(|ids| ids.iter().any(|id| shared.contains(id)))
        .collect();
    (users, shared, uses_shared)
}

/// Applies `settings.patches` to the allocated `names` (one per item, in `homes`): renames each
/// patched item and returns each item's patch. Every problem is collected: a patch whose key
/// names no item, a new name that is not a Rust identifier or that another item it `clash`es
/// with has, and an extra derive or attribute (of the settings or a patch) that is malformed.
fn apply_patches<'s>(
    settings: &'s CodeGenSettings,
    homes: &[Home],
    names: &mut [String],
    clash: impl Fn(Home, Home) -> bool,
) -> CodeGenResult<Vec<Option<&'s TypePatch>>> {
    let mut problems: Vec<String> = Vec::new();
    let check_derives = |derives: &[String], problems: &mut Vec<String>| {
        for derive in derives {
            if !is_rust_path(derive) {
                problems.push(format!("derive `{derive}` is not a Rust path"));
            }
        }
    };
    let check_attrs = |attrs: &[String], problems: &mut Vec<String>| {
        for attr in attrs {
            let attr: &str = attr.trim();
            if attr.is_empty() || attr.starts_with('#') {
                problems.push(format!(
                    "attribute `{attr}` is not the text inside `#[...]`"
                ));
            }
        }
    };
    check_derives(&settings.extra_derives, &mut problems);
    check_attrs(&settings.extra_attrs, &mut problems);
    if let Some(ref name) = settings.root_name {
        if !is_rust_identifier(name) {
            problems.push(format!("root name `{name}` is not a Rust identifier"));
        } else if RESERVED_NAMES.contains(&name.as_str()) {
            problems.push(format!(
                "root name `{name}` is a type the generated code uses"
            ));
        }
    }
    if let Some(ref path) = settings.shared_path
        && !is_rust_path(path)
    {
        problems.push(format!("shared path `{path}` is not a Rust path"));
    }
    for (what, path) in [
        ("crate path", &settings.crate_path),
        ("macro crate path", &settings.macro_crate_path),
    ] {
        if let Some(path) = path {
            if !is_rust_path(path) {
                problems.push(format!("{what} `{path}` is not a Rust path"));
            } else if matches!(first_segment(path), "self" | "super") {
                problems.push(format!(
                    "{what} `{path}` must name a crate: an extern crate path (`my_sdk::jsr`, `::my_sdk`) or a `crate::` path"
                ));
            }
        }
    }
    let patched: Vec<Option<&TypePatch>> = names
        .iter()
        .map(|name: &String| settings.patches.get(name))
        .collect();
    let mut renamed: Vec<usize> = Vec::new();
    for (key, patch) in &settings.patches {
        if !names.iter().any(|name: &String| name == key) {
            problems.push(format!(
                "patch for `{key}`: no generated type has that name"
            ));
        }
        check_derives(&patch.derives, &mut problems);
        check_attrs(&patch.attrs, &mut problems);
        if let Some(ref rename) = patch.rename
            && !is_rust_identifier(rename)
        {
            problems.push(format!(
                "patch for `{key}`: `{rename}` is not a Rust identifier"
            ));
        }
    }
    for (index, patch) in patched.iter().enumerate() {
        if let Some(rename) = patch.and_then(|p: &TypePatch| p.rename.as_ref()) {
            names[index].clone_from(rename);
            renamed.push(index);
        }
    }
    for &index in &renamed {
        let clashes: bool = names
            .iter()
            .enumerate()
            .any(|(other, name): (usize, &String)| {
                other != index && *name == names[index] && clash(homes[index], homes[other])
            });
        if clashes {
            problems.push(format!(
                "a patch renames a type to `{}`, which another generated type in its module has",
                names[index]
            ));
        }
    }
    if problems.is_empty() {
        Ok(patched)
    } else {
        problems.dedup();
        Err(CodeGenError::Customization(problems))
    }
}

/// Whether `text` is a Rust identifier (not a keyword) that a type can be named.
fn is_rust_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c: char| c.is_alphabetic() || c == '_')
        && chars.all(|c: char| c.is_alphanumeric() || c == '_')
        && text != "_"
        && !is_rust_keyword(text)
}

/// Whether `text` is made of identifier characters (a keyword included).
fn is_identifier_text(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c: char| c.is_alphabetic() || c == '_')
        && chars.all(|c: char| c.is_alphanumeric() || c == '_')
}

/// The first segment of a path (empty for a path that starts with `::`).
fn first_segment(text: &str) -> &str {
    text.split("::").next().unwrap_or(text)
}

/// Whether `text` is a Rust path (`Hash`, `serde::Serialize`, `::schemars::JsonSchema`). No
/// segment is a keyword, except that `crate`, `self` and `super` may start a path without a
/// leading `::`.
fn is_rust_path(text: &str) -> bool {
    let (path, rooted): (&str, bool) = match text.strip_prefix("::") {
        Some(rest) => (rest, true),
        None => (text, false),
    };
    path.split("::")
        .enumerate()
        .all(|(index, segment): (usize, &str)| {
            is_identifier_text(segment)
                && (!is_rust_keyword(segment)
                    || (index == 0 && !rooted && matches!(segment, "crate" | "self" | "super")))
        })
}

/// Whether `text` is a Rust type that is a path, each segment optionally with generic arguments
/// that are such types themselves or lifetimes (`rust_decimal::Decimal`,
/// `my::Wrapper<String, u8>`, `Cow<'static, str>`).
pub(crate) fn is_rust_type(text: &str) -> bool {
    // Split at the delimiters: every piece before a `<`, `,` or the end of a list is a path,
    // and only whitespace may follow a `>`.
    let mut depth: usize = 0;
    let mut piece: String = String::new();
    let mut closed: bool = false;
    for c in text.chars() {
        match c {
            '<' | ',' | '>' => {
                let path_ok: bool = if closed {
                    piece.trim().is_empty() && c != '<'
                } else if let Some(lifetime) = piece.trim().strip_prefix('\'') {
                    depth > 0 && c != '<' && is_identifier_text(lifetime)
                } else {
                    is_rust_path(piece.trim())
                };
                if !path_ok || (c != '<' && depth == 0) {
                    return false;
                }
                piece.clear();
                match c {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                closed = c == '>';
            }
            _ => piece.push(c),
        }
    }
    depth == 0
        && if closed {
            piece.trim().is_empty()
        } else {
            is_rust_path(piece.trim())
        }
}

/// The best name proposal for `id`: lowest tier, then lowest origin. A node without proposals
/// is `Unnamed`.
fn best_proposal(graph: &TypeGraph, id: TypeId) -> NameProposal {
    graph.proposals[id.index()]
        .iter()
        // A title like `"!!"` sanitizes to nothing: fall back to the next proposal.
        .filter(|p| p.text.chars().any(char::is_alphanumeric))
        .min()
        .cloned()
        .unwrap_or(NameProposal {
            tier: NameTier::Fallback,
            origin: (usize::MAX, String::new()),
            text: "Unnamed".to_string(),
            qualifier: None,
        })
}

/// The name proposal for schema `schema`'s root type alias: only proposals made at that root
/// (its title, `Root`) count, since an unnamed root node (any value, a scalar) can be shared
/// with other positions and schemas.
fn root_alias_proposal(graph: &TypeGraph, schema: usize, root: TypeId) -> NameProposal {
    graph.proposals[root.index()]
        .iter()
        .filter(|p| p.origin.0 == schema && p.origin.1.is_empty())
        .filter(|p| p.text.chars().any(char::is_alphanumeric))
        .min()
        .cloned()
        .unwrap_or(NameProposal {
            tier: NameTier::Root,
            origin: (schema, String::new()),
            text: "Root".to_string(),
            qualifier: None,
        })
}

/// The base name of a named node from its best proposal: a union reached through a property or
/// title gets an `AnyOf` / `OneOf` suffix; a `$defs` key is used as written, and a given root
/// name exactly as written.
fn item_base_name(graph: &TypeGraph, id: TypeId, proposal: &NameProposal) -> String {
    if proposal.tier == NameTier::Given {
        return proposal.text.clone();
    }
    let base: String = sanitize_struct_name(&proposal.text);
    match (graph.node(id), proposal.tier) {
        (TypeNode::Union(u), tier) if tier != NameTier::Definition => match u.kind {
            UnionKind::AnyOf => format!("{base}AnyOf"),
            UnionKind::OneOf => format!("{base}OneOf"),
            UnionKind::TypeArray => base,
        },
        _ => base,
    }
}

/// `wanted` with every base name that collides (two or more items that `clash` want it, or it is
/// reserved)
/// qualified by each item's parent (`proposal.qualifier`): `type` in `GeoJSON Point` and in
/// `GeoJSON Polygon` become `GeoJsonPointType` and `GeoJsonPolygonType`, so only names that
/// still collide get numbered ([`allocate_names`]). An item without a parent, or whose
/// qualified name another item of the group shares, keeps its base.
fn qualify_collisions<K>(
    wanted: &[(String, Home)],
    candidates: &[(NameProposal, K, Home)],
    clash: &impl Fn(Home, Home) -> bool,
) -> Vec<(String, Home)> {
    let mut by_base: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (base, _)) in wanted.iter().enumerate() {
        by_base.entry(base.as_str()).or_default().push(i);
    }
    let mut out: Vec<(String, Home)> = wanted.to_vec();
    for members in by_base.values() {
        let mut qualified: Vec<(usize, String)> = Vec::new();
        for &i in members {
            let base: &str = wanted[i].0.as_str();
            let collides: bool = RESERVED_NAMES.contains(&base)
                || members
                    .iter()
                    .any(|&j| j != i && clash(wanted[i].1, wanted[j].1));
            if !collides {
                continue;
            }
            if let Some(parent) = &candidates[i].0.qualifier {
                let parent: String = sanitize_struct_name(parent);
                // A base that already names its parent (`ExecutorsValueVariant0` under
                // `ExecutorsValue`) or a parent ending in the base (`HomeAddress` for `Address`)
                // gains nothing from the prefix.
                if !base.starts_with(parent.as_str()) && !parent.ends_with(base) {
                    qualified.push((i, format!("{parent}{base}")));
                }
            }
        }
        // Qualifying helps only where it tells an item apart: one whose qualified name another
        // item of the group shares (two `Item`s under one parent) keeps its base and is
        // numbered.
        for (i, name) in &qualified {
            let shared: bool = qualified
                .iter()
                .any(|(j, other)| j != i && other == name && clash(wanted[*i].1, wanted[*j].1));
            if !shared {
                out[*i].0.clone_from(name);
            }
        }
    }
    out
}

/// Final names for `wanted` items (base name and home, in allocation order). Items whose base
/// names are equal and that `clash` (would be visible in one module) form a group; when a group
/// has two or more items, or its name is reserved, every item in it gets a numeric suffix
/// (`Name0`, `Name1`) in allocation order. A suffixed name already used where the item is
/// visible moves to the next number. A `given` item (a root named by `root_name`) keeps its name
/// while it is the only given one in its group, and the others are numbered around it.
fn allocate_names(
    wanted: &[(String, Home)],
    given: &[bool],
    clash: impl Fn(Home, Home) -> bool,
) -> Vec<String> {
    let n: usize = wanted.len();
    // Union-find over items with the same base that clash.
    let mut parent: Vec<usize> = (0..n).collect();
    let mut by_base: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, (base, _)) in wanted.iter().enumerate() {
        by_base.entry(base.as_str()).or_default().push(i);
    }
    for members in by_base.values() {
        for (k, &i) in members.iter().enumerate() {
            for &j in &members[k + 1..] {
                if clash(wanted[i].1, wanted[j].1) {
                    let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                    if ri != rj {
                        parent[rj.max(ri)] = rj.min(ri);
                    }
                }
            }
        }
    }
    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut group_size: HashMap<usize, usize> = HashMap::new();
    let mut group_given: HashMap<usize, usize> = HashMap::new();
    for (root, is_given) in roots.iter().zip(given) {
        *group_size.entry(*root).or_insert(0) += 1;
        *group_given.entry(*root).or_insert(0) += usize::from(*is_given);
    }
    let suffixed: Vec<bool> = wanted
        .iter()
        .zip(&roots)
        .zip(given)
        .map(|(((base, _), root), is_given)| {
            if *is_given && group_given[root] == 1 {
                false
            } else {
                group_size[root] > 1 || RESERVED_NAMES.contains(&base.as_str())
            }
        })
        .collect();
    // Names in use per home: every unsuffixed base first, so a suffix never takes one.
    let mut taken: Vec<(Home, String)> = wanted
        .iter()
        .zip(&suffixed)
        .filter(|(_, s)| !**s)
        .map(|((base, home), _)| (*home, base.clone()))
        .collect();
    let mut next: HashMap<usize, usize> = HashMap::new();
    let mut out: Vec<String> = Vec::with_capacity(n);
    for (((base, home), root), is_suffixed) in wanted.iter().zip(&roots).zip(&suffixed) {
        let home: Home = *home;
        if !is_suffixed {
            out.push(base.clone());
            continue;
        }
        let counter: &mut usize = next.entry(*root).or_insert(0);
        loop {
            let candidate: String = format!("{base}{counter}");
            *counter += 1;
            let used: bool = taken
                .iter()
                .any(|(h, name)| *name == candidate && clash(*h, home));
            if !used {
                taken.push((home, candidate.clone()));
                out.push(candidate);
                break;
            }
        }
    }
    out
}

/// The representative of `i`'s union-find set (with path halving).
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Final names for a list of identifiers that must be unique among themselves (fields of one
/// struct, variants of one enum): every colliding one gets a numeric suffix, in order.
pub(super) fn unique_idents(bases: &[String], separator: &str) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for base in bases {
        *counts.entry(base.as_str()).or_insert(0) += 1;
    }
    let mut taken: BTreeSet<String> = bases
        .iter()
        .filter(|b| counts[b.as_str()] == 1)
        .cloned()
        .collect();
    let mut next: HashMap<&str, usize> = HashMap::new();
    bases
        .iter()
        .map(|base| {
            if counts[base.as_str()] == 1 {
                return base.clone();
            }
            let counter: &mut usize = next.entry(base.as_str()).or_insert(0);
            loop {
                let candidate: String = format!("{base}{separator}{counter}");
                *counter += 1;
                if taken.insert(candidate.clone()) {
                    return candidate;
                }
            }
        })
        .collect()
}

/// What a buffer's rendered code needs imported.
#[derive(Debug, Default)]
struct Imports {
    /// `use` paths (sorted).
    uses: BTreeSet<&'static str>,
    /// Shared items referenced from a per-schema buffer.
    shared: BTreeSet<String>,
    /// The `deserialize_some` helper is needed.
    deserialize_some: bool,
    /// The code names `serde::Deserialize` (a derive, an impl or a bound), so it is imported.
    deserialize: bool,
    /// The set types `uniqueItems` fields use (each gets a `UniqueSet` impl).
    unique_sets: BTreeSet<SetKind>,
    /// The `unique_items` deserialize functions fields use.
    unique_shapes: BTreeSet<FieldShape>,
    /// The `unique_items` deserialize functions for integer items fields use.
    unique_integer_shapes: BTreeSet<FieldShape>,
    /// The conversions whose schema markers the buffer names (indices into the settings).
    converted: BTreeSet<usize>,
    /// What a named item's name is written after: `super::` from the `builder` submodule.
    item_prefix: &'static str,
    /// The builders of the buffer's structs, and the `use` paths their field types need (see
    /// [`render_builder`]).
    builders: String,
    builder_uses: BTreeSet<&'static str>,
}

/// A set type a `uniqueItems` field can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SetKind {
    BTree,
    Hash,
    #[cfg(feature = "indexmap")]
    Index,
}

impl SetKind {
    /// The set type for `setting`, `None` for `Vec`.
    fn from_setting(setting: UniqueItemsType) -> Option<SetKind> {
        match setting {
            UniqueItemsType::Vec => None,
            UniqueItemsType::HashSet => Some(SetKind::Hash),
            UniqueItemsType::BTreeSet => Some(SetKind::BTree),
            #[cfg(feature = "indexmap")]
            UniqueItemsType::IndexSet => Some(SetKind::Index),
        }
    }

    /// The type name and the `use` path that brings it into scope.
    fn name_and_path(self) -> (&'static str, &'static str) {
        match self {
            SetKind::BTree => ("BTreeSet", "std::collections::BTreeSet"),
            SetKind::Hash => ("HashSet", "std::collections::HashSet"),
            #[cfg(feature = "indexmap")]
            SetKind::Index => ("IndexSet", "indexmap::IndexSet"),
        }
    }

    /// The `UniqueSet` impl for this set type in the `unique_items` helper module.
    fn helper_impl(self) -> &'static str {
        match self {
            SetKind::BTree => UNIQUE_SET_BTREE_SET,
            SetKind::Hash => UNIQUE_SET_HASH_SET,
            #[cfg(feature = "indexmap")]
            SetKind::Index => UNIQUE_SET_INDEX_SET,
        }
    }
}

/// Renders the Rust type of `id` as seen from `home`, recording imports.
fn rust_type(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    imports: &mut Imports,
) -> String {
    render_type(graph, plan, home, id, imports, false)
}

/// The type whose `ToJsonSchema` gives `id`'s schema, when it is not `id`'s own Rust type: `id`
/// reaches a converted type (outside named items), whose schema comes from its marker in
/// `converted_schemas`, as no impl of the crate's trait exists for the user's type. `None`
/// otherwise.
fn schema_type(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    imports: &mut Imports,
) -> Option<String> {
    let mut stack: Vec<TypeId> = vec![id];
    let mut converted: bool = false;
    while let Some(id) = stack.pop() {
        if plan.items.contains_key(&id) {
            continue;
        }
        let node: &TypeNode = graph.node(id);
        if let TypeNode::External {
            conversion: Some(_),
            ..
        } = node
        {
            converted = true;
            break;
        }
        stack.extend(node.children());
    }
    converted.then(|| render_type(graph, plan, home, id, imports, true))
}

/// Renders the Rust type of `id` as seen from `home`, recording imports; with `markers`, a
/// converted type as its schema marker (see [`schema_type`]). Iterative: a type is rendered as
/// a token stream of open / close parts so deep nesting cannot overflow.
fn render_type(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    imports: &mut Imports,
    markers: bool,
) -> String {
    enum Part {
        Node(TypeId),
        Text(&'static str),
    }
    let mut out: String = String::new();
    let mut stack: Vec<Part> = vec![Part::Node(id)];
    while let Some(part) = stack.pop() {
        let id: TypeId = match part {
            Part::Text(t) => {
                out.push_str(t);
                continue;
            }
            Part::Node(id) => id,
        };
        if let Some((item_home, name)) = plan.items.get(&id) {
            out.push_str(imports.item_prefix);
            out.push_str(name);
            if *item_home == Home::Shared && home != Home::Shared {
                imports.shared.insert(name.clone());
            }
            continue;
        }
        match graph.node(id) {
            TypeNode::Pending | TypeNode::Any => out.push_str("serde_json::Value"),
            TypeNode::Null => out.push_str("()"),
            node @ (TypeNode::Boolean
            | TypeNode::Integer(..)
            | TypeNode::Number
            | TypeNode::String { .. }) => out.push_str(scalar_type(node, imports)),
            TypeNode::Array { items, .. } => {
                out.push_str("Vec<");
                stack.push(Part::Text(">"));
                stack.push(Part::Node(*items));
            }
            TypeNode::Map { values } => {
                let (name, path) = plan.map;
                imports.uses.insert(path);
                out.push_str(name);
                out.push_str("<String, ");
                stack.push(Part::Text(">"));
                stack.push(Part::Node(*values));
            }
            TypeNode::Option(inner) => {
                out.push_str("Option<");
                stack.push(Part::Text(">"));
                stack.push(Part::Node(*inner));
            }
            TypeNode::Box(inner) => {
                out.push_str("Box<");
                stack.push(Part::Text(">"));
                stack.push(Part::Node(*inner));
            }
            TypeNode::External {
                conversion: Some(index),
                ..
            } if markers => {
                let name: &str = &plan.converted[*index].0;
                out.push_str("converted_schemas::");
                out.push_str(name);
                imports.converted.insert(*index);
            }
            TypeNode::External {
                path, parameters, ..
            } => {
                out.push_str(path);
                if let Some((last, rest)) = parameters.split_last() {
                    out.push('<');
                    stack.push(Part::Text(">"));
                    stack.push(Part::Node(*last));
                    for parameter in rest.iter().rev() {
                        stack.push(Part::Text(", "));
                        stack.push(Part::Node(*parameter));
                    }
                }
            }
            TypeNode::Object(_) | TypeNode::Enum(_) | TypeNode::Union(_) => {
                unreachable!("every reachable named node has a planned name")
            }
        }
    }
    out
}

/// The Rust type of a scalar node (boolean, integer, number, string).
fn scalar_type(node: &TypeNode, imports: &mut Imports) -> &'static str {
    match node {
        TypeNode::Integer(w, _) => int_name(*w),
        TypeNode::Number => "f64",
        TypeNode::String { format, .. } => string_type(*format, imports),
        _ => "bool",
    }
}

fn int_name(width: IntWidth) -> &'static str {
    match width {
        IntWidth::U8 => "u8",
        IntWidth::U16 => "u16",
        IntWidth::U32 => "u32",
        IntWidth::U64 => "u64",
        IntWidth::I8 => "i8",
        IntWidth::I16 => "i16",
        IntWidth::I32 => "i32",
        IntWidth::I64 => "i64",
    }
}

/// The Rust type for a string of `format` (see [`StringFormat::rust_type`]).
fn string_type(format: StringFormat, imports: &mut Imports) -> &'static str {
    if format == StringFormat::Uuid {
        imports.uses.insert("uuid::Uuid");
    }
    format.rust_type()
}

/// The item type of a `uniqueItems` array that is a field's own value (the field's type with
/// its nullable `Option` removed, as [`render_field`] does): the arrays a set can hold.
fn unique_field_items(graph: &TypeGraph, field: &Field) -> Option<TypeId> {
    let value_id: TypeId = match graph.node(field.ty) {
        TypeNode::Option(inner) => *inner,
        _ => field.ty,
    };
    match graph.node(value_id) {
        TypeNode::Array {
            items,
            unique: true,
            ..
        } => Some(*items),
        _ => None,
    }
}

/// Whether an enum holds a whole number (`1`, `1.0`, `1e2`): its variant matches a JSON number
/// in either representation, while an integer type accepts only the integer one.
fn has_whole_number(e: &EnumNode) -> bool {
    e.values.iter().any(|v| match v {
        Value::Number(n) => {
            n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        _ => false,
    })
}

/// The nodes that reach a node for which `start` holds (including those nodes): a reverse
/// breadth-first walk over `parents`.
fn reaching(
    graph: &TypeGraph,
    parents: &[Vec<TypeId>],
    start: impl Fn(&TypeNode) -> bool,
) -> Vec<bool> {
    let mut out: Vec<bool> = graph.nodes.iter().map(start).collect();
    let mut queue: Vec<usize> = (0..out.len()).filter(|&i| out[i]).collect();
    while let Some(i) = queue.pop() {
        for parent in &parents[i] {
            if !out[parent.index()] {
                out[parent.index()] = true;
                queue.push(parent.index());
            }
        }
    }
    out
}

/// Per node, whether its Rust type can be a set item under a set `UniqueItemsType`: the type
/// implements (or derives) `Hash`, `Eq` and `Ord`, and deserializing maps JSON-equal values to
/// equal Rust values and different ones to different values, so the set rejects exactly the
/// arrays `uniqueItems` rejects. The greatest such set: a node is out when it is blocked itself
/// or reaches a blocked node, so recursive types stay in.
///
/// Blocked: any value (`serde_json::Value`, no `Hash`), floats (no `Eq`), a user's type (its
/// traits and equality are unknown), a typed format
/// (parsing folds case and accepts several spellings, and `chrono` compares instants, so
/// different strings are equal values), a struct that
/// drops unknown keys (two objects differing only there would be equal) or keeps them as
/// `serde_json::Value`, a struct with a set field (sets ignore order, arrays do not), a map
/// (or a struct keeping unknown keys in one) of a `MapType` without `Hash` and `Ord`, and a
/// union where one branch reaches an integer type and another a whole number in an enum: `1`
/// and `1.0` are equal JSON, but the integer type rejects `1.0`, so they can deserialize to
/// different variants.
fn set_item_nodes(graph: &TypeGraph, map_type: MapType) -> Vec<bool> {
    let ordered_maps: bool = map_type == MapType::BTreeMap;
    let mut parents: Vec<Vec<TypeId>> = vec![Vec::new(); graph.nodes.len()];
    for (i, node) in graph.nodes.iter().enumerate() {
        let id: TypeId = TypeId(u32::try_from(i).expect("fewer than 2^32 nodes"));
        for child in node.children() {
            parents[child.index()].push(id);
        }
    }
    let integer: Vec<bool> = reaching(graph, &parents, |n| matches!(n, TypeNode::Integer(..)));
    let whole_enum: Vec<bool> = reaching(
        graph,
        &parents,
        |n| matches!(n, TypeNode::Enum(e) if has_whole_number(e)),
    );
    let blocked = |node: &TypeNode| -> bool {
        match node {
            TypeNode::Pending | TypeNode::Any | TypeNode::Number | TypeNode::External { .. } => {
                true
            }
            TypeNode::String { format, .. } => *format != StringFormat::Plain,
            TypeNode::Map { .. } => !ordered_maps,
            TypeNode::Object(o) => {
                (!o.deny_unknown && o.additional.is_none())
                    || (!ordered_maps && o.additional.is_some())
                    || o.fields
                        .iter()
                        .any(|f| unique_field_items(graph, f).is_some())
            }
            TypeNode::Union(u) => u.variants.iter().enumerate().any(|(i, a)| {
                integer[a.ty.index()]
                    && u.variants
                        .iter()
                        .enumerate()
                        .any(|(j, b)| i != j && whole_enum[b.ty.index()])
            }),
            _ => false,
        }
    };
    let unusable: Vec<bool> = reaching(graph, &parents, blocked);
    unusable.into_iter().map(|u| !u).collect()
}

/// The name and `use` path of a map type.
fn map_names(map_type: MapType) -> (&'static str, &'static str) {
    match map_type {
        MapType::BTreeMap => ("BTreeMap", "std::collections::BTreeMap"),
        MapType::HashMap => ("HashMap", "std::collections::HashMap"),
        #[cfg(feature = "indexmap")]
        MapType::IndexMap => ("IndexMap", "indexmap::IndexMap"),
    }
}

/// The structs and unions that derive `Hash`, `Eq` and `Ord`: those a set field's item type
/// reaches (a derive needs every field and variant type to implement the traits). Generated
/// enums always derive them.
fn ord_items(graph: &TypeGraph, set_item: &[bool]) -> BTreeSet<TypeId> {
    let mut stack: Vec<TypeId> = graph
        .nodes
        .iter()
        .filter_map(|node| match node {
            TypeNode::Object(o) => Some(o.fields.iter()),
            _ => None,
        })
        .flatten()
        .filter_map(|f| unique_field_items(graph, f))
        .filter(|items| set_item[items.index()])
        .collect();
    let mut seen: BTreeSet<TypeId> = BTreeSet::new();
    let mut out: BTreeSet<TypeId> = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if matches!(graph.node(id), TypeNode::Object(_) | TypeNode::Union(_)) {
            out.insert(id);
        }
        stack.extend(graph.node(id).children());
    }
    out
}

/// Per node, whether its Rust type implements `Eq`: every type a generated item can hold does
/// (`serde_json::Value` included) except floats and a user's type (which may not), so a node is
/// out when it reaches either.
fn eq_nodes(graph: &TypeGraph) -> Vec<bool> {
    let mut parents: Vec<Vec<TypeId>> = vec![Vec::new(); graph.nodes.len()];
    for (i, node) in graph.nodes.iter().enumerate() {
        let id: TypeId = TypeId(u32::try_from(i).expect("fewer than 2^32 nodes"));
        for child in node.children() {
            parents[child.index()].push(id);
        }
    }
    reaching(graph, &parents, |node: &TypeNode| {
        matches!(
            node,
            TypeNode::Pending | TypeNode::Number | TypeNode::External { .. }
        )
    })
    .into_iter()
    .map(|reaches_float: bool| !reaches_float)
    .collect()
}

/// Whether the Rust type for `id` can hold JSON `null` itself: any value, `()`, or a union
/// with such a branch.
fn accepts_null(graph: &TypeGraph, id: TypeId) -> bool {
    let mut stack: Vec<TypeId> = vec![id];
    let mut seen: BTreeSet<TypeId> = BTreeSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        match graph.node(id) {
            TypeNode::Any | TypeNode::Null | TypeNode::Option(_) => return true,
            TypeNode::Box(inner) => stack.push(*inner),
            TypeNode::Union(u) => stack.extend(u.variants.iter().map(|v| v.ty)),
            _ => {}
        }
    }
    false
}

/// An item's or field's doc comment lines (`""` for a blank line) from its description and
/// examples (see [`doc_text`]), and the description it must also state in
/// `#[json_schema(description = "...")]` (empty for none) when the lines do not read back as it.
fn annotation_docs(
    description: Option<&str>,
    examples: Option<&[Value]>,
) -> (Vec<String>, Option<String>) {
    let mut lines: Vec<String> = description.map_or_else(Vec::new, doc_text::description_lines);
    if let Some(examples) = examples.filter(|e| !e.is_empty()) {
        let section: Vec<String> = doc_text::examples_lines(examples);
        // The section's leading blank line separates it from a description.
        let skip: usize = usize::from(lines.is_empty());
        lines.extend(section.into_iter().skip(skip));
    }
    let wanted: Option<String> = description.and_then(doc_text::normalized);
    let stated: Option<String> =
        (doc_text::read_back(&lines) != wanted).then(|| wanted.unwrap_or_default());
    (lines, stated)
}

/// `lines` as `///` comments at `indent`.
fn doc_comment(lines: &[String], indent: &str) -> String {
    let mut out: String = String::new();
    for line in lines {
        if line.is_empty() {
            let _ = writeln!(out, "{indent}///");
        } else {
            let _ = writeln!(out, "{indent}/// {line}");
        }
    }
    out
}

/// `s` escaped for the inside of a Rust string literal: quotes, backslashes and every control
/// character (`\r`, `\u{1}`, ...) are escaped, so any JSON string is a valid literal.
fn escape(s: &str) -> String {
    let quoted: String = format!("{s:?}");
    quoted[1..quoted.len() - 1].to_string()
}

/// `value` as Rust tokens for `serde_json::json!` (and the derive's `value = ...`): strings and
/// keys as Rust string literals, numbers as written, built iteratively.
fn json_tokens(value: &Value) -> String {
    enum Part<'a> {
        Value(&'a Value),
        Text(String),
    }
    let mut out: String = String::new();
    let mut stack: Vec<Part<'_>> = vec![Part::Value(value)];
    while let Some(part) = stack.pop() {
        let value: &Value = match part {
            Part::Text(t) => {
                out.push_str(&t);
                continue;
            }
            Part::Value(v) => v,
        };
        match value {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(&b.to_string()),
            Value::Number(n) => out.push_str(&n.to_string()),
            Value::String(s) => {
                let _ = write!(out, "{s:?}");
            }
            Value::Array(items) => {
                out.push('[');
                stack.push(Part::Text("]".to_string()));
                for (i, item) in items.iter().enumerate().rev() {
                    stack.push(Part::Value(item));
                    if i > 0 {
                        stack.push(Part::Text(", ".to_string()));
                    }
                }
            }
            Value::Object(map) => {
                out.push('{');
                stack.push(Part::Text("}".to_string()));
                let entries: Vec<(&String, &Value)> = map.iter().collect();
                for (i, (key, item)) in entries.into_iter().enumerate().rev() {
                    stack.push(Part::Value(item));
                    stack.push(Part::Text(format!("{key:?}: ")));
                    if i > 0 {
                        stack.push(Part::Text(", ".to_string()));
                    }
                }
            }
        }
    }
    out
}

/// Whether the generated item for `id` (a struct or constrained newtype) is `#[deprecated]`.
fn is_deprecated_item(graph: &TypeGraph, id: TypeId) -> bool {
    let deprecated: Option<bool> = match graph.node(id) {
        TypeNode::Object(object) => object.annotations.deprecated,
        _ => graph.newtypes[id.index()]
            .as_deref()
            .and_then(|newtype: &Newtype| newtype.annotations.deprecated),
    };
    deprecated == Some(true)
}

/// Whether the Rust type of `id` names a `#[deprecated]` item of the generated code (a struct
/// or constrained newtype whose schema is `deprecated`), however deep inside `Vec`, `Option`,
/// a map or a generic: the place that names it carries `#[expect(deprecated)]`, as that mention
/// is generated code, never the consumer's own use. `owner` is the item the place sits inside:
/// rustc does not fire `deprecated` for a use within the deprecated item itself, so a mention of
/// the owner does not count (a self-referencing deprecated struct). Iterative, like [`render_type`].
fn names_deprecated_item(
    graph: &TypeGraph,
    plan: &Plan,
    id: TypeId,
    owner: Option<TypeId>,
) -> bool {
    let mut stack: Vec<TypeId> = vec![id];
    while let Some(id) = stack.pop() {
        if owner == Some(id) {
            continue;
        }
        if plan.items.contains_key(&id) {
            if is_deprecated_item(graph, id) {
                return true;
            }
            continue;
        }
        stack.extend(graph.node(id).children());
    }
    false
}

/// `#[expect(deprecated)]` for a place that names the type `id` when it names a deprecated item
/// (see [`names_deprecated_item`]), else nothing. Only an `expect`: the strictest generated-code
/// lint set forbids `allow`, and the expectation is always fulfilled where it is written.
fn expect_deprecated(
    graph: &TypeGraph,
    plan: &Plan,
    id: TypeId,
    owner: Option<TypeId>,
    indent: &str,
) -> String {
    if names_deprecated_item(graph, plan, id, owner) {
        format!("{indent}#[expect(deprecated)]\n")
    } else {
        String::new()
    }
}

/// `#[deprecated]` (or `#[deprecated = "<first description line>"]`) when `deprecated` is true.
fn deprecated_attr(deprecated: Option<bool>, description: Option<&str>, indent: &str) -> String {
    if deprecated != Some(true) {
        return String::new();
    }
    let message: Option<String> = description
        .map(str::trim)
        .and_then(|s| s.split('\n').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(escape);
    match message {
        Some(m) => format!("{indent}#[deprecated = \"{m}\"]\n"),
        None => format!("{indent}#[deprecated]\n"),
    }
}

/// Doc comments for an item from its annotations (description, then examples), and the
/// description it must also state in its `#[json_schema(...)]` (see [`annotation_docs`]).
fn item_docs(annotations: &Annotations) -> (String, Option<String>) {
    let (lines, stated): (Vec<String>, Option<String>) = annotation_docs(
        annotations.description.as_deref(),
        annotations.examples.as_deref(),
    );
    (doc_comment(&lines, ""), stated)
}

/// The `#[json_schema(description = "...")]` line for a stated description.
fn description_attr(stated: Option<&String>) -> String {
    stated.map_or_else(String::new, |d| {
        format!("#[json_schema(description = \"{}\")]\n", escape(d))
    })
}

/// A Rust literal for a scalar JSON default of the value type `node` (a string default is a
/// `&str` literal). `None` when the default does not fit the type (a string for a number, `300`
/// for a `u8`, a UUID string): the field then has no default accessor, rather than one that
/// does not compile.
fn default_literal(value: &Value, node: &TypeNode) -> Option<String> {
    match (node, value) {
        (TypeNode::Boolean, Value::Bool(b)) => Some(b.to_string()),
        (
            TypeNode::String {
                format: StringFormat::Plain,
                ..
            },
            Value::String(text),
        ) => Some(format!("\"{}\"", escape(text))),
        (TypeNode::Integer(width, _), Value::Number(n)) => {
            // An integer value (`3` or `3.0`): its ceiling and floor agree.
            let exact: i128 = number::integer_lower_bound(n, false)?;
            if number::integer_upper_bound(n, false)? != exact {
                return None;
            }
            let (lo, hi): (i128, i128) = width.range();
            (lo..=hi).contains(&exact).then(|| exact.to_string())
        }
        (TypeNode::Number, Value::Number(n)) => n.as_f64().map(|f: f64| format!("{f:?}")),
        _ => None,
    }
}

/// The `#[json_schema(default = ...)]` argument for a field's `default`, so the derive gives the
/// property schema its `default` back: a string, number or boolean, written as the JSON value
/// (`5` stays an integer, `2.5` a float). Other values (`null`, arrays, objects) have no
/// attribute form.
fn default_annotation(value: &Value) -> Option<String> {
    match value {
        Value::Bool(b) => Some(format!("default = {b}")),
        Value::Number(n) => Some(format!("default = {n}")),
        Value::String(text) => Some(format!("default = \"{}\"", escape(text))),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// The accessor for an optional field with a scalar `default`: the field's value, or the default
/// when the key is absent. The field itself keeps absence (`None`), so a deserialize →
/// serialize round trip writes back only the keys the instance had.
fn render_default_accessor(
    name: &str,
    shape: FieldShape,
    node: &TypeNode,
    value_ty: &str,
    literal: &str,
) -> Option<String> {
    let string: bool = matches!(node, TypeNode::String { .. });
    let (ret, body): (String, String) = match (shape, string) {
        (FieldShape::Optional, false) => (
            value_ty.to_string(),
            format!("self.{name}.unwrap_or({literal})"),
        ),
        (FieldShape::Optional, true) => (
            "&str".to_string(),
            format!("self.{name}.as_deref().unwrap_or({literal})"),
        ),
        (FieldShape::OptionalNullable, false) => (
            format!("Option<{value_ty}>"),
            format!("self.{name}.or_absent(Some({literal}))"),
        ),
        (FieldShape::OptionalNullable, true) => (
            "Option<&str>".to_string(),
            format!("self.{name}.as_deref().or_absent(Some({literal}))"),
        ),
        (FieldShape::Plain | FieldShape::RequiredNullable | FieldShape::OptionalHoldingNull, _) => {
            return None;
        }
    };
    Some(format!(
        "    /// The field's value, or its schema `default` when the key is absent.\n    #[must_use]\n    pub fn {name}_or_default(&self) -> {ret} {{\n        {body}\n    }}\n"
    ))
}

/// The default accessor for an optional newtype field: the newtype's inner value (`&str` for a
/// string), or the default when the key is absent. The default is the schema's annotation and
/// need not satisfy the newtype's constraints, so it is never turned into a newtype value.
/// With `deprecated` (the newtype is `#[deprecated]`) a body that names it expects the
/// deprecation: that mention is generated code's own.
fn render_newtype_default_accessor(
    name: &str,
    shape: FieldShape,
    node: &TypeNode,
    newtype: &str,
    literal: &str,
    deprecated: bool,
) -> Option<String> {
    let string: bool = matches!(node, TypeNode::String { .. });
    let inner: &str = scalar_type(node, &mut Imports::default());
    let (ret, body): (String, String) = match (shape, string) {
        (FieldShape::Optional, false) => (
            inner.to_string(),
            format!("self.{name}.map_or({literal}, {newtype}::into_inner)"),
        ),
        (FieldShape::Optional, true) => (
            "&str".to_string(),
            format!("self.{name}.as_deref().map_or({literal}, String::as_str)"),
        ),
        (FieldShape::OptionalNullable, false) => (
            format!("Option<{inner}>"),
            format!("self.{name}.map({newtype}::into_inner).or_absent(Some({literal}))"),
        ),
        (FieldShape::OptionalNullable, true) => (
            "Option<&str>".to_string(),
            format!("self.{name}.as_deref().map(String::as_str).or_absent(Some({literal}))"),
        ),
        (FieldShape::Plain | FieldShape::RequiredNullable | FieldShape::OptionalHoldingNull, _) => {
            return None;
        }
    };
    // Only a body naming the newtype (`{newtype}::into_inner`) fires `deprecated`: the string
    // arms read through `String::as_str` and name nothing.
    let expect: &str = if deprecated && !string {
        "    #[expect(deprecated)]\n"
    } else {
        ""
    };
    Some(format!(
        "    /// The field's value, or its schema `default` when the key is absent.\n    #[must_use]\n{expect}    pub fn {name}_or_default(&self) -> {ret} {{\n        {body}\n    }}\n"
    ))
}

/// The default accessor of the optional field `name` whose value type is `value_id` (rendered as
/// `value_ty`) and whose schema `default` is `value`, if it has one.
fn field_default_accessor(
    graph: &TypeGraph,
    value_id: TypeId,
    name: &str,
    shape: FieldShape,
    value_ty: &str,
    value: &Value,
) -> Option<String> {
    let node: &TypeNode = graph.node(value_id);
    let literal: String = default_literal(value, node)?;
    if graph.newtypes[value_id.index()].is_some() {
        let deprecated: bool = is_deprecated_item(graph, value_id);
        return render_newtype_default_accessor(name, shape, node, value_ty, &literal, deprecated);
    }
    render_default_accessor(name, shape, node, value_ty, &literal)
}

/// How a field holds its value: presence and nullability (absent and `null` are different).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FieldShape {
    /// Required, not nullable: `T`.
    Plain,
    /// Optional, not nullable: `Option<T>`, `None` when absent (and not serialized).
    Optional,
    /// Required, nullable: `Option<T>`, `None` for `null`; absence is an error.
    RequiredNullable,
    /// Optional, nullable: `json_schema_rs::Nullable<T>`: `Absent`, `Null` or `Value`.
    OptionalNullable,
    /// Optional, and the value type itself holds `null` (any value, a union with a `null`
    /// branch): `Option<T>`, `None` only when absent (present values go through
    /// `deserialize_some`).
    OptionalHoldingNull,
}

/// `skip_serializing_if` for a `Nullable` field: an absent key is not written.
struct SkipAbsent<'a>(&'a str);

impl fmt::Display for SkipAbsent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "skip_serializing_if = \"{}::Nullable::is_absent\"",
            self.0
        )
    }
}

/// One rendered field.
#[derive(Debug)]
struct RenderedField {
    name: String,
    ty: String,
    attrs: String,
    /// `pub fn <name>_or_default(&self) -> T { ... }` when the field has a default accessor.
    default_accessor: Option<String>,
    /// The field's kind and type in its struct's [`json_schema_rs::builder!`](crate::builder!):
    /// `required` (`ty`), `optional` (`ty` is `Option` of the type given, which the setter
    /// takes), or `defaulted` (absent is the type's default: the unknown-keys map, a
    /// `Nullable`).
    builder: (&'static str, String),
}

/// The `#[serde(rename_all = "...")]` rule for members named `(Rust name, serde name)`, if one
/// saves lines: the rule leaving the fewest members needing their own `rename` (the first in
/// serde's order on a tie), when that count plus the `rename_all` itself is below the renames
/// needed without a rule. `write` is the rule's conversion for the member kind
/// ([`RenameRule::write_field`] or [`RenameRule::write_variant`]).
///
/// Every struct and enum passes through here, so it stays cheap: a type with fewer than two
/// renames cannot gain and is skipped without converting a name, a rule stops counting once it
/// cannot win, and every conversion reuses one buffer.
fn rename_all_rule(
    members: &[(&str, &str)],
    write: fn(RenameRule, &str, &mut String),
) -> Option<RenameRule> {
    let without: usize = members.iter().filter(|(name, key)| name != key).count();
    if without < 2 {
        return None;
    }
    let mut buffer: String = String::new();
    // The best cost so far, in lines: own renames, plus one for the `rename_all`.
    let mut best: (usize, Option<RenameRule>) = (without, None);
    for rule in RenameRule::ALL {
        let mut cost: usize = 1;
        for (name, key) in members {
            write(rule, name, &mut buffer);
            if buffer != *key {
                cost += 1;
                if cost >= best.0 {
                    break;
                }
            }
        }
        if cost < best.0 {
            best = (cost, Some(rule));
        }
    }
    best.1
}

/// Whether a member named `name` needs its own `rename` to be read as `key`: its serde name
/// (`name` under `rule`, if any, converted by `write` into `buffer`) is not `key`.
fn needs_rename(
    rule: Option<RenameRule>,
    write: fn(RenameRule, &str, &mut String),
    name: &str,
    key: &str,
    buffer: &mut String,
) -> bool {
    match rule {
        None => name != key,
        Some(rule) => {
            write(rule, name, buffer);
            buffer != key
        }
    }
}

/// Renders the fields of `object`, with the container's `rename_all` rule (see
/// [`rename_all_rule`]); a field gets its own `rename` only when the rule does not give its key.
fn render_fields(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    owner: TypeId,
    object: &ObjectNode,
    imports: &mut Imports,
) -> (Vec<RenderedField>, Option<RenameRule>) {
    let mut bases: Vec<String> = object
        .fields
        .iter()
        .map(|f| sanitize_field_name(&f.json_key))
        .collect();
    if object.additional.is_some() {
        bases.push("additional".to_string());
    }
    let names: Vec<String> = unique_idents(&bases, "");
    let members: Vec<(&str, &str)> = object
        .fields
        .iter()
        .zip(&names)
        .map(|(field, name)| (name.as_str(), field.json_key.as_str()))
        .collect();
    let rule: Option<RenameRule> = rename_all_rule(&members, RenameRule::write_field);
    let mut buffer: String = String::new();
    let mut out: Vec<RenderedField> = Vec::new();
    for (field, name) in object.fields.iter().zip(&names) {
        let rename: bool = needs_rename(
            rule,
            RenameRule::write_field,
            name,
            &field.json_key,
            &mut buffer,
        );
        out.push(render_field(
            graph,
            plan,
            home,
            owner,
            field,
            (name, rename),
            imports,
        ));
    }
    if let Some(additional) = object.additional {
        let value: String = rust_type(graph, plan, home, additional, imports);
        let (map, path) = plan.map;
        imports.uses.insert(path);
        // The map's values read by the integer rule (see [`reads_integers`]): with `flatten`,
        // `json_schema_rs_macro::Deserialize` reads each value through the `deserialize_with`.
        let attrs: String = if reads_integers(graph, plan, additional) {
            format!(
                "    #[serde(flatten, deserialize_with = \"{}\")]\n",
                IntegerRead(&plan.runtime)
            )
        } else {
            "    #[serde(flatten)]\n".to_string()
        };
        let attrs: String = format!(
            "{}{attrs}",
            expect_deprecated(graph, plan, additional, Some(owner), "    ")
        );
        let attrs: String = match schema_type(graph, plan, home, additional, imports) {
            Some(schema) => {
                format!("{attrs}    #[json_schema(with = \"{map}<String, {schema}>\")]\n")
            }
            None => attrs,
        };
        let ty: String = format!("{map}<String, {value}>");
        out.push(RenderedField {
            name: names.last().expect("additional field name").clone(),
            builder: ("defaulted", ty.clone()),
            ty,
            attrs,
            default_accessor: None,
        });
    }
    (out, rule)
}

/// Whether a value of type `id` holds integers serde's own `Deserialize` would read: an integer
/// type of a `Numeric` dialect, directly or through `Option`, `Vec`, maps and `Box`. Such a value
/// reads through `json_schema_rs::integer`, which also accepts a number with a zero fraction
/// (`1.0`), as the dialect's integer rule does. A named item (struct, enum, union, newtype)
/// reads its own integers.
fn reads_integers(graph: &TypeGraph, plan: &Plan, id: TypeId) -> bool {
    let mut id: TypeId = id;
    let mut seen: HashSet<TypeId> = HashSet::new();
    while seen.insert(id) && !plan.items.contains_key(&id) {
        id = match graph.node(id) {
            TypeNode::Integer(_, IntegerSemantics::Numeric) => return true,
            TypeNode::Array { items, .. } => *items,
            TypeNode::Map { values } => *values,
            TypeNode::Option(inner) | TypeNode::Box(inner) => *inner,
            _ => return false,
        };
    }
    false
}

/// The path generated code reads an integer-holding value through (see [`reads_integers`]).
struct IntegerRead<'a>(&'a str);

impl fmt::Display for IntegerRead<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::integer::deserialize", self.0)
    }
}

/// The Rust type of a field's value (without the field's own `Option`s), and the set type when
/// it is a `uniqueItems` array held in a set (`UniqueItemsType`). A set applies only where the
/// field's own value is the array, since a duplicate is rejected through the field's
/// `deserialize_with`.
fn field_value_type(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    value_id: TypeId,
    imports: &mut Imports,
) -> (String, Option<SetKind>) {
    if let TypeNode::Array {
        items,
        unique: true,
        ..
    } = graph.node(value_id)
        && plan.set_item[items.index()]
        && let Some(kind) = plan.set_kind
    {
        let (set_name, path): (&str, &str) = kind.name_and_path();
        imports.uses.insert(path);
        imports.unique_sets.insert(kind);
        let item_ty: String = rust_type(graph, plan, home, *items, imports);
        return (format!("{set_name}<{item_ty}>"), Some(kind));
    }
    (rust_type(graph, plan, home, value_id, imports), None)
}

/// The serde arguments for a field's presence and nullability (`shape`), reading a set field
/// through the `unique_items` helper that rejects a duplicate item.
fn shape_serde_args(
    rt: &str,
    shape: FieldShape,
    set: Option<SetKind>,
    integers: bool,
    imports: &mut Imports,
    serde_args: &mut Vec<String>,
) {
    if integers {
        integer_serde_args(rt, shape, set, imports, serde_args);
        return;
    }
    if set.is_some() {
        imports.unique_shapes.insert(shape);
    }
    match (shape, set) {
        (FieldShape::Plain, Some(_)) => {
            serde_args.push("deserialize_with = \"unique_items::required\"".to_string());
        }
        (FieldShape::Optional, Some(_)) => {
            serde_args.push("default".to_string());
            serde_args.push("deserialize_with = \"unique_items::optional\"".to_string());
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
        (FieldShape::RequiredNullable, Some(_)) => {
            serde_args.push("deserialize_with = \"unique_items::nullable\"".to_string());
        }
        (FieldShape::OptionalNullable, Some(_)) => {
            serde_args.push("default".to_string());
            serde_args.push("deserialize_with = \"unique_items::optional_nullable\"".to_string());
            serde_args.push(SkipAbsent(rt).to_string());
        }
        (FieldShape::Plain, None) => {}
        (FieldShape::Optional, None) => {
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
        (FieldShape::RequiredNullable, None) => {
            serde_args.push("deserialize_with = \"Option::deserialize\"".to_string());
        }
        (FieldShape::OptionalNullable, None) => {
            serde_args.push("default".to_string());
            serde_args.push(SkipAbsent(rt).to_string());
        }
        // A set never holds `null`, so this shape has none.
        (FieldShape::OptionalHoldingNull, _) => {
            imports.deserialize_some = true;
            serde_args.push("default".to_string());
            serde_args.push("deserialize_with = \"deserialize_some\"".to_string());
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
    }
}

/// [`shape_serde_args`] for a value that holds integers (see [`reads_integers`]): every shape
/// reads through `json_schema_rs::integer` (a set through the `unique_items` helpers that read
/// their items the same way).
fn integer_serde_args(
    rt: &str,
    shape: FieldShape,
    set: Option<SetKind>,
    imports: &mut Imports,
    serde_args: &mut Vec<String>,
) {
    let optional: bool = matches!(
        shape,
        FieldShape::Optional | FieldShape::OptionalNullable | FieldShape::OptionalHoldingNull
    );
    if optional {
        serde_args.push("default".to_string());
    }
    let path: String = match (shape, set) {
        (_, Some(_)) => {
            imports.unique_integer_shapes.insert(shape);
            format!(
                "unique_items::{}_integer",
                match shape {
                    FieldShape::Plain => "required",
                    FieldShape::Optional | FieldShape::OptionalHoldingNull => "optional",
                    FieldShape::RequiredNullable => "nullable",
                    FieldShape::OptionalNullable => "optional_nullable",
                }
            )
        }
        (FieldShape::OptionalHoldingNull, None) => {
            format!("{rt}::integer::deserialize_some")
        }
        (_, None) => IntegerRead(rt).to_string(),
    };
    serde_args.push(format!("deserialize_with = \"{path}\""));
    if shape == FieldShape::OptionalNullable {
        serde_args.push(SkipAbsent(rt).to_string());
    } else if optional {
        serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
    }
}

/// The `#[json_schema(..)]` arguments that carry the constraints of a field's value node
/// `value_id` into its reverse schema; a newtype carries its own.
fn constraint_schema_args(graph: &TypeGraph, value_id: TypeId, schema_args: &mut Vec<String>) {
    match graph.node(value_id) {
        // A newtype carries its own constraints.
        _ if graph.newtypes[value_id.index()].is_some() => {}
        TypeNode::String { constraints, .. } => {
            if let Some(n) = constraints.min_length {
                schema_args.push(format!("min_length = {n}"));
            }
            if let Some(n) = constraints.max_length {
                schema_args.push(format!("max_length = {n}"));
            }
            if let Some(ref p) = constraints.pattern {
                schema_args.push(format!("pattern = \"{}\"", escape(p)));
            }
        }
        TypeNode::Array {
            constraints,
            unique,
            ..
        } => {
            if let Some(n) = constraints.min_items {
                schema_args.push(format!("min_items = {n}"));
            }
            if let Some(n) = constraints.max_items {
                schema_args.push(format!("max_items = {n}"));
            }
            if *unique {
                schema_args.push("unique_items".to_string());
            }
        }
        _ => {}
    }
}

/// Renders one field named `name`, with its own `rename` when `rename` (see [`needs_rename`]).
fn render_field(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    owner: TypeId,
    field: &Field,
    (name, rename): (&str, bool),
    imports: &mut Imports,
) -> RenderedField {
    let (nullable, value_id): (bool, TypeId) = match graph.node(field.ty) {
        TypeNode::Option(inner) => (true, *inner),
        _ => (false, field.ty),
    };
    // A value type that itself holds `null` (any value, `()`, a union with a null branch):
    // `Option`'s own deserializer would turn `null` into `None`, so present values go through
    // `deserialize_some` and only absence is `None`.
    let holds_null: bool = !nullable && accepts_null(graph, value_id);
    let shape: FieldShape = match (field.required, nullable, holds_null) {
        (true, false, _) => FieldShape::Plain,
        (false, false, false) => FieldShape::Optional,
        (true, true, _) => FieldShape::RequiredNullable,
        (false, true, _) => FieldShape::OptionalNullable,
        (false, false, true) => FieldShape::OptionalHoldingNull,
    };
    let (value_ty, set): (String, Option<SetKind>) =
        field_value_type(graph, plan, home, value_id, imports);
    let ty: String = match shape {
        FieldShape::Plain => value_ty.clone(),
        // `T` already holds `null`: one `Option` marks absence.
        FieldShape::Optional | FieldShape::RequiredNullable | FieldShape::OptionalHoldingNull => {
            format!("Option<{value_ty}>")
        }
        FieldShape::OptionalNullable => format!("{}::Nullable<{value_ty}>", plan.runtime),
    };
    let mut serde_args: Vec<String> = Vec::new();
    if rename {
        serde_args.push(format!("rename = \"{}\"", escape(&field.json_key)));
    }
    // `default` is an annotation: deserializing never fills it in (absent stays absent, and a
    // required key stays required). An optional scalar field gets an accessor that applies it.
    let default_accessor: Option<String> = field.default.as_ref().and_then(|value: &Value| {
        field_default_accessor(graph, value_id, name, shape, &value_ty, value)
    });
    let integers: bool = reads_integers(graph, plan, value_id);
    shape_serde_args(
        &plan.runtime,
        shape,
        set,
        integers,
        imports,
        &mut serde_args,
    );
    let mut attrs: String = String::new();
    let (doc_lines, stated): (Vec<String>, Option<String>) =
        annotation_docs(field.description.as_deref(), None);
    attrs.push_str(&doc_comment(&doc_lines, "    "));
    if let Some(stated) = stated {
        let _ = writeln!(
            attrs,
            "    #[json_schema(description = \"{}\")]",
            escape(&stated)
        );
    }
    attrs.push_str(&deprecated_attr(
        field.deprecated,
        field.description.as_deref(),
        "    ",
    ));
    attrs.push_str(&expect_deprecated(
        graph,
        plan,
        field.ty,
        Some(owner),
        "    ",
    ));
    if !serde_args.is_empty() {
        let _ = writeln!(attrs, "    #[serde({})]", serde_args.join(", "));
    }
    let mut schema_args: Vec<String> = Vec::new();
    if shape == FieldShape::RequiredNullable {
        schema_args.push("required".to_string());
    }
    if let Some(schema) = schema_type(graph, plan, home, value_id, imports) {
        let with: String = match shape {
            FieldShape::Plain | FieldShape::Optional | FieldShape::OptionalHoldingNull => schema,
            FieldShape::RequiredNullable => format!("Option<{schema}>"),
            FieldShape::OptionalNullable => format!("{}::Nullable<{schema}>", plan.runtime),
        };
        schema_args.push(format!("with = \"{with}\""));
    }
    constraint_schema_args(graph, value_id, &mut schema_args);
    if let Some(arg) = field.default.as_ref().and_then(default_annotation) {
        schema_args.push(arg);
    }
    if !schema_args.is_empty() {
        let _ = writeln!(attrs, "    #[json_schema({})]", schema_args.join(", "));
    }
    let builder: (&'static str, String) = match shape {
        FieldShape::Plain | FieldShape::RequiredNullable => ("required", ty.clone()),
        FieldShape::Optional | FieldShape::OptionalHoldingNull => ("optional", value_ty),
        FieldShape::OptionalNullable => ("defaulted", ty.clone()),
    };
    RenderedField {
        name: name.to_string(),
        ty,
        attrs,
        default_accessor,
        builder,
    }
}

/// The derive line of a struct or union. Every one derives `PartialEq`, and `Eq` when it reaches
/// no float; a set item (or what one reaches) also `Hash` and `Ord` (see [`set_item_nodes`]),
/// where Rust equality, hashing and order agree with JSON equality. A struct whose properties
/// are all optional derives `Default`: the empty object, every key absent (never the schema's
/// `default`s, which deserializing does not fill in either). A struct derives
/// `json_schema_rs_macro::Deserialize`, which reads each field through one shared out-of-line
/// reader: serde's derive inlines every field's read into one function, which an optimized
/// build compiles in superlinear time (minutes for a struct with hundreds of fields). A union's
/// `Deserialize` is hand-written (see [`render_union`]).
fn item_derive(
    plan: &Plan,
    id: TypeId,
    default: bool,
    deserialize: bool,
    manual_eq: bool,
) -> (String, bool) {
    let schema_default: bool = plan.defaults.contains_key(&id);
    let mut traits: Vec<&str> = vec!["Debug", "Clone"];
    if default && !schema_default {
        traits.push("Default");
    }
    traits.push("PartialEq");
    if plan.ord_items.contains(&id) {
        traits.extend(["Eq", "Hash", "PartialOrd", "Ord"]);
    } else if plan.eq_nodes[id.index()] {
        traits.push("Eq");
    }
    traits.push("Serialize");
    if deserialize {
        traits.push("json_schema_rs_macro::Deserialize");
    }
    traits.push("json_schema_rs_macro::ToJsonSchema");
    derive_block(
        plan,
        plan.node_patches.get(&id),
        &traits,
        schema_default,
        manual_eq,
    )
}

/// The `Eq` impl of an item whose `Eq` derive is left out (see [`derive_block`]): `Eq` has no
/// method, so the impl is the whole of the derive except its assertion that every field is `Eq`.
/// A `deprecated` item names itself, so the impl expects that.
fn manual_eq_impl(name: &str, deprecated: bool) -> String {
    let expect: &str = if deprecated {
        "#[expect(deprecated)]\n"
    } else {
        ""
    };
    format!("{expect}impl ::core::cmp::Eq for {name} {{}}\n\n")
}

/// The derive line of an item deriving `base`, then its attribute lines: the settings' extra
/// derives and those of the item's `patch` that `base` lacks (compared by last path segment, so
/// `serde::Deserialize` matches a generated `Deserialize`; every item implements `Serialize` and
/// `Deserialize`, derived or written by hand, so those two are never added), then the settings'
/// extra attributes and the patch's. Ends with a newline. With `schema_default` (the item
/// implements `Default` from its schema `default`) no `Default` derive is added. Also whether
/// the `Eq` derive was left out. With
/// `manual_eq` (the item names a `#[deprecated]` item, see [`names_deprecated_item`]) the `Eq`
/// derive is left out and the caller writes [`manual_eq_impl`] instead: the derive asserts each
/// field's type is `Eq` and so names the deprecated type in code no attribute reaches, which
/// rustc reports as the consumer's use.
fn derive_block(
    plan: &Plan,
    patch: Option<&TypePatch>,
    base: &[&str],
    schema_default: bool,
    manual_eq: bool,
) -> (String, bool) {
    let last =
        |path: &str| -> String { path.rsplit("::").next().unwrap_or(path).trim().to_string() };
    let mut traits: Vec<&str> = base.to_vec();
    let patch_derives: &[String] = patch.map_or(&[], |p: &TypePatch| &p.derives);
    for extra in plan.extra_derives.iter().chain(patch_derives) {
        let name: String = last(extra);
        if name != "Serialize"
            && name != "Deserialize"
            && !(schema_default && name == "Default")
            && !traits.iter().any(|t: &&str| last(t) == name)
        {
            traits.push(extra);
        }
    }
    let before: usize = traits.len();
    if manual_eq {
        traits.retain(|t: &&str| last(t) != "Eq");
    }
    let dropped_eq: bool = traits.len() != before;
    let mut out: String = String::from("#[derive(");
    for (index, t) in traits.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        match t.strip_prefix("json_schema_rs_macro::") {
            Some(name) => {
                out.push_str(&plan.macros);
                out.push_str("::");
                out.push_str(name);
            }
            None => out.push_str(t),
        }
    }
    out.push_str(")]\n");
    if let Some(ref attr) = plan.crate_attr {
        out.push_str(attr);
    }
    let patch_attrs: &[String] = patch.map_or(&[], |p: &TypePatch| &p.attrs);
    for attr in plan.extra_attrs.iter().chain(patch_attrs) {
        let _ = writeln!(out, "#[{}]", attr.trim());
    }
    (out, dropped_eq)
}

/// Unit variants, one per distinct JSON value: equality, hashing and order are exact and cheap.
const STRING_ENUM_DERIVE: &[&str] = &[
    "Debug",
    "Clone",
    "PartialEq",
    "Eq",
    "Hash",
    "PartialOrd",
    "Ord",
    "Serialize",
    "Deserialize",
    "json_schema_rs_macro::ToJsonSchema",
];
const VALUE_ENUM_DERIVE: &[&str] = &[
    "Debug",
    "Clone",
    "PartialEq",
    "Eq",
    "Hash",
    "PartialOrd",
    "Ord",
    "json_schema_rs_macro::ToJsonSchema",
];

fn render_struct(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    name: &str,
    object: &ObjectNode,
    imports: &mut Imports,
) -> String {
    let (fields, rule): (Vec<RenderedField>, Option<RenameRule>) =
        render_fields(graph, plan, home, id, object, imports);
    let default_accessors: Vec<String> = fields
        .iter()
        .filter_map(|f| f.default_accessor.clone())
        .collect();
    let mut out: String = String::new();
    let a: &Annotations = &object.annotations;
    let (docs, stated): (String, Option<String>) = item_docs(a);
    out.push_str(&docs);
    out.push_str(&deprecated_attr(a.deprecated, a.description.as_deref(), ""));
    let default: bool = object.fields.iter().all(|field: &Field| !field.required);
    let manual_eq: bool = object
        .fields
        .iter()
        .map(|field: &Field| field.ty)
        .chain(object.additional)
        .any(|ty: TypeId| names_deprecated_item(graph, plan, ty, None));
    let (derive, dropped_eq): (String, bool) = item_derive(plan, id, default, true, manual_eq);
    out.push_str(&derive);
    out.push_str(&description_attr(stated.as_ref()));
    if let Some(ref title) = a.title {
        let _ = writeln!(out, "#[json_schema(title = \"{}\")]", escape(title));
    }
    if let Some(ref id) = a.id {
        let _ = writeln!(out, "#[json_schema(id = \"{}\")]", escape(id));
    }
    let mut container_args: Vec<String> = Vec::new();
    if object.deny_unknown {
        container_args.push("deny_unknown_fields".to_string());
    }
    if let Some(rule) = rule {
        container_args.push(format!("rename_all = \"{}\"", rule.serde_name()));
    }
    if !container_args.is_empty() {
        let _ = writeln!(out, "#[serde({})]", container_args.join(", "));
    }
    let _ = writeln!(out, "{} struct {name} {{", plan.vis);
    for field in &fields {
        out.push_str(&field.attrs);
        let _ = writeln!(out, "    {} {}: {},", plan.vis, field.name, field.ty);
    }
    out.push_str("}\n\n");
    if dropped_eq {
        out.push_str(&manual_eq_impl(name, a.deprecated == Some(true)));
    }
    out.push_str(&default_accessor_impl(
        name,
        a.deprecated == Some(true),
        &default_accessors,
    ));
    out
}

/// The `impl Name { ... }` of the default accessors (see [`render_default_accessor`]), or nothing
/// without any. For a `deprecated` struct the header names it, which is generated code's own use.
fn default_accessor_impl(name: &str, deprecated: bool, accessors: &[String]) -> String {
    if accessors.is_empty() {
        return String::new();
    }
    let expect: &str = if deprecated {
        "#[expect(deprecated)]\n"
    } else {
        ""
    };
    format!("{expect}impl {name} {{\n{}}}\n\n", accessors.join("\n"))
}

/// Records the builder of the struct `name` (see [`CodeGenSettings::builders`]) in `imports`: one
/// [`json_schema_rs::builder!`](crate::builder!) for the buffer's `builder` submodule, with the
/// struct's fields typed as seen from there (named items through `super::`) and each field's
/// kind (see [`RenderedField::builder`]). `validated` (the struct validates itself) validates
/// the built struct.
fn render_builder(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    name: &str,
    validated: bool,
    imports: &mut Imports,
) {
    let TypeNode::Object(object) = graph.node(id) else {
        return;
    };
    let mut scoped: Imports = Imports {
        item_prefix: "super::",
        ..Imports::default()
    };
    let (fields, _): (Vec<RenderedField>, Option<RenameRule>) =
        render_fields(graph, plan, home, id, object, &mut scoped);
    imports.builder_uses.extend(scoped.uses);
    let marker: &str = if validated { "@validated " } else { "" };
    let out: &mut String = &mut imports.builders;
    if !out.is_empty() {
        out.push('\n');
    }
    if fields.is_empty() {
        let _ = writeln!(out, "    {}::builder!({marker}{name} {{}});", plan.runtime);
        return;
    }
    let _ = writeln!(out, "    {}::builder!({marker}{name} {{", plan.runtime);
    for field in &fields {
        let (kind, ty) = &field.builder;
        let _ = writeln!(out, "        {kind} {}: {ty},", field.name);
    }
    out.push_str("    });\n");
}

/// A constrained newtype: the scalar behind a private field, so every value it holds was
/// validated against its schema (by `TryFrom` or `Deserialize`, through the embedded validator).
/// Reading goes through `Deref`, `AsRef`, `into_inner` and `From<Name>` for the scalar;
/// `Serialize` is the scalar's own.
#[expect(
    clippy::too_many_arguments,
    reason = "the newtype's node, names, patch and default flag all shape its one rendering"
)]
fn render_newtype(
    plan: &Plan,
    patch: Option<&TypePatch>,
    schema_default: bool,
    node: &TypeNode,
    name: &str,
    newtype: &Newtype,
    entry: &Entry,
    imports: &mut Imports,
) -> String {
    let inner: &str = scalar_type(node, imports);
    let string: bool = inner == "String";
    let a: &Annotations = &newtype.annotations;
    let (mut out, stated): (String, Option<String>) = item_docs(a);
    out.push_str(&deprecated_attr(a.deprecated, a.description.as_deref(), ""));
    let base: &[&str] = match node {
        TypeNode::String { .. } if string => &[
            "Debug",
            "Clone",
            "PartialEq",
            "Eq",
            "Hash",
            "PartialOrd",
            "Ord",
            "Serialize",
            "json_schema_rs_macro::ToJsonSchema",
        ],
        TypeNode::Number => &[
            "Debug",
            "Clone",
            "Copy",
            "PartialEq",
            "PartialOrd",
            "Serialize",
            "json_schema_rs_macro::ToJsonSchema",
        ],
        _ => &[
            "Debug",
            "Clone",
            "Copy",
            "PartialEq",
            "Eq",
            "Hash",
            "PartialOrd",
            "Ord",
            "Serialize",
            "json_schema_rs_macro::ToJsonSchema",
        ],
    };
    out.push_str(&derive_block(plan, patch, base, schema_default, false).0);
    out.push_str("#[serde(transparent)]\n");
    let mut schema_args: Vec<String> = Vec::new();
    if let Some(ref title) = a.title {
        schema_args.push(format!("title = \"{}\"", escape(title)));
    }
    if let Some(ref stated) = stated {
        schema_args.push(format!("description = \"{}\"", escape(stated)));
    }
    if let Some(ref id) = a.id {
        schema_args.push(format!("id = \"{}\"", escape(id)));
    }
    if let TypeNode::String { constraints, .. } = node {
        if let Some(n) = constraints.min_length {
            schema_args.push(format!("min_length = {n}"));
        }
        if let Some(n) = constraints.max_length {
            schema_args.push(format!("max_length = {n}"));
        }
        if let Some(ref p) = constraints.pattern {
            schema_args.push(format!("pattern = \"{}\"", escape(p)));
        }
    }
    let NumberConstraints {
        minimum,
        maximum,
        exclusive_minimum,
        exclusive_maximum,
        multiple_of,
    } = &newtype.numbers;
    for (key, value) in [
        ("minimum", minimum),
        ("maximum", maximum),
        ("exclusive_minimum", exclusive_minimum),
        ("exclusive_maximum", exclusive_maximum),
        ("multiple_of", multiple_of),
    ] {
        if let Some(n) = value {
            schema_args.push(format!("{key} = {n}"));
        }
    }
    if !schema_args.is_empty() {
        let _ = writeln!(out, "#[json_schema({})]", schema_args.join(", "));
    }
    let _ = writeln!(out, "{} struct {name}({inner});\n", plan.vis);
    let Entry { validators, index } = entry;
    let call: String = match inner {
        "String" => format!("string, {name}, {validators}, {index}"),
        _ if matches!(node, TypeNode::String { format, .. } if *format != StringFormat::Plain) => {
            format!("parsed, {name}, {inner}, {validators}, {index}")
        }
        _ if matches!(node, TypeNode::Integer(_, IntegerSemantics::Numeric)) => {
            format!("integer, {name}, {inner}, {validators}, {index}")
        }
        _ => format!("scalar, {name}, {inner}, {validators}, {index}"),
    };
    let _ = writeln!(out, "{}::constrained_newtype!({call});\n", plan.runtime);
    out
}

/// Schema `schema`'s root when it is a map or array that validates itself as a newtype
/// ([`render_root_alias`]): with self-validating types, a root that is not a named item and
/// has a validation target. A nested map or array is checked by the item holding it; a root has
/// no such parent.
fn root_newtype(graph: &TypeGraph, plan: &Plan, schema: usize) -> Option<TypeId> {
    let root: TypeId = graph.schemas[schema].root;
    (plan.self_validating
        && plan.root_aliases[schema].is_some()
        && matches!(
            graph.node(root),
            TypeNode::Array { .. } | TypeNode::Map { .. }
        )
        && graph.targets[root.index()].is_some())
    .then_some(root)
}

/// A buffer's self-validation statics (`has_sets`: the buffer embeds its schemas), with the root
/// newtype ([`root_newtype`]) registered among its items.
fn buffer_validators(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    items: &[TypeId],
    has_sets: bool,
) -> (Option<TypeId>, Option<BufferValidators>) {
    if !has_sets {
        return (None, None);
    }
    let root: Option<TypeId> = match home {
        Home::Schema(s) => root_newtype(graph, plan, s),
        Home::Shared => None,
    };
    let registered: Vec<TypeId> = items.iter().copied().chain(root).collect();
    (
        root,
        Some(BufferValidators::new(
            graph,
            &registered,
            plan.self_validating,
        )),
    )
}

/// A root that is not a named item: a `pub type` alias, or, given its validation `entry`, a
/// newtype over its Rust type (`pub struct Root(pub Vec<String>);`) with `validate` and
/// `TryFrom<serde_json::Value>`, like a struct: its `Deserialize` reads the shape, and
/// validation checks the whole schema.
fn render_root_alias(
    graph: &TypeGraph,
    plan: &Plan,
    schema: usize,
    name: &str,
    entry: Option<&Entry>,
    imports: &mut Imports,
) -> String {
    let home: Home = Home::Schema(schema);
    let root: TypeId = graph.schemas[schema].root;
    let patch: Option<&TypePatch> = plan.alias_patches[schema].as_ref();
    let ty: String = rust_type(graph, plan, home, root, imports);
    let expect: String = expect_deprecated(graph, plan, root, None, "");
    let Some(entry) = entry else {
        return format!("{expect}{} type {name} = {ty};\n\n", plan.vis);
    };
    imports.deserialize = true;
    let mut traits: Vec<&str> = vec!["Debug", "Clone", "PartialEq"];
    if plan.eq_nodes[root.index()] {
        traits.push("Eq");
    }
    traits.extend([
        "Serialize",
        "Deserialize",
        "json_schema_rs_macro::ToJsonSchema",
    ]);
    let manual_eq: bool = names_deprecated_item(graph, plan, root, None);
    let (mut out, dropped_eq): (String, bool) =
        derive_block(plan, patch, &traits, false, manual_eq);
    out.push_str("#[serde(transparent)]\n");
    let field_attrs: String = if reads_integers(graph, plan, root) {
        format!(
            "#[serde(deserialize_with = \"{}\")] ",
            IntegerRead(&plan.runtime)
        )
    } else {
        String::new()
    };
    let vis: &str = plan.vis;
    let field_attrs: String = if expect.is_empty() {
        field_attrs
    } else {
        format!("#[expect(deprecated)] {field_attrs}")
    };
    let _ = writeln!(out, "{vis} struct {name}({field_attrs}{vis} {ty});\n");
    if dropped_eq {
        out.push_str(&manual_eq_impl(name, false));
    }
    out.push_str(&rust_validators::render_item_impls(
        &plan.runtime,
        name,
        entry,
    ));
    out
}

/// The variant name for one enum value.
fn value_variant_base(value: &Value) -> String {
    match value {
        Value::String(s) => enum_variant_name_from_value(s),
        Value::Null => "Null".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Number(n) => {
            let text: String = n
                .to_string()
                .replace('-', "Minus")
                .replace('.', "Point")
                .replace('+', "");
            to_pascal_case(&format!("N{text}"))
        }
        Value::Array(_) => "Array".to_string(),
        Value::Object(_) => "Object".to_string(),
    }
}

fn render_enum(
    plan: &Plan,
    patch: Option<&TypePatch>,
    schema_default: bool,
    name: &str,
    e: &EnumNode,
    imports: &mut Imports,
) -> String {
    let bases: Vec<String> = e.values.iter().map(value_variant_base).collect();
    let variants: Vec<String> = lint_clean_variant_names(name, unique_idents(&bases, ""));
    let (mut out, stated): (String, Option<String>) = item_docs(&e.annotations);
    imports.deserialize = true;
    if e.is_string_enum() {
        out.push_str(&derive_block(plan, patch, STRING_ENUM_DERIVE, schema_default, false).0);
        out.push_str(&description_attr(stated.as_ref()));
        let members: Vec<(&str, &str)> = e
            .values
            .iter()
            .zip(&variants)
            .map(|(value, variant)| (variant.as_str(), value.as_str().expect("string enum value")))
            .collect();
        let rule: Option<RenameRule> = rename_all_rule(&members, RenameRule::write_variant);
        if let Some(rule) = rule {
            let _ = writeln!(out, "#[serde(rename_all = \"{}\")]", rule.serde_name());
        }
        let _ = writeln!(out, "{} enum {name} {{", plan.vis);
        let mut buffer: String = String::new();
        for (variant, raw) in members {
            if needs_rename(rule, RenameRule::write_variant, variant, raw, &mut buffer) {
                let _ = writeln!(out, "    #[serde(rename = \"{}\")]", escape(raw));
            }
            let _ = writeln!(out, "    {variant},");
        }
        out.push_str("}\n\n");
        if plan.string_enum_impls && !e.values.is_empty() {
            let _ = writeln!(out, "{}::string_enum!({name});\n", plan.runtime);
        }
        return out;
    }
    // Values of other JSON types: serde goes through `serde_json::Value` and compares by JSON
    // Schema equality (`1` equals `1.0`).
    out.push_str(&derive_block(plan, patch, VALUE_ENUM_DERIVE, schema_default, false).0);
    out.push_str(&description_attr(stated.as_ref()));
    let _ = writeln!(out, "{} enum {name} {{", plan.vis);
    for (value, variant) in e.values.iter().zip(&variants) {
        let _ = writeln!(out, "    #[json_schema(value = {})]", json_tokens(value));
        let _ = writeln!(out, "    {variant},");
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "impl Serialize for {name} {{");
    out.push_str(
        "    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {\n",
    );
    out.push_str("        let value: serde_json::Value = match self {\n");
    for (value, variant) in e.values.iter().zip(&variants) {
        let _ = writeln!(
            out,
            "            Self::{variant} => serde_json::json!({}),",
            json_tokens(value)
        );
    }
    out.push_str("        };\n        value.serialize(serializer)\n    }\n}\n\n");
    let _ = writeln!(out, "impl<'de> Deserialize<'de> for {name} {{");
    out.push_str(
        "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {\n",
    );
    out.push_str(
        "        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;\n",
    );
    for (value, variant) in e.values.iter().zip(&variants) {
        let _ = writeln!(
            out,
            "        if {}::json_equal(&value, &serde_json::json!({})) {{\n            return Ok(Self::{variant});\n        }}",
            plan.runtime,
            json_tokens(value)
        );
    }
    // The validator's wording, without the generated type's name.
    let allowed: String = e
        .values
        .iter()
        .map(|value| serde_json::to_string(value).unwrap_or_default())
        .collect::<Vec<String>>()
        .join(", ");
    let _ = writeln!(
        out,
        "        Err(serde::de::Error::custom(format!(\"value {{value}} not in enum (allowed: {{}})\", {allowed:?})))\n    }}\n}}\n"
    );
    out
}

/// The variant name for a union branch: its title, the named type a `$ref` points to, the
/// last segment of a user's type, the kind of a scalar, else `Variant{i}`.
/// The name a user's type suggests: its path's last segment, without generic arguments.
fn external_base(path: &str) -> String {
    let bare: &str = path.split('<').next().unwrap_or(path);
    sanitize_struct_name(bare.rsplit("::").next().unwrap_or(bare))
}

fn variant_base(graph: &TypeGraph, plan: &Plan, union: &UnionNode, index: usize) -> String {
    let variant = &union.variants[index];
    if let Some(ref title) = variant.title {
        return sanitize_struct_name(title);
    }
    let target: TypeId = graph.peel(variant.ty);
    if let Some((_, name)) = plan.items.get(&target)
        && (variant.is_ref || union.kind == UnionKind::TypeArray)
    {
        return name.clone();
    }
    if let TypeNode::External { path, .. } = graph.node(target) {
        return external_base(path);
    }
    let kind: Option<&str> = match graph.node(target) {
        TypeNode::Null => Some("Null"),
        TypeNode::Boolean => Some("Boolean"),
        TypeNode::Integer(..) => Some("Integer"),
        TypeNode::Number => Some("Number"),
        TypeNode::String { .. } => Some("String"),
        TypeNode::Array { .. } => Some("Array"),
        TypeNode::Map { .. } => Some("Object"),
        TypeNode::Any => Some("Value"),
        TypeNode::Object(_) if union.kind == UnionKind::TypeArray => Some("Object"),
        _ => None,
    };
    kind.map_or_else(|| format!("Variant{index}"), str::to_string)
}

/// Renders a union: an untagged enum deriving only `Serialize`. With `validated` (a
/// self-validating `anyOf` / `oneOf`) its `Deserialize` chooses the variant by validation;
/// otherwise `json_schema_rs::deserialize_untagged!` takes the first variant that deserializes.
#[expect(clippy::too_many_arguments)]
fn render_union(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    name: &str,
    union: &UnionNode,
    validated: Option<&UnionEntries>,
    imports: &mut Imports,
) -> String {
    let bases: Vec<String> = (0..union.variants.len())
        .map(|i| variant_base(graph, plan, union, i))
        .collect();
    let names: Vec<String> = lint_clean_variant_names(name, unique_idents(&bases, ""));
    let (mut out, stated): (String, Option<String>) = item_docs(&union.annotations);
    let manual_eq: bool = union
        .variants
        .iter()
        .any(|variant: &type_graph::Variant| names_deprecated_item(graph, plan, variant.ty, None));
    let (derive, dropped_eq): (String, bool) = item_derive(plan, id, false, false, manual_eq);
    out.push_str(&derive);
    out.push_str(&description_attr(stated.as_ref()));
    out.push_str("#[serde(untagged)]\n");
    if union.kind == UnionKind::OneOf {
        out.push_str("#[json_schema(one_of)]\n");
    }
    let _ = writeln!(out, "{} enum {name} {{", plan.vis);
    let mut variants: Vec<rust_validators::UnionVariant> = Vec::with_capacity(union.variants.len());
    for (variant, variant_name) in union.variants.iter().zip(&names) {
        let ty: String = rust_type(graph, plan, home, variant.ty, imports);
        out.push_str(&expect_deprecated(graph, plan, variant.ty, None, "    "));
        match schema_type(graph, plan, home, variant.ty, imports) {
            Some(schema) => {
                let _ = writeln!(
                    out,
                    "    {variant_name}(#[json_schema(with = \"{schema}\")] {ty}),"
                );
            }
            None => {
                let _ = writeln!(out, "    {variant_name}({ty}),");
            }
        }
        variants.push(rust_validators::UnionVariant {
            name: variant_name.clone(),
            ty,
            integers: reads_integers(graph, plan, variant.ty),
        });
    }
    out.push_str("}\n\n");
    if dropped_eq {
        out.push_str(&manual_eq_impl(name, false));
    }
    if let Some(entries) = validated {
        // Only an arm reading through serde's own `Deserialize` names the trait.
        imports.deserialize |= variants.iter().any(|variant| !variant.integers);
        out.push_str(&rust_validators::render_union_deserialize(
            &plan.runtime,
            name,
            union.kind,
            entries,
            &variants,
        ));
    } else {
        // serde's untagged derive buffers the input through its `Content`, which breaks numbers
        // under `serde_json/arbitrary_precision`; the macro buffers a `serde_json::Value`.
        let list: Vec<String> = variants
            .iter()
            .map(|v| {
                if v.integers {
                    format!(
                        "{}({} = with({}))",
                        v.name,
                        v.ty,
                        IntegerRead(&plan.runtime)
                    )
                } else {
                    format!("{}({})", v.name, v.ty)
                }
            })
            .collect();
        let list: String = if list.is_empty() {
            String::new()
        } else {
            format!(" {} ", list.join(", "))
        };
        let _ = writeln!(
            out,
            "{}::deserialize_untagged!({name} {{{list}}});\n",
            plan.runtime
        );
    }
    out
}

/// The items of `home` in output order: a depth-first walk from the buffer's starting points,
/// each item after the items it uses (children in field / variant order), so the file reads
/// bottom-up. Per-schema buffers start at the root, then each definition; the shared buffer
/// starts at its items in name order.
fn ordered_items(graph: &TypeGraph, plan: &Plan, home: Home) -> Vec<TypeId> {
    let starts: Vec<TypeId> = match home {
        Home::Schema(s) => {
            let roots = &graph.schemas[s];
            std::iter::once(roots.root)
                .chain(roots.definitions.iter().map(|(_, id)| *id))
                .collect()
        }
        Home::Shared => {
            let mut shared: Vec<(&str, TypeId)> = plan
                .items
                .iter()
                .filter(|(_, (h, _))| *h == Home::Shared)
                .map(|(id, (_, name))| (name.as_str(), *id))
                .collect();
            shared.sort_unstable();
            shared.into_iter().map(|(_, id)| id).collect()
        }
    };
    let mut out: Vec<TypeId> = Vec::new();
    let mut visited: BTreeSet<TypeId> = BTreeSet::new();
    for start in starts {
        if visited.contains(&start) {
            continue;
        }
        visited.insert(start);
        // Frames: (node, its children, next child index).
        let mut stack: Vec<(TypeId, Vec<TypeId>, usize)> =
            vec![(start, graph.node(start).children(), 0)];
        while let Some((id, children, next)) = stack.last_mut() {
            if *next < children.len() {
                let child: TypeId = children[*next];
                *next += 1;
                if visited.insert(child) {
                    let grandchildren: Vec<TypeId> = graph.node(child).children();
                    stack.push((child, grandchildren, 0));
                }
                continue;
            }
            let id: TypeId = *id;
            stack.pop();
            if plan.items.get(&id).is_some_and(|(h, _)| *h == home) {
                out.push(id);
            }
        }
    }
    out
}

/// The `impl Default` of item `id` from its schema `default`, when it has one: one
/// [`json_schema_rs::schema_default!`](crate::schema_default!) line.
fn push_schema_default(body: &mut String, plan: &Plan, id: TypeId, name: &str) {
    if let Some(json) = plan.defaults.get(&id) {
        let _ = writeln!(
            body,
            "{}::schema_default!({name}, {json:?});\n",
            plan.runtime
        );
    }
}

/// Renders one output buffer. `sets` (with self-validating types) holds each input's embedded
/// documents.
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the buffer's items, its validators and its builders, in output order"
)]
fn render_buffer(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    sets: Option<&[EmbeddedSet]>,
) -> CodeGenResult<Vec<u8>> {
    let mut imports: Imports = Imports::default();
    let mut body: String = String::new();
    let items: Vec<TypeId> = ordered_items(graph, plan, home);
    let (root_newtype, validators): (Option<TypeId>, Option<BufferValidators>) =
        buffer_validators(graph, plan, home, &items, sets.is_some());
    for &id in &items {
        let name: &str = plan.name(id);
        debug_assert_eq!(plan.home(id), home);
        let union_entries: Option<UnionEntries> = validators.as_ref().and_then(|v| v.union(id));
        let entry: Option<Entry> = validators.as_ref().and_then(|v| v.item(id));
        if let Some(newtype) = &graph.newtypes[id.index()] {
            let entry: &Entry = entry.as_ref().expect("a newtype has a validation target");
            body.push_str(&render_newtype(
                plan,
                plan.node_patches.get(&id),
                plan.defaults.contains_key(&id),
                graph.node(id),
                name,
                newtype,
                entry,
                &mut imports,
            ));
            push_schema_default(&mut body, plan, id, name);
            continue;
        }
        let item: String = match graph.node(id) {
            TypeNode::Object(o) => {
                if plan.builders {
                    render_builder(graph, plan, home, id, name, entry.is_some(), &mut imports);
                }
                render_struct(graph, plan, home, id, name, o, &mut imports)
            }
            TypeNode::Enum(e) => render_enum(
                plan,
                plan.node_patches.get(&id),
                plan.defaults.contains_key(&id),
                name,
                e,
                &mut imports,
            ),
            TypeNode::Union(u) => render_union(
                graph,
                plan,
                home,
                id,
                name,
                u,
                union_entries.as_ref(),
                &mut imports,
            ),
            _ => unreachable!("only named nodes are items"),
        };
        body.push_str(&item);
        if let Some(entry) = entry {
            body.push_str(&rust_validators::render_item_impls(
                &plan.runtime,
                name,
                &entry,
            ));
        }
        push_schema_default(&mut body, plan, id, name);
    }
    if let (Some(validators), Some(sets)) = (&validators, sets) {
        body.push_str(&validators.render(&plan.runtime, sets)?);
    }
    if !imports.builders.is_empty() {
        let _ = writeln!(
            body,
            "/// The builders of this module's structs (`Name::builder()`).\n{} mod builder {{",
            plan.vis
        );
        for path in &imports.builder_uses {
            let _ = writeln!(body, "    use {path};");
        }
        if !imports.builder_uses.is_empty() {
            body.push('\n');
        }
        body.push_str(&imports.builders);
        body.push_str("}\n\n");
    }
    if let Home::Schema(s) = home {
        let roots = &graph.schemas[s];
        if let Some(ref alias) = plan.root_aliases[s] {
            let entry: Option<Entry> =
                root_newtype.and_then(|id| validators.as_ref().and_then(|v| v.item(id)));
            let root: String =
                render_root_alias(graph, plan, s, alias, entry.as_ref(), &mut imports);
            body.push_str(&root);
        }
        // A schema's own root and definitions are reachable from its module even when shared.
        for id in std::iter::once(roots.root).chain(roots.definitions.iter().map(|(_, id)| *id)) {
            let id: TypeId = graph.peel(id);
            if let Some((Home::Shared, name)) = plan.items.get(&id) {
                imports.shared.insert(name.clone());
            }
        }
    }
    let mut out: Vec<u8> = Vec::new();
    if let Some(ref header) = plan.header {
        for line in header.lines() {
            if line.is_empty() {
                writeln!(out, "//")?;
            } else {
                writeln!(out, "// {line}")?;
            }
        }
    }
    writeln!(
        out,
        "//! Generated by json-schema-rs. Do not edit manually."
    )?;
    writeln!(out)?;
    // Only items (a root newtype among them, and the `deserialize_some` helper) use serde: a
    // buffer holding just a type alias or re-exports of shared items needs no import.
    let serde_import: bool =
        !items.is_empty() || root_newtype.is_some() || imports.deserialize_some;
    if serde_import && (imports.deserialize || imports.deserialize_some) {
        writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
    } else if serde_import {
        writeln!(out, "use serde::Serialize;")?;
    }
    for path in &imports.uses {
        writeln!(out, "use {path};")?;
    }
    if serde_import || !imports.uses.is_empty() {
        writeln!(out)?;
    }
    if !imports.shared.is_empty() {
        for name in &imports.shared {
            // Re-exporting a deprecated item is a use of it, generated code's own.
            let deprecated: bool = plan.items.iter().any(|(id, (home, item))| {
                *home == Home::Shared && item == name && is_deprecated_item(graph, *id)
            });
            if deprecated {
                writeln!(out, "#[expect(deprecated)]")?;
            }
            writeln!(out, "{} use {}::{name};", plan.vis, plan.shared_path)?;
        }
        writeln!(out)?;
    }
    if imports.deserialize_some {
        out.write_all(DESERIALIZE_SOME.as_bytes())?;
    }
    if !imports.unique_shapes.is_empty() || !imports.unique_integer_shapes.is_empty() {
        out.write_all(unique_items_module(plan, &imports).as_bytes())?;
    }
    if !imports.converted.is_empty() {
        out.write_all(render_converted_schemas(plan, &imports).as_bytes())?;
    }
    out.write_all(body.as_bytes())?;
    Ok(out)
}

/// The schema markers of the conversions a buffer names: a private module of
/// [`json_schema_rs::schema_marker!`](crate::schema_marker) types, one per conversion, each
/// giving the conversion's schema to the `ToJsonSchema` derive (see [`schema_type`]).
fn render_converted_schemas(plan: &Plan, imports: &Imports) -> String {
    let mut out: String = "mod converted_schemas {\n".to_string();
    for &index in &imports.converted {
        let (name, json) = &plan.converted[index];
        let _ = writeln!(
            out,
            "    {}::schema_marker!({name}, {});",
            plan.runtime,
            rust_validators::raw_string(json)
        );
    }
    out.push_str("}\n\n");
    out
}

/// Per conversion in `settings`, its schema marker's name (the type's last path segment,
/// colliding names suffixed) and its schema as compact JSON.
fn converted_markers(settings: &CodeGenSettings) -> Vec<(String, String)> {
    let bases: Vec<String> = settings
        .conversions
        .iter()
        .map(|c| external_base(&c.rust_type))
        .collect();
    unique_idents(&bases, "")
        .into_iter()
        .zip(&settings.conversions)
        .map(|(name, c)| (name, c.schema.to_string()))
        .collect()
}

/// Helper for optional nullable fields: a present value (including `null`) is `Some(..)`, so
/// an absent field (`None`, via `#[serde(default)]`) differs from `null` (`Some(None)`).
const DESERIALIZE_SOME: &str =
    "fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

";

/// The paths the generated code reaches the library and the derive macros through, and the
/// attribute that points the derives at the library's path when it is not the default.
fn crate_paths(settings: &CodeGenSettings) -> (String, String, Option<String>) {
    let runtime: String = settings
        .crate_path
        .clone()
        .unwrap_or_else(|| "json_schema_rs".to_string());
    let macros: String = settings
        .macro_crate_path
        .clone()
        .unwrap_or_else(|| "json_schema_rs_macro".to_string());
    let attr: Option<String> = settings
        .crate_path
        .as_ref()
        .map(|path: &String| format!("#[json_schema(crate = \"{path}\")]\n"));
    (runtime, macros, attr)
}

/// The `unique_items` helper module for a buffer: the `UniqueSet` impls and the deserialize
/// functions its `uniqueItems` set fields use. A duplicate item is a deserialize error, so an
/// invalid instance is never silently turned into a different, valid one.
/// The templates name the library `json_schema_rs`; with another
/// [`crate_path`](CodeGenSettings::crate_path) the module names it by that path.
fn unique_items_module(plan: &Plan, imports: &Imports) -> String {
    let mut out: String = String::from(UNIQUE_ITEMS_HEAD);
    for kind in &imports.unique_sets {
        out.push_str(kind.helper_impl());
    }
    if !imports.unique_shapes.is_empty() {
        out.push_str(UNIQUE_ITEMS_VISITOR);
    }
    for shape in &imports.unique_shapes {
        out.push_str(match shape {
            FieldShape::Plain => UNIQUE_ITEMS_REQUIRED,
            FieldShape::Optional | FieldShape::OptionalHoldingNull => UNIQUE_ITEMS_OPTIONAL,
            FieldShape::RequiredNullable => UNIQUE_ITEMS_NULLABLE,
            FieldShape::OptionalNullable => UNIQUE_ITEMS_OPTIONAL_NULLABLE,
        });
    }
    if !imports.unique_integer_shapes.is_empty() {
        out.push_str(UNIQUE_ITEMS_INTEGER_VISITOR);
    }
    for shape in &imports.unique_integer_shapes {
        out.push_str(match shape {
            FieldShape::Plain => UNIQUE_ITEMS_REQUIRED_INTEGER,
            FieldShape::Optional | FieldShape::OptionalHoldingNull => UNIQUE_ITEMS_OPTIONAL_INTEGER,
            FieldShape::RequiredNullable => UNIQUE_ITEMS_NULLABLE_INTEGER,
            FieldShape::OptionalNullable => UNIQUE_ITEMS_OPTIONAL_NULLABLE_INTEGER,
        });
    }
    out.push_str("}\n\n");
    if plan.runtime == "json_schema_rs" {
        out
    } else {
        out.replace("json_schema_rs::", &format!("{}::", plan.runtime))
    }
}

const UNIQUE_ITEMS_HEAD: &str =
    "/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }
";

const UNIQUE_SET_BTREE_SET: &str = "
    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }
";

const UNIQUE_SET_HASH_SET: &str = "
    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }
";

#[cfg(feature = "indexmap")]
const UNIQUE_SET_INDEX_SET: &str = "
    impl<T: std::hash::Hash + Eq> UniqueSet for indexmap::IndexSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }
";

const UNIQUE_ITEMS_VISITOR: &str = "
    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(\"an array with unique items\")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        \"duplicate item at index {index} of an array with uniqueItems\"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }
";

const UNIQUE_ITEMS_REQUIRED: &str = "
    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
";

const UNIQUE_ITEMS_OPTIONAL: &str = "
    pub(super) fn optional<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| Some(unique.0))
    }
";

const UNIQUE_ITEMS_NULLABLE: &str = "
    pub(super) fn nullable<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Option::<Unique<S>>::deserialize(deserializer)
            .map(|value: Option<Unique<S>>| value.map(|unique: Unique<S>| unique.0))
    }
";

const UNIQUE_ITEMS_OPTIONAL_NULLABLE: &str = "
    pub(super) fn optional_nullable<'de, D, S>(
        deserializer: D,
    ) -> Result<json_schema_rs::Nullable<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        json_schema_rs::Nullable::<Unique<S>>::deserialize(deserializer)
            .map(|value: json_schema_rs::Nullable<Unique<S>>| value.map(|unique: Unique<S>| unique.0))
    }
";

const UNIQUE_ITEMS_INTEGER_VISITOR: &str = "
    /// Reads integer items by JSON Schema's integer rule (`1.0` is the integer 1).
    struct IntegerVisitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for IntegerVisitor<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(\"an array with unique items\")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(json_schema_rs::integer::Integer(item)) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        \"duplicate item at index {index} of an array with uniqueItems\"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array of integers read into the set `S`.
    struct UniqueIntegers<S>(S);

    impl<'de, S> Deserialize<'de> for UniqueIntegers<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(IntegerVisitor(std::marker::PhantomData))
                .map(UniqueIntegers)
        }
    }
";

const UNIQUE_ITEMS_REQUIRED_INTEGER: &str = "
    pub(super) fn required_integer<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| unique.0)
    }
";

const UNIQUE_ITEMS_OPTIONAL_INTEGER: &str = "
    pub(super) fn optional_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| Some(unique.0))
    }
";

const UNIQUE_ITEMS_NULLABLE_INTEGER: &str = "
    pub(super) fn nullable_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        Option::<UniqueIntegers<S>>::deserialize(deserializer)
            .map(|value: Option<UniqueIntegers<S>>| value.map(|unique: UniqueIntegers<S>| unique.0))
    }
";

const UNIQUE_ITEMS_OPTIONAL_NULLABLE_INTEGER: &str = "
    pub(super) fn optional_nullable_integer<'de, D, S>(
        deserializer: D,
    ) -> Result<json_schema_rs::Nullable<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        json_schema_rs::Nullable::<UniqueIntegers<S>>::deserialize(deserializer).map(
            |value: json_schema_rs::Nullable<UniqueIntegers<S>>| value.map(|unique: UniqueIntegers<S>| unique.0),
        )
    }
";

#[cfg(test)]
mod tests {
    use super::CodeGenError;
    use super::{CodeGenBackend, CodeGenInput, RustBackend, generate_rust};
    use crate::code_gen::settings::{
        CodeGenSettings, DedupeMode, FormatTypes, MapType, ModelNameSource, TypePatch,
        UniqueItemsType, UnknownProperties, Visibility,
    };
    use crate::json_schema::{JsonSchema, JsonType, RegistryError, Schema};

    /// The defaults with self-validation off: these goldens show the shape of the generated
    /// types; the self-validating output has its own tests (see *Self-validating types*).
    fn default_settings() -> CodeGenSettings {
        CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .build()
    }

    /// Every output buffer as text: the shared buffer, then one per schema.
    fn buffers(output: super::GenerateRustOutput) -> (Option<String>, Vec<String>) {
        let shared: Option<String> = output.shared.map(|b| String::from_utf8(b).unwrap());
        let per_schema: Vec<String> = output
            .per_schema
            .into_iter()
            .map(|b| String::from_utf8(b).unwrap())
            .collect();
        (shared, per_schema)
    }

    /// Parses each JSON value as a schema.
    fn schemas_of(values: &[serde_json::Value]) -> Vec<Schema> {
        values
            .iter()
            .map(|v| Schema::try_from(v.to_string().as_str()).unwrap())
            .collect()
    }

    /// `a.json` with an optional `b` property referring to `b_ref`, and `b.json` titled `Bee`.
    fn sibling_documents(b_ref: &str) -> Vec<Schema> {
        schemas_of(&[
            serde_json::json!({"type": "object", "properties": {"b": {"$ref": b_ref}}}),
            serde_json::json!({
                "title": "Bee",
                "type": "object",
                "properties": {"x": {"type": "integer"}},
                "required": ["x"]
            }),
        ])
    }

    #[test]
    fn cross_document_ref_to_sibling_input_shares_its_type() {
        let schemas: Vec<Schema> = sibling_documents("b.json");
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("file:///s/a.json", &schemas[0])
            .schema_at("file:///s/b.json", &schemas[1]);
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(input, &default_settings()).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Bee")]
pub struct Bee {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

pub use super::shared::Bee;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Bee>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Bee;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_ref_to_fragment_of_sibling_input() {
        let schemas: Vec<Schema> = schemas_of(&[
            serde_json::json!({"type": "object", "properties": {"t": {"$ref": "b.json#/$defs/Thing"}}}),
            serde_json::json!({
                "$defs": {"Thing": {"type": "object", "properties": {"n": {"type": "string"}}}},
                "type": "string"
            }),
        ]);
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("file:///s/a.json", &schemas[0])
            .schema_at("file:///s/b.json", &schemas[1]);
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(input, &default_settings()).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Thing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

pub use super::shared::Thing;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Thing;

pub type Root = String;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_refs_between_two_inputs_in_both_directions() {
        let schemas: Vec<Schema> = schemas_of(&[
            serde_json::json!({
                "title": "Parent",
                "type": "object",
                "properties": {"children": {"type": "array", "items": {"$ref": "child.json"}}}
            }),
            serde_json::json!({
                "title": "Child",
                "type": "object",
                "properties": {"parent": {"$ref": "parent.json"}}
            }),
        ]);
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("file:///s/parent.json", &schemas[0])
            .schema_at("file:///s/child.json", &schemas[1]);
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(input, &default_settings()).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Parent")]
pub struct Parent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Child>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Child")]
pub struct Child {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<Parent>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Parent;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Child;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_ref_to_retrieved_document_is_generated_with_the_referrer() {
        let schemas: Vec<Schema> = sibling_documents("b.json");
        let documents: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::from([(
                "file:///s/b.json".to_string(),
                serde_json::to_value(&schemas[1]).unwrap(),
            )]);
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("file:///s/a.json", &schemas[0])
            .retriever(documents);
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(input, &default_settings()).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Bee")]
pub struct Bee {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Bee>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_ref_to_missing_document_is_an_error_naming_its_uri() {
        let schemas: Vec<Schema> = sibling_documents("b.json");
        let input: CodeGenInput<'_> =
            CodeGenInput::new().schema_at("file:///s/a.json", &schemas[0]);
        let actual: String = generate_rust(input, &default_settings())
            .unwrap_err()
            .to_string();
        let expected: &str = r#"schema at index 0: $ref "b.json" at /properties/b ("file:///s/b.json") does not resolve: could not retrieve "file:///s/b.json": retrieval is disabled; no document was given for "file:///s/b.json""#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_ref_reaches_input_without_uri_by_its_root_id() {
        let schemas: Vec<Schema> = schemas_of(&[
            serde_json::json!({"type": "object", "properties": {"b": {"$ref": "https://example.com/b.json"}}}),
            serde_json::json!({"$id": "https://example.com/b.json", "title": "Bee", "type": "string"}),
        ]);
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(&schemas, &default_settings()).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub type Bee = String;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cross_document_input_with_relative_uri_is_an_error_at_its_own_index() {
        let schemas: Vec<Schema> = schemas_of(&[
            serde_json::json!({"type": "string"}),
            serde_json::json!({"type": "integer"}),
        ]);
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("file:///s/a.json", &schemas[0])
            .schema_at("b.json", &schemas[1]);
        let actual: String = generate_rust(input, &default_settings())
            .unwrap_err()
            .to_string();
        let expected: &str = r#"schema at index 1: "b.json" is not a valid URI: it has no scheme (it is a relative reference)"#;
        assert_eq!(expected, actual);
    }

    /// A retriever that serves one string schema and counts its calls.
    struct CountingRetriever {
        calls: std::cell::Cell<usize>,
    }

    impl crate::json_schema::Retrieve for CountingRetriever {
        fn retrieve(
            &self,
            _uri: &str,
        ) -> Result<serde_json::Value, crate::json_schema::RetrieveError> {
            self.calls.set(self.calls.get() + 1);
            Ok(serde_json::json!({"type": "string"}))
        }
    }

    #[test]
    fn cross_document_retrieves_a_document_two_inputs_reach_once() {
        let schemas: Vec<Schema> = schemas_of(&[
            serde_json::json!({"$ref": "https://example.com/common.json"}),
            serde_json::json!({"$ref": "https://example.com/common.json"}),
        ]);
        let retriever: CountingRetriever = CountingRetriever {
            calls: std::cell::Cell::new(0),
        };
        let input: CodeGenInput<'_> = CodeGenInput::from(&schemas).retriever(&retriever);
        generate_rust(input, &default_settings()).unwrap();
        let actual: usize = retriever.calls.get();
        let expected: usize = 1;
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_with_id_emits_id_attribute() {
        let json = r#"{"$id":"http://example.com/schema","type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = "http://example.com/schema")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_false_emits_deny_unknown_fields() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deprecated_type_named_by_another_item_expects_the_deprecation_and_implements_eq_by_hand() {
        let json = r#"{"title":"Item","type":"object","deprecated":true,"properties":{"id":{"type":"string"},"old":{"title":"Old","type":"object","deprecated":true,"properties":{"x":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[deprecated]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Old")]
pub struct Old {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[deprecated]
#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[deprecated]
    #[expect(deprecated)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<Old>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[expect(deprecated)]
impl ::core::cmp::Eq for Item {}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deprecated_type_naming_itself_expects_nothing_and_implements_eq_by_hand() {
        let json = r##"{"title":"Node","type":"object","deprecated":true,"properties":{"children":{"type":"array","items":{"$ref":"#"}},"next":{"$ref":"#"}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[deprecated]
#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[expect(deprecated)]
impl ::core::cmp::Eq for Node {}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deprecated_property_emits_deprecated_attr() {
        let json =
            r#"{"type":"object","properties":{"legacy":{"type":"string","deprecated":true}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[deprecated]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deprecated_property_with_description_emits_deprecated_with_message() {
        let json = r#"{"type":"object","properties":{"old":{"type":"string","deprecated":true,"description":"Use 'new' instead"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Use 'new' instead
    #[deprecated = "Use 'new' instead"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deprecated_false_emits_no_attr() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string","deprecated":false}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_properties_schema_emits_flattened_map_field() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":{"type":"string"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An optional `["string", "null"]` property tells absent (`None`) from `null` (`Some(None)`).
    #[test]
    fn optional_property_type_string_or_null_emits_nullable_string() {
        let json = r#"{"type":"object","properties":{"name":{"type":["string","null"]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub name: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The order of a type array does not matter: `["null", "string"]` reads as `["string", "null"]`.
    #[test]
    fn optional_property_type_null_or_string_emits_nullable_string() {
        let json = r#"{"type":"object","properties":{"name":{"type":["null","string"]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub name: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A struct with an additional-properties map derives `json_schema_rs_macro::Deserialize`,
    /// which reads the `#[serde(flatten)]` map key by key, and a type-array union reads through
    /// `json_schema_rs::deserialize_untagged!`: neither uses serde's derive, whose
    /// `#[serde(flatten)]` / `#[serde(untagged)]` would buffer the input. `Serialize` keeps the
    /// serde attributes.
    #[test]
    #[expect(clippy::too_many_lines)]
    fn struct_with_additional_map_and_type_array_union_deserialize_without_buffering() {
        let json = r#"{"type":"object","properties":{"plain":{"type":"number"},"opt":{"type":"string"},"req_null":{"type":["integer","null"]},"opt_null":{"type":["integer","null"]},"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true},"Renamed-Key":{"type":"boolean"},"multi":{"type":["string","number"]}},"required":["plain","req_null","Renamed-Key"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn optional<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| Some(unique.0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Multi {
    Number(f64),
    String(String),
}

json_schema_rs::deserialize_untagged!(Multi { Number(f64), String(String) });

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "Renamed-Key")]
    pub renamed_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multi: Option<Multi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opt: Option<String>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub opt_null: json_schema_rs::Nullable<i64>,
    pub plain: f64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    #[json_schema(required)]
    pub req_null: Option<i64>,
    #[serde(default, deserialize_with = "unique_items::optional", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub tags: Option<BTreeSet<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A type array with two non-null types becomes an untagged enum named after the property.
    #[test]
    fn property_type_with_two_non_null_types_emits_untagged_union() {
        let json = r#"{"type":"object","properties":{"id":{"type":["integer","string"]},"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Id {
    Integer(i64),
    String(String),
}

json_schema_rs::deserialize_untagged!(Id { Integer(i64 = with(json_schema_rs::integer::deserialize)), String(String) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A `true` property schema accepts any value, `null` included: an optional `serde_json::Value`
    /// read through `deserialize_some`, so absent and `null` differ.
    #[test]
    fn property_true_schema_emits_optional_value_field() {
        let json = r#"{"type":"object","properties":{"anything":true,"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub anything: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A `false` property schema accepts no value, so the property can only be absent: no field.
    #[test]
    fn property_false_schema_is_omitted() {
        let json = r#"{"type":"object","properties":{"never":false,"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `"items": true` reads as `{}`: an array of any JSON value.
    #[test]
    fn array_items_true_emits_vec_of_value() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":true}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<serde_json::Value>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `additionalProperties: true` allows extra keys, like leaving it out: they are captured in a
    /// flattened `serde_json::Value` map.
    #[test]
    fn additional_properties_true_captures_unknown_keys_in_value_map() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":true}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A root whose type is `["object", "null"]` reads as `object` and still emits its struct.
    #[test]
    fn root_type_object_or_null_with_properties_emits_struct() {
        let json = r#"{"type":["object","null"],"properties":{"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_string_property() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn string_property_with_pattern_emits_attribute() {
        let json =
            r#"{"type":"object","properties":{"name":{"type":"string","pattern":"^[a-z]+$"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(pattern = "^[a-z]+$")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_field_emits_without_option() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_defs_object_emits_named_struct_and_field_type() {
        let json = r##"{
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
}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub address: Address,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_missing_defs_returns_registry_error() {
        let json = r##"{
  "type": "object",
  "properties": { "x": { "$ref": "#/$defs/Missing" } }
}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual: super::CodeGenResult<super::GenerateRustOutput> =
            generate_rust(&[schema], &settings);
        assert!(
            matches!(
                actual,
                Err(CodeGenError::Batch { index: 0, ref source })
                if matches!(
                    source.as_ref(),
                    CodeGenError::Registry(errors)
                    if errors.to_vec() == vec![RegistryError::UnresolvableRef {
                        ref_str: "#/$defs/Missing".to_string(),
                        location: "/properties/x".to_string(),
                        target: "json-schema:///#/$defs/Missing".to_string(),
                        reason: "no such location".to_string(),
                    }]
                )
            ),
            "expected a registry error for the missing definition, got: {actual:?}"
        );
    }

    /// A `$ref` chain that loops without reaching a schema of its own constrains nothing: the
    /// property is any JSON value, and the definitions on the loop emit no items.
    #[test]
    fn pure_ref_loop_emits_value() {
        let json: &str = r##"{"$defs":{"A":{"$ref":"#/$defs/B"},"B":{"$ref":"#/$defs/A"}},"type":"object","properties":{"a":{"$ref":"#/$defs/A"}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub a: Option<serde_json::Value>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_enum_property_emits_enum_and_struct() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"]}},"required":["status"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Closed,
    Open,
}

json_schema_rs::string_enum!(Status);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub status: Status,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_string_property_emits_single_variant_enum() {
        let json = r#"{"type":"object","properties":{"key":{"const":"only"}},"required":["key"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Key {
    #[serde(rename = "only")]
    Only,
}

json_schema_rs::string_enum!(Key);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub key: Key,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn all_of_merged_object_golden() {
        let json = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}},"required":["b"]}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub b: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn anyof_property_golden() {
        let json = r#"{"type":"object","properties":{"foo":{"anyOf":[{"type":"string"},{"type":"object","properties":{"x":{"type":"integer"}}}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooObject {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum FooAnyOf {
    String(String),
    Variant1(FooObject),
}

json_schema_rs::deserialize_untagged!(FooAnyOf { String(String), Variant1(FooObject) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foo: Option<FooAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn anyof_root_golden() {
        let json = r#"{"anyOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

json_schema_rs::deserialize_untagged!(RootAnyOf { Variant0(RootVariant0), Variant1(RootVariant1) });

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn oneof_property_golden() {
        let json = r#"{"type":"object","properties":{"foo":{"oneOf":[{"type":"string"},{"type":"object","properties":{"x":{"type":"integer"}}}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooObject {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum FooOneOf {
    String(String),
    Variant1(FooObject),
}

json_schema_rs::deserialize_untagged!(FooOneOf { String(String), Variant1(FooObject) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foo: Option<FooOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn oneof_root_golden() {
        let json = r#"{"oneOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

json_schema_rs::deserialize_untagged!(RootOneOf { Variant0(RootVariant0), Variant1(RootVariant1) });

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_enum_property_emits_enum_and_struct() {
        let json = r#"{"type":"object","properties":{"level":{"enum":["low","medium","high"]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    High,
    Low,
    Medium,
}

json_schema_rs::string_enum!(Level);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_dedupe_two_properties_same_enum_emits_one_enum() {
        let json = r#"{"type":"object","properties":{"a":{"enum":["x","y"]},"b":{"enum":["x","y"]}},"required":["a"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum A {
    X,
    Y,
}

json_schema_rs::string_enum!(A);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub a: A,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<A>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_collision_emits_suffixed_variants() {
        let json = r#"{"type":"object","properties":{"t":{"enum":["a","A"]}},"required":["t"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum T {
    #[serde(rename = "A")]
    A0,
    #[serde(rename = "a")]
    A1,
}

json_schema_rs::string_enum!(T);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub t: T,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_values_sharing_a_leading_word_drop_it_from_the_variants() {
        let json = r#"{"type":"object","properties":{"kind":{"enum":["OPT_ONE","OPT_TWO","OPT_THREE"]}},"required":["kind"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Kind {
    #[serde(rename = "OPT_ONE")]
    One,
    #[serde(rename = "OPT_THREE")]
    Three,
    #[serde(rename = "OPT_TWO")]
    Two,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub kind: Kind,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_values_sharing_a_trailing_word_drop_it_from_the_variants() {
        let json = r#"{"type":"object","properties":{"kind":{"enum":["ONE_OPT","TWO_OPT","THREE_OPT"]}},"required":["kind"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Kind {
    #[serde(rename = "ONE_OPT")]
    One,
    #[serde(rename = "THREE_OPT")]
    Three,
    #[serde(rename = "TWO_OPT")]
    Two,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub kind: Kind,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_values_starting_and_ending_with_the_enum_name_drop_it() {
        let json = r#"{"type":"object","properties":{"shape":{"enum":["shape_circle","square_shape","shape","rank_1st"]}},"required":["shape"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Shape {
    #[serde(rename = "rank_1st")]
    Rank1st,
    #[serde(rename = "shape")]
    Value,
    #[serde(rename = "shape_circle")]
    Circle,
    #[serde(rename = "square_shape")]
    Square,
}

json_schema_rs::string_enum!(Shape);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub shape: Shape,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_variants_equal_after_dropping_the_enum_name_get_numeric_suffixes() {
        let json = r#"{"type":"object","properties":{"tier":{"enum":["tier_foo","foo","other"]}},"required":["tier"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Tier {
    #[serde(rename = "foo")]
    Foo0,
    #[serde(rename = "other")]
    Other,
    #[serde(rename = "tier_foo")]
    Foo1,
}

json_schema_rs::string_enum!(Tier);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tier: Tier,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_variant_starting_with_enum_name_then_digit_gets_e_prefix() {
        let json = r#"{"type":"object","properties":{"rank":{"enum":["rank_1st","two","three"]}},"required":["rank"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Rank {
    #[serde(rename = "rank_1st")]
    E1st,
    Three,
    Two,
}

json_schema_rs::string_enum!(Rank);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub rank: Rank,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn union_variants_sharing_a_trailing_word_drop_it() {
        let json = r#"{"type":"object","properties":{"pet":{"oneOf":[{"title":"CatPet","type":"object","properties":{"meow":{"type":"boolean"}}},{"title":"DogPet","type":"object","properties":{"bark":{"type":"boolean"}}},{"title":"BirdPet","type":"object","properties":{"tweet":{"type":"boolean"}}}]}},"required":["pet"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "CatPet")]
pub struct CatPet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "DogPet")]
pub struct DogPet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "BirdPet")]
pub struct BirdPet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tweet: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum PetOneOf {
    Cat(CatPet),
    Dog(DogPet),
    Bird(BirdPet),
}

json_schema_rs::deserialize_untagged!(PetOneOf { Cat(CatPet), Dog(DogPet), Bird(BirdPet) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub pet: PetOneOf,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn union_variants_starting_with_the_union_name_drop_it() {
        let json = r##"{"type":"object","$defs":{"Pet":{"oneOf":[{"title":"PetCat","type":"object","properties":{"meow":{"type":"boolean"}}},{"title":"PetDog","type":"object","properties":{"bark":{"type":"boolean"}}},{"type":"string"}]}},"properties":{"pet":{"$ref":"#/$defs/Pet"}},"required":["pet"]}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "PetCat")]
pub struct PetCat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "PetDog")]
pub struct PetDog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum Pet {
    Cat(PetCat),
    Dog(PetDog),
    String(String),
}

json_schema_rs::deserialize_untagged!(Pet { Cat(PetCat), Dog(PetDog), String(String) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub pet: Pet,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_duplicate_values_in_schema_emits_single_variant_per_value() {
        let json = r#"{"type":"object","properties":{"t":{"enum":["A","A","A","a","a","a","a","a","a","a"]}},"required":["t"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum T {
    #[serde(rename = "A")]
    A0,
    #[serde(rename = "a")]
    A1,
}

json_schema_rs::string_enum!(T);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub t: T,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_non_rust_compliant_values_emits_valid_variants() {
        let json = r#"{"type":"object","properties":{"id":{"enum":["/8633","todd.griffin"]}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Id {
    #[serde(rename = "/8633")]
    E8633,
    #[serde(rename = "todd.griffin")]
    ToddGriffin,
}

json_schema_rs::string_enum!(Id);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: Id,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn camel_case_property_emits_snake_case_field() {
        let json = r#"{"type":"object","properties":{"toddGriffin":{"type":"string"}},"required":["toddGriffin"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "toddGriffin")]
    pub todd_griffin: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_accessor_name_uses_snake_case_field_name() {
        let json = r#"{"type":"object","properties":{"person":{"title":"ValerieHunter","type":"object","properties":{"toddGriffin":{"type":"string","default":"bar"}}}},"required":["person"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "ValerieHunter")]
pub struct ValerieHunter {
    #[serde(rename = "toddGriffin", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "bar")]
    pub todd_griffin: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl ValerieHunter {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn todd_griffin_or_default(&self) -> &str {
        self.todd_griffin.as_deref().unwrap_or("bar")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub person: ValerieHunter,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_type_enum_emits_value_enum_with_json_equal_impls() {
        let json =
            r#"{"type":"object","properties":{"tag":{"enum":["foo",1,true]}},"required":["tag"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum Tag {
    #[json_schema(value = "foo")]
    Foo,
    #[json_schema(value = 1)]
    N1,
    #[json_schema(value = true)]
    True,
}

impl Serialize for Tag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::Foo => serde_json::json!("foo"),
            Self::N1 => serde_json::json!(1),
            Self::True => serde_json::json!(true),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("foo")) {
            return Ok(Self::Foo);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(true)) {
            return Ok(Self::True);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "\"foo\", 1, true")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tag: Tag,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_required_integer_property() {
        let json =
            r#"{"type":"object","properties":{"count":{"type":"integer"}},"required":["count"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub count: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_optional_integer_property() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_required_float_property() {
        let json =
            r#"{"type":"object","properties":{"value":{"type":"number"}},"required":["value"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn single_optional_float_property() {
        let json = r#"{"type":"object","properties":{"value":{"type":"number"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_minimum_maximum_u8_range_emits_u8() {
        let json = r#"{"type":"object","properties":{"byte":{"type":"integer","minimum":0,"maximum":255}},"required":["byte"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub byte: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_exclusive_bounds_emits_u8() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","exclusiveMinimum":-1,"exclusiveMaximum":256}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_fractional_bounds_rounds_inward() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":-0.5,"maximum":255.9}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_u64_range_emits_u64() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":18446744073709551615}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: u64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_range_beyond_64_bits_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":-1,"maximum":18446744073709551615}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_empty_range_emits_uninhabited_enum() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","exclusiveMinimum":5,"maximum":5}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum N {
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: N,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_tighter_exclusive_bound_wins() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":-100,"exclusiveMinimum":-1,"maximum":300,"exclusiveMaximum":200}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_exclusive_bounds_emits_f64() {
        let json = r#"{"type":"object","properties":{"n":{"type":"number","exclusiveMinimum":0,"exclusiveMaximum":1}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_huge_bound_emits_f64() {
        let json = r#"{"type":"object","properties":{"n":{"type":"number","minimum":0,"maximum":1e39}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_default_above_i64_max_is_exact() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":18446744073709551615,"default":18446744073709551615}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 18446744073709551615)]
    pub n: Option<u64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn n_or_default(&self) -> u64 {
        self.n.unwrap_or(18446744073709551615)
    }
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_only_minimum_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer","minimum":0}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_only_maximum_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"count":{"type":"integer","maximum":100}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_without_min_max_emits_f64_fallback() {
        let json =
            r#"{"type":"object","properties":{"value":{"type":"number"}},"required":["value"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_emits_f64() {
        let json = r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_property_required_emits_bool() {
        let json = r#"{"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub enabled: bool,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_property_optional_emits_option_bool() {
        let json = r#"{"type":"object","properties":{"flag":{"type":"boolean"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_property_with_description_emits_doc_and_field() {
        let json = r#"{"type":"object","properties":{"flag":{"type":"boolean","description":"Is it enabled?"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Is it enabled?
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_items_boolean_emits_vec_bool() {
        let json = r#"{"type":"object","properties":{"flags":{"type":"array","items":{"type":"boolean"}}},"required":["flags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub flags: Vec<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_items_boolean_unique_items_emits_vec_bool() {
        let json = r#"{"type":"object","properties":{"flags":{"type":"array","items":{"type":"boolean"},"uniqueItems":true}},"required":["flags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub flags: Vec<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_property_with_default_emits_accessor() {
        let json =
            r#"{"type":"object","properties":{"enabled":{"type":"boolean","default":true}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = true)]
    pub enabled: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn enabled_or_default(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_required_string_property() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}},"required":["tags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_unique_items_true_emits_vec_string() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_unique_items_false_emits_vec_string() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":false}},"required":["tags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "one full golden of the generated helper module"
    )]
    fn unique_items_hash_set_covers_every_field_shape_and_falls_back_to_vec() {
        let schema: Schema = Schema::try_from(r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true},"ids":{"type":"array","items":{"type":"integer"},"uniqueItems":true},"maybe":{"type":["array","null"],"items":{"type":"boolean"},"uniqueItems":true},"must":{"type":["array","null"],"items":{"type":"string"},"uniqueItems":true},"floats":{"type":"array","items":{"type":"number"},"uniqueItems":true},"nested":{"type":"array","items":{"type":"array","items":{"type":"string"},"uniqueItems":true}}},"required":["tags","must"]}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(UniqueItemsType::HashSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }

    pub(super) fn nullable<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Option::<Unique<S>>::deserialize(deserializer)
            .map(|value: Option<Unique<S>>| value.map(|unique: Unique<S>| unique.0))
    }

    pub(super) fn optional_nullable<'de, D, S>(
        deserializer: D,
    ) -> Result<json_schema_rs::Nullable<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        json_schema_rs::Nullable::<Unique<S>>::deserialize(deserializer)
            .map(|value: json_schema_rs::Nullable<Unique<S>>| value.map(|unique: Unique<S>| unique.0))
    }

    /// Reads integer items by JSON Schema's integer rule (`1.0` is the integer 1).
    struct IntegerVisitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for IntegerVisitor<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(json_schema_rs::integer::Integer(item)) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array of integers read into the set `S`.
    struct UniqueIntegers<S>(S);

    impl<'de, S> Deserialize<'de> for UniqueIntegers<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(IntegerVisitor(std::marker::PhantomData))
                .map(UniqueIntegers)
        }
    }

    pub(super) fn optional_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| Some(unique.0))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub floats: Option<Vec<f64>>,
    #[serde(default, deserialize_with = "unique_items::optional_integer", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub ids: Option<HashSet<i64>>,
    #[serde(default, deserialize_with = "unique_items::optional_nullable", skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    #[json_schema(unique_items)]
    pub maybe: json_schema_rs::Nullable<HashSet<bool>>,
    #[serde(deserialize_with = "unique_items::nullable")]
    #[json_schema(required, unique_items)]
    pub must: Option<HashSet<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested: Option<Vec<Vec<String>>>,
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub tags: HashSet<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_btree_set_emits_btree_set() {
        let schema: Schema = Schema::try_from(r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub tags: BTreeSet<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "indexmap")]
    #[test]
    fn unique_items_index_set_emits_index_set() {
        let schema: Schema = Schema::try_from(r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"]}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(UniqueItemsType::IndexSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use indexmap::IndexSet;
use std::collections::BTreeMap;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for indexmap::IndexSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub tags: IndexSet<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_unique_items_true_object_items_emits_vec() {
        let json = r#"{"type":"object","properties":{"items":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"}}},"uniqueItems":true}},"required":["items"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub items: Vec<Item>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_min_items_max_items_emits_attribute() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":5}},"required":["tags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(min_items = 2, max_items = 5)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_with_min_items_only_emits_attribute() {
        let json = r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"minItems":1}},"required":["tags"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(min_items = 1)]
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_optional_string_property() {
        let json =
            r#"{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_integers_property() {
        let json = r#"{"type":"object","properties":{"counts":{"type":"array","items":{"type":"integer"}}},"required":["counts"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub counts: Vec<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_objects_property() {
        let json = r#"{"type":"object","properties":{"items":{"type":"array","items":{"type":"object","properties":{"name":{"type":"string"}}}}},"required":["items"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub items: Vec<Item>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_of_arrays_property() {
        let json = r#"{"type":"object","properties":{"matrix":{"type":"array","items":{"type":"array","items":{"type":"string"}}}},"required":["matrix"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub matrix: Vec<Vec<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_string_integer_float_properties() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"},"count":{"type":"integer"},"value":{"type":"number"}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_string_and_integer_properties() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"},"count":{"type":"integer"}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
    pub id: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
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
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
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
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deeply_nested_schema_does_not_stack_overflow() {
        const DEPTH: usize = 150;
        let mut inner: JsonSchema = JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: {
                let mut m = std::collections::BTreeMap::new();
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
            title: Some("Leaf".to_string()),
            ..Default::default()
        };
        for i in (0..DEPTH).rev() {
            let mut wrap: JsonSchema = JsonSchema {
                type_: Some(JsonType::Object.into()),
                title: Some(format!("Level{i}")),
                ..Default::default()
            };
            wrap.properties.insert("child".to_string(), inner.into());
            inner = wrap;
        }
        let settings: CodeGenSettings = default_settings();
        let actual = generate_rust(&[Schema::from(inner)], &settings);
        assert!(actual.is_ok(), concat!("deep schema must not ", "overflow"));
        let out = actual.unwrap();
        let output: String = String::from_utf8(out.per_schema[0].clone()).unwrap();
        assert!(
            output.contains(concat!("pub struct ", "Level0")),
            concat!("output must contain root ", "struct")
        );
        assert!(
            output.contains(concat!("pub struct ", "Leaf")),
            concat!("output must contain leaf ", "struct")
        );
    }

    #[test]
    fn field_rename_when_key_differs_from_identifier() {
        let json = r#"{"type":"object","properties":{"foo-bar":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "foo-bar", skip_serializing_if = "Option::is_none")]
    pub foo_bar: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Three camelCase keys: one `rename_all` beside `deny_unknown_fields` replaces their
    /// renames.
    #[test]
    fn fields_one_rule_names_become_rename_all() {
        let json: &str = r#"{"type":"object","properties":{"logLevel":{"type":"string"},"maxRetries":{"type":"integer"},"retryDelayMs":{"type":"integer"}},"required":["logLevel"],"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Root {
    pub log_level: String,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<i64>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub retry_delay_ms: Option<i64>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// One camelCase key: its own `rename` is no longer than a `rename_all`, so it stays.
    #[test]
    fn single_renamed_field_keeps_its_own_rename() {
        let json: &str = r#"{"type":"object","properties":{"logLevel":{"type":"string"},"name":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "logLevel", skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Most keys camelCase: `rename_all` for them, and a `rename` for the key the rule does not
    /// give (`foo-bar`); the keyword field `type_` needs none, since camelCase drops its `_`.
    #[test]
    fn fields_the_rule_does_not_name_keep_their_own_rename() {
        let json: &str = r#"{"type":"object","properties":{"logLevel":{"type":"string"},"maxRetries":{"type":"integer"},"retryDelayMs":{"type":"integer"},"foo-bar":{"type":"string"},"type":{"type":"string"}},"required":["logLevel","maxRetries","retryDelayMs","foo-bar","type"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    #[serde(rename = "foo-bar")]
    pub foo_bar: String,
    pub log_level: String,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub max_retries: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub retry_delay_ms: i64,
    pub type_: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Values no serde rule gives from the variant names (`team_1` from `Team1`): one `rename`
    /// per variant, no `rename_all`.
    #[test]
    fn enum_values_no_rule_names_keep_their_own_renames() {
        let json: &str = r#"{"type":"object","properties":{"team":{"enum":["team_1","team_2","team_3"]}},"required":["team"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Team {
    #[serde(rename = "team_1")]
    Team1,
    #[serde(rename = "team_2")]
    Team2,
    #[serde(rename = "team_3")]
    Team3,
}

json_schema_rs::string_enum!(Team);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub team: Team,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `SCREAMING_SNAKE_CASE` values: one `rename_all`, and a `rename` for the value it does not
    /// give.
    #[test]
    fn enum_values_one_rule_names_become_rename_all() {
        let json: &str = r#"{"type":"object","properties":{"level":{"enum":["LOG_ENTRY","INFO","WARN","other"]}},"required":["level"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Level {
    Info,
    LogEntry,
    Warn,
    #[serde(rename = "other")]
    Other,
}

json_schema_rs::string_enum!(Level);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub level: Level,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A value whose variant name ends with its enum's name (`LogLevel` in `Level`) loses it,
    /// keeps its value through its own `rename`, and the others still share the `rename_all`.
    #[test]
    fn enum_value_named_after_its_enum_is_cut_while_rename_all_stays() {
        let json: &str = r#"{"type":"object","properties":{"level":{"enum":["LOG_LEVEL","INFO","WARN","ERROR"]}},"required":["level"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Level {
    Error,
    Info,
    #[serde(rename = "LOG_LEVEL")]
    Log,
    Warn,
}

json_schema_rs::string_enum!(Level);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub level: Level,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn generate_rust_equals_rust_backend_generate() {
        let json: &str =
            r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual: super::GenerateRustOutput =
            generate_rust(std::slice::from_ref(&schema), &settings).unwrap();
        let expected: super::GenerateRustOutput = RustBackend
            .generate(&CodeGenInput::from(&[schema]), &settings)
            .unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn property_key_first_uses_key_over_title_for_nested_struct() {
        let json = r#"{"type":"object","properties":{"address":{"type":"object","title":"FooBar","properties":{"city":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .model_name_source(ModelNameSource::PropertyKeyFirst)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "FooBar")]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn generate_rust_two_schemas_returns_two_buffers() {
        let json1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let json2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(json1).unwrap();
        let s2: Schema = serde_json::from_str(json2).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn batch_error_includes_index() {
        let valid: &str = r#"{"type":"object","properties":{"x":{"type":"string"}}}"#;
        let invalid: &str = r##"{"type":"object","properties":{"x":{"$ref":"#/$defs/Missing"}}}"##;
        let s0: Schema = serde_json::from_str(valid).unwrap();
        let s1: Schema = serde_json::from_str(invalid).unwrap();
        let settings: CodeGenSettings = default_settings();
        let actual: CodeGenError = generate_rust(&[s0, s1], &settings).unwrap_err();
        assert!(matches!(actual, CodeGenError::Batch { index: 1, .. }));
    }

    #[test]
    fn dedupe_disabled_returns_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema.clone(), schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_disabled_two_schemas_same_shape_two_buffers_no_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(json).unwrap();
        let s2: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_two_identical_schemas_produces_shared() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(json).unwrap();
        let s2: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_single_schema_no_shared() {
        let json = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_two_different_schemas_no_shared() {
        let j1 = r#"{"type":"object","properties":{"a":{"type":"string"}}}"#;
        let j2 = r#"{"type":"object","properties":{"b":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_static_embeds_a_custom_metaschema() {
        let meta: serde_json::Value = serde_json::json!({
            "$id": "https://example.com/meta",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$vocabulary": {
                "https://json-schema.org/draft/2020-12/vocab/core": true,
                "https://json-schema.org/draft/2020-12/vocab/applicator": true,
                "https://json-schema.org/draft/2020-12/vocab/validation": true,
                "https://json-schema.org/draft/2020-12/vocab/format-assertion": true
            },
            "$dynamicAnchor": "meta"
        });
        let schema: Schema = serde_json::from_str(
            r#"{"$schema":"https://example.com/meta","type":"object","properties":{"ip":{"type":"string","format":"ipv4"}}}"#,
        )
        .unwrap();
        let documents: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::from([("https://example.com/meta".to_string(), meta)]);
        let input: CodeGenInput<'_> = CodeGenInput::new().schema(&schema).retriever(documents);
        let output: super::GenerateRustOutput =
            generate_rust(input, &CodeGenSettings::builder().build()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Ip(std::net::Ipv4Addr);

json_schema_rs::constrained_newtype!(parsed, Ip, std::net::Ipv4Addr, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<Ip>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 1);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$schema":"https://example.com/meta","properties":{"ip":{"format":"ipv4","type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
        json_schema_rs::EmbeddedDocument {
            uri: "https://example.com/meta",
            json: r#"{"$id":"https://example.com/meta","$schema":"https://json-schema.org/draft/2020-12/schema","$vocabulary":{"https://json-schema.org/draft/2020-12/vocab/applicator":true,"https://json-schema.org/draft/2020-12/vocab/core":true,"https://json-schema.org/draft/2020-12/vocab/format-assertion":true,"https://json-schema.org/draft/2020-12/vocab/validation":true}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Loaded,
        },
    ],
    &[
        &["#/properties/ip"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    #[expect(clippy::too_many_lines)]
    fn dedupe_self_validating_identical_types_of_two_documents_are_shared() {
        let order: Schema = serde_json::from_str(r#"{"title":"Order","type":"object","properties":{"id":{"type":"string"},"address":{"title":"Address","type":"object","properties":{"street":{"type":"string","minLength":1}}}}}"#).unwrap();
        let customer: Schema = serde_json::from_str(r#"{"title":"Customer","type":"object","properties":{"name":{"type":"string"},"address":{"title":"Address","type":"object","properties":{"street":{"type":"string","minLength":1}}}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[order, customer], &CodeGenSettings::builder().build()).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 1)]
pub struct Street(String);

json_schema_rs::constrained_newtype!(string, Street, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Address")]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<Street>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Address, TYPE_VALIDATORS, 1);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"address":{"properties":{"street":{"minLength":1,"type":"string"}},"type":"object"},"id":{"type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/address/properties/street"],
        &["#/properties/address"],
    ],
);

"##
                .to_string(),
            ),
            vec![
                r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

pub use super::shared::Address;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Order")]
pub struct Order {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Order, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"address":{"properties":{"street":{"minLength":1,"type":"string"}},"type":"object"},"id":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##
                .to_string(),
                r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

pub use super::shared::Address;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Customer")]
pub struct Customer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Customer, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"address":{"properties":{"street":{"minLength":1,"type":"string"}},"type":"object"},"name":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_self_validating_identical_nested_objects_of_one_document_are_one_type() {
        let schema: Schema = serde_json::from_str(r#"{"type":"object","properties":{"home":{"type":"object","properties":{"street":{"type":"string","minLength":1}}},"work":{"type":"object","properties":{"street":{"type":"string","minLength":1}}}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &CodeGenSettings::builder().build()).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 1)]
pub struct Street(String);

json_schema_rs::constrained_newtype!(string, Street, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Home {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<Street>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Home, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<Home>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<Home>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 2);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"home":{"properties":{"street":{"minLength":1,"type":"string"}},"type":"object"},"work":{"properties":{"street":{"minLength":1,"type":"string"}},"type":"object"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/work/properties/street"],
        &["#/properties/home"],
        &["#"],
    ],
);

"##
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
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
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Addr1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addr1: Option<Addr1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addr2: Option<Addr1>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_root_struct_single_line() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"A root type"}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// A root type
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_root_struct_multi_line() {
        let json = r#"{"type":"object","properties":{"x":{"type":"string"}},"description":"Line one\nLine two"}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// Line one
/// Line two
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_empty_emits_no_doc() {
        let json: &str =
            r#"{"type":"object","properties":{"a":{"type":"string"}},"description":""}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_whitespace_only_emits_no_doc() {
        let json: &str =
            r#"{"type":"object","properties":{"a":{"type":"string"}},"description":"   \n  "}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_nested_object_struct_doc() {
        let json = r#"{"type":"object","properties":{"nested":{"type":"object","description":"Inner type","properties":{"v":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// Inner type
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Nested {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Inner type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested: Option<Nested>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_property_field_doc() {
        let json = r#"{"type":"object","properties":{"name":{"type":"string","description":"User full name"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// User full name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_enum_property_emits_enum_doc() {
        let json = r#"{"type":"object","properties":{"status":{"enum":["open","closed"],"description":"Issue status"}},"required":["status"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Issue status
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Closed,
    Open,
}

json_schema_rs::string_enum!(Status);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    /// Issue status
    pub status: Status,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_description_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"First"}"#;
        let j2 =
            r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"Second"}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// First
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_same_shape_different_description_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"First"}"#;
        let j2 =
            r#"{"type":"object","properties":{"id":{"type":"string"}},"description":"Second"}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// First
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// Second
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_comment_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"First"}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"Second"}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_same_shape_different_comment_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"First"}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"$comment":"Second"}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn description_markup_is_rendered_and_stated_for_the_round_trip() {
        let json: &str = r#"{"description":"Configures the WidgetPool.","type":"object","properties":{"max_retries":{"type":"integer","description":"Written as <n>."},"name":{"type":"string","description":"Plain text."}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// Configures the `WidgetPool`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "Configures the WidgetPool.")]
pub struct Root {
    /// Written as \<n\>.
    #[json_schema(description = "Written as <n>.")]
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<i64>,
    /// Plain text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn examples_golden_with_examples_emits_doc() {
        let json: &str = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":[{"x":"foo"}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// {"x":"foo"}
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub struct Root {
    pub x: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn examples_struct_doc_contains_examples() {
        let json: &str = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":["a",1]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// "a"
/// 1
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub struct Root {
    pub x: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn examples_enum_doc_contains_examples() {
        let json: &str = r#"{"type":"object","properties":{"status":{"enum":["open"],"examples":["open"]}},"required":["status"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// "open"
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub enum Status {
    #[serde(rename = "open")]
    Open,
}

json_schema_rs::string_enum!(Status);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub status: Status,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn examples_empty_array_emits_no_doc() {
        let json: &str = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":[]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub x: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_same_shape_different_examples_two_structs() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[1]}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[2]}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// 1
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// 2
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_examples_one_struct() {
        let j1 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[1]}"#;
        let j2 = r#"{"type":"object","properties":{"id":{"type":"string"}},"examples":[2]}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

/// # Examples
///
/// ```json
/// 1
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(description = "")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_full_same_shape_different_id_two_structs() {
        let j1 = r#"{"$id":"http://example.com/a","type":"object","properties":{"x":{"type":"string"}}}"#;
        let j2 = r#"{"$id":"http://example.com/b","type":"object","properties":{"x":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = "http://example.com/a")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = "http://example.com/b")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_shape_different_id_one_struct() {
        let j1 = r#"{"$id":"http://example.com/a","type":"object","properties":{"x":{"type":"string"}}}"#;
        let j2 = r#"{"$id":"http://example.com/b","type":"object","properties":{"x":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(j1).unwrap();
        let s2: Schema = serde_json::from_str(j2).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(id = "http://example.com/a")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
                r"//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dedupe_functional_same_as_full_when_no_description() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let s1: Schema = serde_json::from_str(json).unwrap();
        let s2: Schema = serde_json::from_str(json).unwrap();
        let settings_full: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Full)
            .build();
        let settings_func: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output_full: super::GenerateRustOutput =
            generate_rust(&[s1.clone(), s2.clone()], &settings_full).unwrap();
        let output_func: super::GenerateRustOutput =
            generate_rust(&[s1, s2], &settings_func).unwrap();
        let expected: super::GenerateRustOutput = output_full;
        let actual: super::GenerateRustOutput = output_func;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_settings_use_vec_for_unique_items() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected: UniqueItemsType = UniqueItemsType::Vec;
        let actual: UniqueItemsType = settings.unique_items;
        assert_eq!(expected, actual);
    }

    /// Single-entry `anyOf` / `oneOf` stay unions unless the flatten setting is turned on.
    #[test]
    fn default_settings_keep_single_entry_combinators() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected: bool = false;
        let actual: bool = settings.flatten_single_entry_combinators;
        assert_eq!(expected, actual);
    }

    /// The builder turns on flattening of single-entry `anyOf` / `oneOf`.
    #[test]
    fn builder_sets_flatten_single_entry_combinators() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let expected: bool = true;
        let actual: bool = settings.flatten_single_entry_combinators;
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_sets_unique_items() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let expected: UniqueItemsType = UniqueItemsType::BTreeSet;
        let actual: UniqueItemsType = settings.unique_items;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_settings_use_full_dedupe() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected = DedupeMode::Full;
        let actual = settings.dedupe_mode;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_required_property() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}},"required":["id"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: Uuid,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_optional_property() {
        let json = r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_array_items() {
        let json = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"type":"string","format":"uuid"}}},"required":["ids"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub ids: Vec<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// A root without `type` or keywords accepts any value: a type alias to `serde_json::Value`.
    #[test]
    fn root_without_keywords_emits_value_type_alias() {
        let json: &str = r"{}";
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = serde_json::Value;

";
        assert_eq!(expected, actual);
    }

    /// An object root without properties is a map of its additional properties (any value here).
    #[test]
    fn root_object_without_properties_emits_map_type_alias() {
        let json: &str = r#"{"type":"object"}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::BTreeMap;

pub type Root = BTreeMap<String, serde_json::Value>;

";
        assert_eq!(expected, actual);
    }

    /// A root whose type is not an object gets a type alias named `Root`.
    #[test]
    fn root_string_emits_type_alias() {
        let json: &str = r#"{"type":"string"}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

";
        assert_eq!(expected, actual);
    }

    /// A root object whose `additionalProperties` is a schema and that has no properties is a map of that schema.
    #[test]
    fn root_object_with_only_additional_properties_emits_map_type_alias() {
        let json: &str = r#"{"type":"object","additionalProperties":{"type":"integer"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::BTreeMap;

pub type Root = BTreeMap<String, i64>;

";
        assert_eq!(expected, actual);
    }

    /// An `allOf` of objects without properties is an object without properties: a map alias.
    #[test]
    fn root_all_of_objects_without_properties_emits_map_type_alias() {
        let json: &str = r#"{"allOf":[{"type":"object"},{"type":"object"}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::BTreeMap;

pub type Root = BTreeMap<String, serde_json::Value>;

";
        assert_eq!(expected, actual);
    }

    /// A `true` document accepts every value, like `{}`: the root is `serde_json::Value`.
    #[test]
    fn root_true_schema_emits_value_alias() {
        let schema: Schema = Schema::try_from("true").unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = "//! Generated by json-schema-rs. Do not edit manually.

pub type Root = serde_json::Value;

";
        assert_eq!(expected, actual);
    }

    /// A `false` document accepts no value: the root is an uninhabited enum.
    #[test]
    fn root_false_schema_emits_uninhabited_enum() {
        let schema: Schema = Schema::try_from("false").unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = "//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Root {
}

";
        assert_eq!(expected, actual);
    }

    /// Boolean documents next to an object schema in one batch: each gets its own root, and the
    /// object schema's output is unchanged.
    #[test]
    fn boolean_roots_in_a_batch_each_get_a_root() {
        let schemas: Vec<Schema> = vec![
            Schema::Bool(true),
            Schema::Bool(false),
            serde_json::from_str(r#"{"type":"string"}"#).unwrap(),
        ];
        let settings: CodeGenSettings = default_settings();
        let actual: (Option<String>, Vec<String>) =
            buffers(generate_rust(&schemas, &settings).unwrap());
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                "//! Generated by json-schema-rs. Do not edit manually.

pub type Root = serde_json::Value;

"
                .to_string(),
                "//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Root {
}

"
                .to_string(),
                "//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

"
                .to_string(),
            ],
        );
        assert_eq!(expected, actual);
    }

    /// An empty `anyOf` has no valid value: an uninhabited untagged enum.
    #[test]
    fn root_empty_any_of_emits_uninhabited_enum() {
        let json: &str = r#"{"anyOf":[]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
}

json_schema_rs::deserialize_untagged!(RootAnyOf {});

";
        assert_eq!(expected, actual);
    }

    /// An empty `oneOf` has no valid value: an uninhabited untagged enum marked `one_of`.
    #[test]
    fn root_empty_one_of_emits_uninhabited_enum() {
        let json: &str = r#"{"oneOf":[]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
}

json_schema_rs::deserialize_untagged!(RootOneOf {});

";
        assert_eq!(expected, actual);
    }

    /// A property whose `anyOf` is empty gets an uninhabited enum, so the field can only be absent.
    #[test]
    fn property_empty_any_of_emits_uninhabited_enum() {
        let json: &str = r#"{"type":"object","properties":{"a":{"anyOf":[]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum AAnyOf {
}

json_schema_rs::deserialize_untagged!(AAnyOf {});

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<AAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` branches with the same `const` keep it.
    #[test]
    fn all_of_same_const_emits_single_variant_enum() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string","const":"same"}}},{"type":"object","properties":{"x":{"type":"string","const":"same"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum X {
    #[serde(rename = "same")]
    Same,
}

json_schema_rs::string_enum!(X);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<X>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` branches with different `const` values have no common value: an uninhabited enum.
    #[test]
    fn all_of_conflicting_const_emits_uninhabited_enum() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string","const":"a"}}},{"type":"object","properties":{"x":{"type":"string","const":"b"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum X {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<X>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` branches with the same `pattern` keep it.
    #[test]
    fn all_of_same_pattern_keeps_pattern() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}},{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(pattern = "^[a-z]+$")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A property typed `string` in one `allOf` branch and `integer` in another has no valid value.
    #[test]
    fn all_of_conflicting_property_types_emits_uninhabited_enum() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"k":{"type":"string"}}},{"type":"object","properties":{"k":{"type":"integer"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum K {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<K>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A required property with no valid value makes the field's type uninhabited, not optional.
    #[test]
    fn all_of_conflicting_required_property_types_emits_required_uninhabited_field() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"k":{"type":"string"}},"required":["k"]},{"type":"object","properties":{"k":{"type":"integer"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum K {
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub k: K,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// An `allOf` of an object and a string has no valid value: the root is an uninhabited enum.
    #[test]
    fn all_of_object_and_string_emits_uninhabited_root() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"string"}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Root {
}

";
        assert_eq!(expected, actual);
    }

    /// `allOf` enums intersect: only the values in every branch remain.
    #[test]
    fn all_of_enums_emit_intersection() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"s":{"enum":["a","b","c"]}}},{"type":"object","properties":{"s":{"enum":["b","c","d"]}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum S {
    B,
    C,
}

json_schema_rs::string_enum!(S);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<S>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` enums without a common value give an uninhabited enum.
    #[test]
    fn all_of_disjoint_enums_emits_uninhabited_enum() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"s":{"enum":["a","b"]}}},{"type":"object","properties":{"s":{"enum":["x","y"]}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum S {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<S>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` integer bounds combine to the tightest range, which picks the integer width.
    #[test]
    fn all_of_integer_bounds_take_tightest_range() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":1000}}},{"type":"object","properties":{"n":{"type":"integer","minimum":10,"maximum":200}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<u8>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Disjoint `allOf` integer bounds leave no valid integer: an uninhabited enum.
    #[test]
    fn all_of_disjoint_integer_bounds_emits_uninhabited_enum() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":10}}},{"type":"object","properties":{"n":{"type":"integer","minimum":20,"maximum":30}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum N {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<N>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `allOf` bounds compare exactly beyond `f64` precision: `minimum: 2^53 + 1` and
    /// `maximum: 2^53` leave no valid integer (an uninhabited enum).
    #[test]
    fn all_of_integer_bounds_beyond_f64_precision_compare_exactly() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"n":{"type":"integer","minimum":9007199254740993}}},{"type":"object","properties":{"n":{"type":"integer","maximum":9007199254740992}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum N {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<N>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A struct estimated above 256 bytes (nine 32-byte `serde_json::Value` fields) is boxed where a
    /// field holds it, optional or not; one at the limit (eight) stays inline.
    #[test]
    fn struct_field_above_size_limit_is_boxed() {
        let json: &str = r##"{"type":"object","properties":{"big":{"$ref":"#/$defs/Big"},"fits":{"$ref":"#/$defs/Fits"},"maybe_big":{"$ref":"#/$defs/Big"}},"required":["big","fits"],"additionalProperties":false,"$defs":{"Big":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{},"f8":{}},"required":["f0","f1","f2","f3","f4","f5","f6","f7","f8"],"additionalProperties":false},"Fits":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{}},"required":["f0","f1","f2","f3","f4","f5","f6","f7"],"additionalProperties":false}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Big {
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
    pub f7: serde_json::Value,
    pub f8: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fits {
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
    pub f7: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub big: Box<Big>,
    pub fits: Fits,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maybe_big: Option<Box<Big>>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A union variant holding a struct above the size limit holds it boxed.
    #[test]
    fn union_variant_above_size_limit_is_boxed() {
        let json: &str = r##"{"anyOf":[{"$ref":"#/$defs/Big"},{"type":"string"}],"$defs":{"Big":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{},"f8":{}},"required":["f0","f1","f2","f3","f4","f5","f6","f7","f8"],"additionalProperties":false}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Big {
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
    pub f7: serde_json::Value,
    pub f8: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Big(Box<Big>),
    String(String),
}

json_schema_rs::deserialize_untagged!(RootAnyOf { Big(Box<Big>), String(String) });

";
        assert_eq!(expected, actual);
    }

    /// A map's values live on the heap already, so a large value type is not boxed.
    #[test]
    fn map_value_above_size_limit_is_not_boxed() {
        let json: &str = r##"{"type":"object","additionalProperties":{"$ref":"#/$defs/Big"},"$defs":{"Big":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{},"f8":{}},"required":["f0","f1","f2","f3","f4","f5","f6","f7","f8"],"additionalProperties":false}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Big {
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
    pub f7: serde_json::Value,
    pub f8: serde_json::Value,
}

pub type Root = BTreeMap<String, Big>;

";
        assert_eq!(expected, actual);
    }

    /// A boxed field counts as a pointer: `Mid` holds `Big` boxed and seven values (232 bytes), so
    /// `Root` holds `Mid` inline.
    #[test]
    fn boxed_child_counts_as_a_pointer_in_its_parent() {
        let json: &str = r##"{"type":"object","properties":{"mid":{"$ref":"#/$defs/Mid"}},"required":["mid"],"additionalProperties":false,"$defs":{"Big":{"type":"object","properties":{"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{},"f7":{},"f8":{}},"required":["f0","f1","f2","f3","f4","f5","f6","f7","f8"],"additionalProperties":false},"Mid":{"type":"object","properties":{"big":{"$ref":"#/$defs/Big"},"f0":{},"f1":{},"f2":{},"f3":{},"f4":{},"f5":{},"f6":{}},"required":["big","f0","f1","f2","f3","f4","f5","f6"],"additionalProperties":false}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Big {
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
    pub f7: serde_json::Value,
    pub f8: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mid {
    pub big: Box<Big>,
    pub f0: serde_json::Value,
    pub f1: serde_json::Value,
    pub f2: serde_json::Value,
    pub f3: serde_json::Value,
    pub f4: serde_json::Value,
    pub f5: serde_json::Value,
    pub f6: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub mid: Mid,
}

";
        assert_eq!(expected, actual);
    }

    /// A root that refers to itself through `#` gets `Box` on the by-value back-edge.
    #[test]
    fn self_reference_through_root_boxes_back_edge() {
        let json: &str = r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"}},"required":["value"]}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A definition that refers to itself gets `Box` on the by-value back-edge.
    #[test]
    fn self_reference_through_defs_boxes_back_edge() {
        let json: &str = r##"{"type":"object","properties":{"head":{"$ref":"#/$defs/Node"}},"$defs":{"Node":{"type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#/$defs/Node"}},"required":["value"]}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Node>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Mutually recursive definitions box exactly one edge of the cycle.
    #[test]
    fn mutual_recursion_boxes_one_edge() {
        let json: &str = r##"{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}},"$defs":{"A":{"type":"object","properties":{"b":{"$ref":"#/$defs/B"}}},"B":{"type":"object","properties":{"a":{"$ref":"#/$defs/A"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct A {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Box<B>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct B {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<A>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A recursive `Vec` needs no `Box` (the vector already allocates); the `Option` back-edge does.
    #[test]
    fn self_reference_in_array_items_is_not_boxed() {
        let json: &str = r##"{"title":"Node","type":"object","properties":{"value":{"type":"integer"},"next":{"$ref":"#"},"children":{"type":"array","items":{"$ref":"#"}}},"required":["value"]}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An object root whose `allOf` is one `$ref` is the referenced definition: the root is
    /// structurally identical to it and takes the definition's name.
    #[test]
    fn all_of_single_ref_merges_root_into_definition() {
        let json: &str = r##"{"type":"object","allOf":[{"$ref":"#/definitions/base"}],"definitions":{"base":{"type":"object","properties":{"a":{"type":"string"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Base {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A root structurally identical to a `$defs` entry is emitted once, under the definition's name.
    #[test]
    fn root_identical_to_definition_takes_definition_name() {
        let json: &str = r#"{"type":"object","properties":{"a":{"type":"string"}},"$defs":{"Thing":{"type":"object","properties":{"a":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Thing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// From 2019-09 on, keywords next to `$ref` combine with its target: the property's struct has
    /// both the target's and its own properties.
    #[test]
    fn ref_with_sibling_properties_merges_into_one_struct() {
        let json: &str = r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"p":{"$ref":"#/$defs/Base","properties":{"extra":{"type":"integer"}},"required":["extra"]}},"$defs":{"Base":{"type":"object","properties":{"a":{"type":"string"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct P {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub extra: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<P>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Base {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Two different nested objects with the same title both keep it, each with a numeric suffix.
    #[test]
    fn colliding_struct_names_get_numeric_suffixes() {
        let json: &str = r#"{"type":"object","properties":{"first":{"title":"Item","type":"object","properties":{"a":{"type":"string"}}},"second":{"title":"Item","type":"object","properties":{"b":{"type":"integer"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item1 {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<Item0>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second: Option<Item1>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A name taken by std or the generated imports (`String`) gets a numeric suffix.
    #[test]
    fn colliding_property_types_are_qualified_by_their_parent() {
        let schema: Schema = serde_json::from_str(r#"{"type":"object","properties":{"point":{"title":"Point","type":"object","properties":{"kind":{"enum":["a","b"]}}},"line":{"title":"Line","type":"object","properties":{"kind":{"enum":["c","d"]}}}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    C,
    D,
}

json_schema_rs::string_enum!(LineKind);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Line")]
pub struct Line {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<LineKind>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    A,
    B,
}

json_schema_rs::string_enum!(PointKind);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Point")]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<PointKind>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<Line>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn union_variants_are_named_after_what_tells_them_apart() {
        let schema: Schema = serde_json::from_str(r#"{"type":"object","properties":{"auth":{"oneOf":[{"type":"object","properties":{"key":{"type":"string"}},"required":["key"],"additionalProperties":false},{"type":"object","properties":{"role":{"type":"string"}},"required":["role"],"additionalProperties":false},{"const":"anonymous"}]}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &CodeGenSettings::builder().build()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthKey {
    pub key: String,
}

json_schema_rs::self_validating!(AuthKey, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuthRole {
    pub role: String,
}

json_schema_rs::self_validating!(AuthRole, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum AuthAnonymous {
    #[serde(rename = "anonymous")]
    Anonymous,
}

json_schema_rs::string_enum!(AuthAnonymous);

json_schema_rs::self_validating!(AuthAnonymous, TYPE_VALIDATORS, 2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum AuthOneOf {
    Variant0(AuthKey),
    Variant1(AuthRole),
    Variant2(AuthAnonymous),
}

json_schema_rs::deserialize_union!(AuthOneOf, TYPE_VALIDATORS, OneOf, Some(3), [0, 1, 2], |value| {
    0 => <AuthKey as Deserialize>::deserialize(value).map(Self::Variant0),
    1 => <AuthRole as Deserialize>::deserialize(value).map(Self::Variant1),
    _ => <AuthAnonymous as Deserialize>::deserialize(value).map(Self::Variant2),
});

json_schema_rs::self_validating!(AuthOneOf, TYPE_VALIDATORS, 3);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 4);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"auth":{"oneOf":[{"additionalProperties":false,"properties":{"key":{"type":"string"}},"required":["key"],"type":"object"},{"additionalProperties":false,"properties":{"role":{"type":"string"}},"required":["role"],"type":"object"},{"const":"anonymous"}]}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/auth/oneOf/0"],
        &["#/properties/auth/oneOf/1"],
        &["#/properties/auth/oneOf/2"],
        &["#/properties/auth"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn reserved_struct_name_is_qualified_by_its_parent() {
        let json: &str = r#"{"type":"object","properties":{"s":{"title":"String","type":"object","properties":{"a":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "String")]
pub struct RootString {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<RootString>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Two keys that sanitize to the same field name both get a numeric suffix and keep their JSON key.
    #[test]
    fn colliding_field_names_get_numeric_suffixes() {
        let json: &str = r#"{"type":"object","properties":{"fooBar":{"type":"string"},"foo_bar":{"type":"integer"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "fooBar", skip_serializing_if = "Option::is_none")]
    pub foo_bar0: Option<String>,
    #[serde(rename = "foo_bar", default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub foo_bar1: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `anyOf` branches that are `$ref`s name their variants after the target types.
    #[test]
    fn any_of_ref_branches_name_variants_after_targets() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"anyOf":[{"$ref":"#/$defs/Cat"},{"$ref":"#/$defs/Dog"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}},"Dog":{"type":"object","properties":{"bark":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
    Dog(Dog),
}

json_schema_rs::deserialize_untagged!(PetAnyOf { Cat(Cat), Dog(Dog) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `oneOf` branches that are `$ref`s name their variants after the target types; the enum is
    /// marked `one_of`.
    #[test]
    fn one_of_ref_branches_name_variants_after_targets() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"oneOf":[{"$ref":"#/$defs/Cat"},{"$ref":"#/$defs/Dog"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}},"Dog":{"type":"object","properties":{"bark":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum PetOneOf {
    Cat(Cat),
    Dog(Dog),
}

json_schema_rs::deserialize_untagged!(PetOneOf { Cat(Cat), Dog(Dog) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// By default a root `anyOf` with one entry is a one-variant untagged union.
    #[test]
    fn flatten_disabled_root_single_any_of_emits_one_variant_union() {
        let json: &str = r#"{"anyOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Variant0(RootObject),
}

json_schema_rs::deserialize_untagged!(RootAnyOf { Variant0(RootObject) });

"#;
        assert_eq!(expected, actual);
    }

    /// With `flatten_single_entry_combinators` on, a root `anyOf` with one entry is the entry's
    /// struct.
    #[test]
    fn flatten_enabled_root_single_any_of_emits_entry_struct() {
        let json: &str = r#"{"anyOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// By default a root `oneOf` with one entry is a one-variant untagged union marked `one_of`.
    #[test]
    fn flatten_disabled_root_single_one_of_emits_one_variant_union() {
        let json: &str = r#"{"oneOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
    Variant0(RootObject),
}

json_schema_rs::deserialize_untagged!(RootOneOf { Variant0(RootObject) });

"#;
        assert_eq!(expected, actual);
    }

    /// With `flatten_single_entry_combinators` on, a root `oneOf` with one entry is the entry's
    /// struct.
    #[test]
    fn flatten_enabled_root_single_one_of_emits_entry_struct() {
        let json: &str = r#"{"oneOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A root `allOf` with one entry is the entry's struct when the flatten setting is off.
    #[test]
    fn flatten_disabled_root_single_all_of_emits_entry_struct() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A root `allOf` with one entry is the entry's struct when the flatten setting is on.
    #[test]
    fn flatten_enabled_root_single_all_of_emits_entry_struct() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"a":{"type":"string"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// By default a property `anyOf` with one `$ref` entry is a one-variant union over the
    /// definition.
    #[test]
    fn flatten_disabled_property_single_any_of_ref_emits_one_variant_union() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"anyOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
}

json_schema_rs::deserialize_untagged!(PetAnyOf { Cat(Cat) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With `flatten_single_entry_combinators` on, a property `anyOf` with one `$ref` entry is the
    /// definition's type.
    #[test]
    fn flatten_enabled_property_single_any_of_ref_emits_definition_type() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"anyOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<Cat>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// By default a property `oneOf` with one `$ref` entry is a one-variant union marked `one_of`.
    #[test]
    fn flatten_disabled_property_single_one_of_ref_emits_one_variant_union() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"oneOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum PetOneOf {
    Cat(Cat),
}

json_schema_rs::deserialize_untagged!(PetOneOf { Cat(Cat) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With `flatten_single_entry_combinators` on, a property `oneOf` with one `$ref` entry is the
    /// definition's type.
    #[test]
    fn flatten_enabled_property_single_one_of_ref_emits_definition_type() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"oneOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<Cat>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A property `allOf` with one `$ref` entry is the definition's type when the flatten setting
    /// is off.
    #[test]
    fn flatten_disabled_property_single_all_of_ref_emits_definition_type() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"allOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<Cat>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A property `allOf` with one `$ref` entry is the definition's type when the flatten setting
    /// is on.
    #[test]
    fn flatten_enabled_property_single_all_of_ref_emits_definition_type() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"allOf":[{"$ref":"#/$defs/Cat"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<Cat>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With the flatten setting on, an `anyOf` whose one entry is `true` constrains nothing:
    /// `serde_json::Value`.
    #[test]
    fn flatten_enabled_single_true_any_of_entry_emits_value() {
        let json: &str = r#"{"type":"object","properties":{"p":{"anyOf":[true]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub p: Option<serde_json::Value>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With the flatten setting on, a `oneOf` whose one entry is `false` is unsatisfiable: an enum
    /// with no variants.
    #[test]
    fn flatten_enabled_single_false_one_of_entry_emits_uninhabited_enum() {
        let json: &str = r#"{"type":"object","properties":{"p":{"oneOf":[false]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum P {
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<P>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With the flatten setting on, a one-entry `anyOf` around a one-entry `oneOf` collapses to the
    /// innermost entry's type.
    #[test]
    fn flatten_enabled_nested_single_entry_combinators_collapse_to_entry_type() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"anyOf":[{"oneOf":[{"$ref":"#/$defs/Cat"}]}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<Cat>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With the flatten setting on, a one-entry `anyOf` combines with its owner's own keywords into
    /// one struct.
    #[test]
    fn flatten_enabled_single_any_of_entry_merges_with_owner_keywords() {
        let json: &str = r#"{"type":"object","properties":{"a":{"type":"string"}},"anyOf":[{"properties":{"b":{"type":"integer"}},"required":["b"]}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub b: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The flatten setting leaves an `anyOf` with more than one entry as a union.
    #[test]
    fn flatten_enabled_multi_entry_any_of_stays_union() {
        let json: &str = r##"{"type":"object","properties":{"pet":{"anyOf":[{"$ref":"#/$defs/Cat"},{"$ref":"#/$defs/Dog"}]}},"$defs":{"Cat":{"type":"object","properties":{"meow":{"type":"boolean"}}},"Dog":{"type":"object","properties":{"bark":{"type":"boolean"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
    Dog(Dog),
}

json_schema_rs::deserialize_untagged!(PetAnyOf { Cat(Cat), Dog(Dog) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The flatten setting leaves a two-entry `anyOf` of a type and `null` as a nullable field.
    #[test]
    fn flatten_enabled_nullable_any_of_stays_nullable() {
        let json: &str = r#"{"type":"object","properties":{"name":{"anyOf":[{"type":"string"},{"type":"null"}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub name: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With the flatten setting on, a one-entry `anyOf` takes its entry's `title` as the type name.
    #[test]
    fn flatten_enabled_single_any_of_entry_title_names_type() {
        let json: &str = r#"{"type":"object","properties":{"p":{"anyOf":[{"title":"Named","type":"object","properties":{"x":{"type":"string"}}}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Named")]
pub struct Named {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<Named>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A required `anyOf` of a type and `null` is a required nullable field: `Option<T>` that must be present.
    #[test]
    fn required_any_of_type_and_null_emits_required_nullable_field() {
        let json: &str = r#"{"type":"object","properties":{"name":{"anyOf":[{"type":"string"},{"type":"null"}]}},"required":["name"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "Option::deserialize")]
    #[json_schema(required)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An optional `oneOf` of `null` and a type is an optional nullable field: `Nullable<T>`.
    #[test]
    fn optional_one_of_null_and_type_emits_optional_nullable_field() {
        let json: &str = r#"{"type":"object","properties":{"name":{"oneOf":[{"type":"null"},{"type":"string"}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub name: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A type array with several non-null types is an untagged enum, `integer` before `number`.
    #[test]
    fn type_array_union_orders_integer_before_number() {
        let json: &str = r#"{"type":"object","properties":{"v":{"type":["string","number","integer","boolean"]}},"required":["v"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum V {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
}

json_schema_rs::deserialize_untagged!(V { Boolean(bool), Integer(i64 = with(json_schema_rs::integer::deserialize)), Number(f64), String(String) });

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub v: V,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// An integer enum is a unit enum whose serde impls compare values by JSON equality.
    #[test]
    fn integer_enum_emits_value_enum_with_json_equal_impls() {
        let json: &str =
            r#"{"type":"object","properties":{"level":{"enum":[1,2,3]}},"required":["level"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum Level {
    #[json_schema(value = 1)]
    N1,
    #[json_schema(value = 2)]
    N2,
    #[json_schema(value = 3)]
    N3,
}

impl Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::N1 => serde_json::json!(1),
            Self::N2 => serde_json::json!(2),
            Self::N3 => serde_json::json!(3),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(2)) {
            return Ok(Self::N2);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(3)) {
            return Ok(Self::N3);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "1, 2, 3")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub level: Level,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An integer `const` is a single-variant value enum.
    #[test]
    fn const_integer_emits_single_variant_value_enum() {
        let json: &str =
            r#"{"type":"object","properties":{"version":{"const":2}},"required":["version"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum Version {
    #[json_schema(value = 2)]
    N2,
}

impl Serialize for Version {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::N2 => serde_json::json!(2),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!(2)) {
            return Ok(Self::N2);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "2")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub version: Version,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A required `["string", "null"]` property must be present but may be `null`.
    #[test]
    fn required_property_type_string_or_null_emits_required_nullable_field() {
        let json: &str = r#"{"type":"object","properties":{"name":{"type":["string","null"]}},"required":["name"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "Option::deserialize")]
    #[json_schema(required)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Properties next to an `additionalProperties` schema: the extra keys go to a flattened map.
    #[test]
    fn properties_with_additional_properties_schema_emits_flattened_map() {
        let json: &str = r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":{"type":"integer"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: String,
    #[serde(flatten, deserialize_with = "json_schema_rs::integer::deserialize")]
    pub additional: BTreeMap<String, i64>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An object property with no properties and an `additionalProperties` schema is a map.
    #[test]
    fn object_property_with_only_additional_properties_emits_map() {
        let json: &str = r#"{"type":"object","properties":{"labels":{"type":"object","additionalProperties":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<BTreeMap<String, String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A required property without keywords is any JSON value.
    #[test]
    fn required_typeless_property_emits_value() {
        let json: &str = r#"{"type":"object","properties":{"data":{}},"required":["data"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub data: serde_json::Value,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// An optional property without keywords is any JSON value, `null` included: one `Option` for
    /// absence, read through `deserialize_some`.
    #[test]
    fn optional_typeless_property_emits_option_value_with_deserialize_some() {
        let json: &str = r#"{"type":"object","properties":{"data":{}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A key only `then` defines takes the base's `additionalProperties` type: when `if` fails, that is
    /// all that constrains it.
    #[test]
    fn then_only_property_takes_additional_properties_type() {
        let json: &str = r#"{"type":"object","additionalProperties":{"type":"integer"},"if":{"required":["k"]},"then":{"properties":{"x":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten, deserialize_with = "json_schema_rs::integer::deserialize")]
    pub additional: BTreeMap<String, i64>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A key only `then` defines is an additional property of a base that forbids them: never valid,
    /// so it is no field.
    #[test]
    fn then_only_property_is_no_field_when_additional_properties_false() {
        let json: &str = r#"{"type":"object","properties":{"k":{"type":"string"}},"additionalProperties":false,"if":{"required":["k"]},"then":{"properties":{"x":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A key matching a `patternProperties` pattern is not checked against `additionalProperties`, so
    /// the map of unnamed keys holds any value.
    #[test]
    fn pattern_properties_beside_additional_properties_schema_keep_value_map() {
        let json: &str = r#"{"type":"object","properties":{"id":{"type":"string"}},"patternProperties":{"^x-":{}},"additionalProperties":{"type":"string"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With `patternProperties`, `additionalProperties: false` still allows the keys a pattern
    /// matches, so the struct keeps a map instead of denying unknown fields.
    #[test]
    fn pattern_properties_beside_additional_properties_false_keep_unnamed_keys() {
        let json: &str = r#"{"type":"object","properties":{"id":{"type":"string"}},"patternProperties":{"^x-":{}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An empty `patternProperties` matches no key: `additionalProperties: false` still denies unknown
    /// fields.
    #[test]
    fn empty_pattern_properties_keep_additional_properties_false() {
        let json: &str = r#"{"type":"object","properties":{"id":{"type":"string"}},"patternProperties":{},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A key the schema does not name, with one pattern and `additionalProperties: false`, takes the
    /// pattern's schema.
    #[test]
    fn pattern_property_types_unnamed_keys() {
        let json: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"patternProperties":{"^x-":{"type":"integer"}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten, deserialize_with = "json_schema_rs::integer::deserialize")]
    pub additional: BTreeMap<String, i64>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// With two patterns and no `additionalProperties`, an unnamed key may take any value.
    #[test]
    fn two_pattern_properties_without_additional_properties_keep_value_map() {
        let json: &str = r#"{"type":"object","patternProperties":{"^x-":{"type":"integer"},"^y-":{"type":"string"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::BTreeMap;

pub type Root = BTreeMap<String, serde_json::Value>;

";
        assert_eq!(expected, actual);
    }

    /// With two patterns and `additionalProperties: false`, an unnamed key takes the `anyOf` of the
    /// patterns' schemas.
    #[test]
    fn two_pattern_properties_type_unnamed_keys_as_any_of() {
        let json: &str = r#"{"type":"object","patternProperties":{"^x-":{"type":"integer"},"^y-":{"type":"string"}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootValueAnyOf {
    Integer(i64),
    String(String),
}

json_schema_rs::deserialize_untagged!(RootValueAnyOf { Integer(i64 = with(json_schema_rs::integer::deserialize)), String(String) });

pub type Root = BTreeMap<String, RootValueAnyOf>;

";
        assert_eq!(expected, actual);
    }

    /// A pattern beside an `additionalProperties` schema: an unnamed key takes the `anyOf` of both, and
    /// a `false` pattern adds nothing.
    #[test]
    fn pattern_property_and_additional_properties_schema_type_unnamed_keys_as_any_of() {
        let json: &str = r#"{"type":"object","patternProperties":{"^x-":{"type":"integer"},"^y-":false},"additionalProperties":{"type":"string"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootValueAnyOf {
    Integer(i64),
    String(String),
}

json_schema_rs::deserialize_untagged!(RootValueAnyOf { Integer(i64 = with(json_schema_rs::integer::deserialize)), String(String) });

pub type Root = BTreeMap<String, RootValueAnyOf>;

";
        assert_eq!(expected, actual);
    }

    /// Patterns that are all `false`, beside `additionalProperties: false`, allow no unnamed key.
    #[test]
    fn false_pattern_properties_beside_additional_properties_false_deny_unknown_fields() {
        let json: &str = r#"{"type":"object","properties":{"a":{"type":"string"}},"patternProperties":{"^x-":false},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A pattern's schema is read in its document's draft: draft 4's boolean `exclusiveMinimum`.
    #[test]
    fn draft4_pattern_property_reads_its_draft() {
        let json: &str = r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"a":{"type":"string"}},"patternProperties":{"^n":{"type":"number","minimum":0,"exclusiveMinimum":true}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, f64>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A pattern alone types the object as a map of unnamed keys, without `type: "object"`.
    #[test]
    fn pattern_property_alone_types_map() {
        let json: &str =
            r#"{"patternProperties":{"^x-":{"type":"boolean"}},"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::BTreeMap;

pub type Root = BTreeMap<String, bool>;

";
        assert_eq!(expected, actual);
    }

    /// In 2020-12, `items` applies only after `prefixItems`, so it does not type every element.
    #[test]
    fn prefix_items_beside_items_keep_value_elements() {
        let json: &str = r#"{"type":"object","properties":{"row":{"type":"array","prefixItems":[{"type":"integer"}],"items":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<Vec<serde_json::Value>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Draft 7 has no `prefixItems` keyword, so `items` applies to every element.
    #[test]
    fn prefix_items_in_draft_7_leave_items_typing_elements() {
        let json: &str = r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","properties":{"row":{"type":"array","prefixItems":[{"type":"integer"}],"items":{"type":"string"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<Vec<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Integer `uniqueItems` sets read their items by the integer rule (`1.0` is 1), in every field
    /// shape, through the `unique_items::*_integer` helpers.
    #[test]
    #[expect(clippy::too_many_lines)]
    fn integer_unique_items_sets_read_items_by_integer_rule() {
        let json: &str = r#"{"type":"object","properties":{"req":{"type":"array","items":{"type":"integer"},"uniqueItems":true},"opt":{"type":"array","items":{"type":"integer"},"uniqueItems":true},"req_null":{"type":["array","null"],"items":{"type":"integer"},"uniqueItems":true},"opt_null":{"type":["array","null"],"items":{"type":"integer"},"uniqueItems":true}},"required":["req","req_null"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    /// Reads integer items by JSON Schema's integer rule (`1.0` is the integer 1).
    struct IntegerVisitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for IntegerVisitor<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(json_schema_rs::integer::Integer(item)) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array of integers read into the set `S`.
    struct UniqueIntegers<S>(S);

    impl<'de, S> Deserialize<'de> for UniqueIntegers<S>
    where
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(IntegerVisitor(std::marker::PhantomData))
                .map(UniqueIntegers)
        }
    }

    pub(super) fn required_integer<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| unique.0)
    }

    pub(super) fn optional_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| Some(unique.0))
    }

    pub(super) fn nullable_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        Option::<UniqueIntegers<S>>::deserialize(deserializer)
            .map(|value: Option<UniqueIntegers<S>>| value.map(|unique: UniqueIntegers<S>| unique.0))
    }

    pub(super) fn optional_nullable_integer<'de, D, S>(
        deserializer: D,
    ) -> Result<json_schema_rs::Nullable<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: json_schema_rs::integer::DeserializeInteger<'de>,
    {
        json_schema_rs::Nullable::<UniqueIntegers<S>>::deserialize(deserializer).map(
            |value: json_schema_rs::Nullable<UniqueIntegers<S>>| value.map(|unique: UniqueIntegers<S>| unique.0),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "unique_items::optional_integer", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub opt: Option<BTreeSet<i64>>,
    #[serde(default, deserialize_with = "unique_items::optional_nullable_integer", skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    #[json_schema(unique_items)]
    pub opt_null: json_schema_rs::Nullable<BTreeSet<i64>>,
    #[serde(deserialize_with = "unique_items::required_integer")]
    #[json_schema(unique_items)]
    pub req: BTreeSet<i64>,
    #[serde(deserialize_with = "unique_items::nullable_integer")]
    #[json_schema(required, unique_items)]
    pub req_null: Option<BTreeSet<i64>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Draft 4 counts only numbers written as integers, which is serde's own integer rule: no
    /// `json_schema_rs::integer` reading.
    #[test]
    fn draft_4_integers_read_as_serde_integers() {
        let json: &str = r#"{"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"n":{"type":"integer"},"list":{"type":"array","items":{"type":"integer"}}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<i64>>,
    pub n: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `FormatTypes::Typed` (the default) maps `format: "uuid"` to `Uuid` even where 2020-12 only
    /// annotates formats.
    #[cfg(feature = "uuid")]
    #[test]
    fn format_types_typed_maps_annotated_uuid_to_uuid() {
        let json: &str =
            r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `FormatTypes::FollowDialect` keeps an annotated `format: "uuid"` a `String`.
    #[cfg(feature = "uuid")]
    #[test]
    fn format_types_follow_dialect_keeps_annotated_uuid_a_string() {
        let json: &str =
            r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .format_types(FormatTypes::FollowDialect)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `FormatTypes::FollowDialect` maps `format: "uuid"` to `Uuid` where the dialect asserts formats
    /// (`format_mode` set to `Assertion`).
    #[cfg(feature = "uuid")]
    #[test]
    fn format_types_follow_dialect_maps_asserted_uuid_to_uuid() {
        use crate::json_schema::{FormatMode, JsonSchemaSettings};

        let json: &str =
            r#"{"type":"object","properties":{"id":{"type":"string","format":"uuid"}}}"#;
        let schema_settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .format_mode(FormatMode::Assertion)
            .build();
        let schema: Schema = Schema::new_from_str(json, &schema_settings).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .format_types(FormatTypes::FollowDialect)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `date-time` and `date` are `chrono` types (full paths, no import) whatever the format
    /// mode, with `FormatTypes::Typed`.
    #[cfg(feature = "chrono")]
    #[test]
    fn chrono_formats_are_chrono_types() {
        let json: &str = r#"{"type":"object","properties":{"at":{"type":"string","format":"date-time"},"days":{"type":"array","items":{"type":"string","format":"date"}},"on":{"type":"string","format":"date"}},"required":["at"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub at: chrono::DateTime<chrono::FixedOffset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<Vec<chrono::NaiveDate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<chrono::NaiveDate>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `ipv4` / `ipv6` are the `std::net` address types (full paths, no feature) whatever the
    /// format mode, with `FormatTypes::Typed`.
    #[test]
    fn ip_formats_are_std_net_types() {
        let json: &str = r#"{"type":"object","properties":{"v4":{"type":"string","format":"ipv4"},"v6":{"type":"string","format":"ipv6"}},"required":["v4"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub v4: std::net::Ipv4Addr,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v6: Option<std::net::Ipv6Addr>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A schema's own `$defs` named `DateTime` / `Ipv4Addr` keep their names: the typed formats
    /// are spelled as full paths, so they reserve nothing.
    #[test]
    fn typed_formats_reserve_no_names() {
        let json: &str = r##"{"type":"object","properties":{"a":{"$ref":"#/$defs/Ipv4Addr"},"b":{"type":"string","format":"ipv4"}},"$defs":{"Ipv4Addr":{"type":"object","properties":{"x":{"type":"integer"}}}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Ipv4Addr {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Ipv4Addr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<std::net::Ipv4Addr>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `FormatTypes::FollowDialect` keeps an annotated `format: "ipv4"` a `String`.
    #[test]
    fn format_types_follow_dialect_keeps_annotated_ipv4_a_string() {
        let json: &str =
            r#"{"type":"object","properties":{"ip":{"type":"string","format":"ipv4"}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .format_types(FormatTypes::FollowDialect)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Where the dialect asserts a typed format (draft 7), the field is a `parsed` newtype: the
    /// validator checks the string before `Ipv4Addr` parses it (the parser alone is looser than
    /// the format).
    #[test]
    fn asserted_ipv4_is_a_parsed_newtype() {
        let json: &str = r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","properties":{"ip":{"type":"string","format":"ipv4"}},"required":["ip"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().self_validating(false).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Ip(std::net::Ipv4Addr);

json_schema_rs::constrained_newtype!(parsed, Ip, std::net::Ipv4Addr, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub ip: Ip,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$schema":"http://json-schema.org/draft-07/schema#","properties":{"ip":{"format":"ipv4","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/ip"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// A set of a typed format stays a `Vec`: different strings parse to equal values (`Uuid`
    /// folds case; `chrono` compares instants), so a set would reject arrays `uniqueItems`
    /// accepts.
    #[test]
    fn unique_typed_format_items_stay_a_vec() {
        let json: &str =
            r#"{"type":"array","items":{"type":"string","format":"ipv6"},"uniqueItems":true}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = Vec<std::net::Ipv6Addr>;

";
        assert_eq!(expected, actual);
    }

    /// `then` / `else` properties become optional fields; a key in both is the `anyOf` of the two,
    /// and a key in one only is any value (the other branch does not constrain it). The `if`
    /// schema itself adds no field.
    #[test]
    fn then_else_properties_emit_optional_fields() {
        let json: &str = r#"{"type":"object","properties":{"kind":{"type":"string"}},"if":{"properties":{"kind":{"const":"a"}}},"then":{"properties":{"a":{"type":"string"},"shared":{"type":"string"}}},"else":{"properties":{"b":{"type":"integer"},"shared":{"type":"integer"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum SharedAnyOf {
    String(String),
    Integer(i64),
}

json_schema_rs::deserialize_untagged!(SharedAnyOf { String(String), Integer(i64 = with(json_schema_rs::integer::deserialize)) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub a: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub b: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared: Option<SharedAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `not` does not change the generated type.
    #[test]
    fn not_is_ignored_for_the_type() {
        let json: &str =
            r#"{"type":"object","properties":{"a":{"type":"string","not":{"const":"x"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A `$defs` entry nothing refers to is still emitted, after the root.
    #[test]
    fn unreferenced_definition_is_emitted() {
        let json: &str = r#"{"type":"object","properties":{"a":{"type":"string"}},"$defs":{"Unused":{"type":"object","properties":{"z":{"type":"boolean"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Unused {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The order of `$defs` keys in the input does not change the output.
    #[test]
    fn defs_key_order_does_not_change_output() {
        let json_a: &str = r##"{"type":"object","properties":{"x":{"$ref":"#/$defs/X"},"y":{"$ref":"#/$defs/Y"}},"$defs":{"X":{"type":"object","properties":{"item":{"title":"Item","type":"object","properties":{"a":{"type":"string"}}}}},"Y":{"type":"object","properties":{"item":{"title":"Item","type":"object","properties":{"b":{"type":"string"}}}}}}}"##;
        let json_b: &str = r##"{"type":"object","properties":{"x":{"$ref":"#/$defs/X"},"y":{"$ref":"#/$defs/Y"}},"$defs":{"Y":{"type":"object","properties":{"item":{"title":"Item","type":"object","properties":{"b":{"type":"string"}}}}},"X":{"type":"object","properties":{"item":{"title":"Item","type":"object","properties":{"a":{"type":"string"}}}}}}}"##;
        let schema_a: Schema = serde_json::from_str(json_a).unwrap();
        let schema_b: Schema = serde_json::from_str(json_b).unwrap();
        let settings: CodeGenSettings = default_settings();
        let expected: super::GenerateRustOutput = generate_rust(&[schema_a], &settings).unwrap();
        let actual: super::GenerateRustOutput = generate_rust(&[schema_b], &settings).unwrap();
        assert_eq!(expected, actual);
    }

    /// A schema's output does not depend on the other schemas in the batch when nothing is shared.
    #[test]
    fn unrelated_schema_in_batch_does_not_change_output() {
        let json: &str = r#"{"type":"object","properties":{"first":{"title":"Item","type":"object","properties":{"a":{"type":"string"}}},"second":{"title":"Item","type":"object","properties":{"b":{"type":"integer"}}}}}"#;
        let other: &str = r#"{"title":"Other","type":"object","properties":{"item":{"title":"Item","type":"object","properties":{"c":{"type":"boolean"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let other_schema: Schema = serde_json::from_str(other).unwrap();
        let settings: CodeGenSettings = default_settings();
        let alone: super::GenerateRustOutput =
            generate_rust(std::slice::from_ref(&schema), &settings).unwrap();
        let together: super::GenerateRustOutput =
            generate_rust(&[schema, other_schema], &settings).unwrap();
        let expected: &[u8] = &alone.per_schema[0];
        let actual: &[u8] = &together.per_schema[0];
        assert_eq!(expected, actual);
    }

    /// With dedup disabled, two identical inline objects with one title are two items with
    /// suffixed names.
    #[test]
    fn dedupe_disabled_identical_inline_objects_get_numeric_suffixes() {
        let json: &str = r#"{"type":"object","properties":{"a":{"title":"Point","type":"object","properties":{"x":{"type":"integer"}}},"b":{"title":"Point","type":"object","properties":{"x":{"type":"integer"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Point")]
pub struct Point0 {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Point")]
pub struct Point1 {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Point0>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Point1>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Without `additionalProperties`, unknown keys are valid data: a flattened `serde_json::Value` map
    /// captures them.
    #[test]
    fn additional_properties_absent_captures_unknown_keys_in_value_map() {
        let json: &str =
            r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// `UnknownProperties::Capture` adds the map even when the schema says `additionalProperties: false`
    /// (and drops `deny_unknown_fields`).
    #[test]
    fn unknown_properties_capture_overrides_additional_properties_false() {
        let json: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unknown_properties(UnknownProperties::Capture)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// `UnknownProperties::Drop` leaves out the map when `additionalProperties` is absent.
    #[test]
    fn unknown_properties_drop_omits_map_when_additional_properties_absent() {
        let json: &str =
            r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
}

";
        assert_eq!(expected, actual);
    }

    /// `UnknownProperties::Drop` leaves out the map even for a typed `additionalProperties` schema.
    #[test]
    fn unknown_properties_drop_omits_typed_map() {
        let json: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":{"type":"integer"}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub name: String,
}

";
        assert_eq!(expected, actual);
    }

    /// `UnknownProperties::Drop` keeps `deny_unknown_fields` for `additionalProperties: false`.
    #[test]
    fn unknown_properties_drop_keeps_deny_unknown_fields() {
        let json: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub name: String,
}

";
        assert_eq!(expected, actual);
    }

    /// A property named `additional` and the unknown-keys map collide: both get a numeric suffix.
    #[test]
    fn property_named_additional_collides_with_unknown_keys_map() {
        let json: &str = r#"{"type":"object","properties":{"additional":{"type":"string"}},"required":["additional"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "additional")]
    pub additional0: String,
    #[serde(flatten)]
    pub additional1: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Two different `pattern`s in a conjunction combine into one pattern of lookaheads.
    #[test]
    fn all_of_different_patterns_combine_into_lookaheads() {
        let json: &str = r#"{"allOf":[{"type":"object","properties":{"x":{"type":"string","pattern":"^[a-z]+$"}}},{"type":"object","properties":{"x":{"type":"string","pattern":"^[0-9]+$"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(pattern = "^(?=[\\s\\S]*?(?:^[a-z]+$))(?=[\\s\\S]*?(?:^[0-9]+$))")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Number bounds with an empty range have no valid value: an uninhabited enum.
    #[test]
    fn number_with_empty_range_emits_uninhabited_enum() {
        let json: &str = r#"{"type":"object","properties":{"n":{"type":"number","minimum":1.5,"exclusiveMaximum":1.5}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum N {
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: N,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// Equal integer bounds hold exactly one value: the narrowest type for it, not an uninhabited enum.
    #[test]
    fn integer_with_equal_bounds_emits_narrowest_type() {
        let json: &str = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":5,"maximum":5}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The value type of a property's typed map is named after the property: `<key>Value`.
    #[test]
    fn map_value_struct_is_named_after_property_key() {
        let json: &str = r#"{"type":"object","properties":{"foo":{"type":"object","additionalProperties":{"type":"object","properties":{"x":{"type":"integer"}}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooValue {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foo: Option<BTreeMap<String, FooValue>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The value type of a root map is named after the root: `RootValue`.
    #[test]
    fn map_value_struct_of_root_is_named_after_root() {
        let json: &str = r#"{"type":"object","additionalProperties":{"type":"object","properties":{"x":{"type":"integer"}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootValue {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

pub type Root = BTreeMap<String, RootValue>;

"#;
        assert_eq!(expected, actual);
    }

    /// Inline union variants of a `$defs` entry are named after the definition key.
    #[test]
    fn union_variant_structs_of_definition_are_named_after_definition_key() {
        let json: &str = r##"{"type":"object","properties":{"s":{"$ref":"#/$defs/Shape"}},"$defs":{"Shape":{"anyOf":[{"type":"object","properties":{"r":{"type":"number"}}},{"type":"object","properties":{"w":{"type":"number"}}}]}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Shape {
    Variant0(ShapeVariant0),
    Variant1(ShapeVariant1),
}

json_schema_rs::deserialize_untagged!(Shape { Variant0(ShapeVariant0), Variant1(ShapeVariant1) });

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<Shape>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Inline union variants of a titled root are named after the title.
    #[test]
    fn union_variant_structs_of_titled_root_are_named_after_title() {
        let json: &str = r#"{"title":"Shape","oneOf":[{"type":"object","properties":{"r":{"type":"number"}}},{"type":"object","properties":{"w":{"type":"number"}}}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum ShapeOneOf {
    Variant0(ShapeVariant0),
    Variant1(ShapeVariant1),
}

json_schema_rs::deserialize_untagged!(ShapeOneOf { Variant0(ShapeVariant0), Variant1(ShapeVariant1) });

"#;
        assert_eq!(expected, actual);
    }

    /// Two by-value back-edges to the same struct are both boxed.
    #[test]
    fn cycle_with_two_back_edges_boxes_both() {
        let schema: Schema = Schema::try_from(r##"{"title":"Tree","type":"object","properties":{"left":{"$ref":"#"},"right":{"$ref":"#"}}}"##).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Tree")]
pub struct Tree {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<Tree>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<Tree>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A cycle through the root and a definition boxes each back-edge.
    #[test]
    fn cycle_through_root_and_definition_boxes_every_back_edge() {
        let schema: Schema = Schema::try_from(r##"{"type":"object","properties":{"a":{"$ref":"#"},"b":{"$ref":"#/$defs/B"}},"$defs":{"B":{"type":"object","properties":{"r":{"$ref":"#"}}}}}"##).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct B {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<Box<Root>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Box<Root>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<B>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A union of itself and null carries no type information: any value.
    #[test]
    fn nullable_self_reference_is_value() {
        let schema: Schema =
            Schema::try_from(r##"{"anyOf":[{"$ref":"#"},{"type":"null"}]}"##).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = serde_json::Value;

";
        assert_eq!(expected, actual);
    }

    /// Two definitions that are each other or null: any value.
    #[test]
    fn nullable_mutual_reference_is_value() {
        let schema: Schema = Schema::try_from(r##"{"$defs":{"A":{"anyOf":[{"$ref":"#/$defs/B"},{"type":"null"}]},"B":{"anyOf":[{"$ref":"#/$defs/A"},{"type":"null"}]}},"properties":{"a":{"$ref":"#/$defs/A"}}}"##).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub a: Option<serde_json::Value>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Defaults that fit the field type become functions; a default that does not fit (300 for u8, a UUID string) is left out.
    #[test]
    fn defaults_that_do_not_fit_the_type_get_no_accessor() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"x": {"type": "number", "default": 5}, "y": {"type": "number", "default": 2.5}, "u": {"type": "string", "format": "uuid", "default": "00000000-0000-0000-0000-000000000000"}, "b": {"type": "integer", "maximum": 10, "minimum": 0, "default": 300}, "c": {"type": "integer", "maximum": 10, "minimum": 0, "default": 7}, "s": {"type": "string", "default": "a\r\"b"}}, "required": ["x"]}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 300)]
    pub b: Option<u8>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 7)]
    pub c: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "a\r\"b")]
    pub s: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "00000000-0000-0000-0000-000000000000")]
    pub u: Option<Uuid>,
    #[json_schema(default = 5)]
    pub x: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 2.5)]
    pub y: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn c_or_default(&self) -> u8 {
        self.c.unwrap_or(7)
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn s_or_default(&self) -> &str {
        self.s.as_deref().unwrap_or("a\r\"b")
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn y_or_default(&self) -> f64 {
        self.y.unwrap_or(2.5)
    }
}

"#;
        assert_eq!(expected, actual);
    }

    /// Default functions are associated functions, so `FooBar.baz` and `Foo.bar_baz` cannot clash.
    #[test]
    fn required_field_defaults_are_annotations_only() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"a": {"type": "string", "default": "x"}, "b": {"type": ["integer", "null"], "default": 3}}, "required": ["a", "b"]}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(default = "x")]
    pub a: String,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    #[json_schema(required, default = 3)]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn optional_nullable_field_default_accessor_keeps_null() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"m": {"type": ["integer", "null"], "default": -3}, "n": {"type": ["string", "null"], "default": "z"}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    #[json_schema(default = -3)]
    pub m: json_schema_rs::Nullable<i64>,
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    #[json_schema(default = "z")]
    pub n: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn m_or_default(&self) -> Option<i64> {
        self.m.or_absent(Some(-3))
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn n_or_default(&self) -> Option<&str> {
        self.n.as_deref().or_absent(Some("z"))
    }
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_scalar_and_null_defaults_have_no_attribute_or_accessor() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"a": {"type": "array", "items": {"type": "string"}, "default": ["x"]}, "o": {"type": "object", "properties": {"k": {"type": "string"}}, "default": {"k": "v"}}, "z": {"type": ["string", "null"], "default": null}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct O {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o: Option<O>,
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub z: json_schema_rs::Nullable<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn float_defaults_get_accessors() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"f": {"type": "number", "minimum": 0, "maximum": 1, "default": 1e300}, "g": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 1e+300)]
    pub f: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 0.5)]
    pub g: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn f_or_default(&self) -> f64 {
        self.f.unwrap_or(1e300)
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn g_or_default(&self) -> f64 {
        self.g.unwrap_or(0.5)
    }
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_accessors_of_deprecated_items_expect_the_deprecation() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "$defs": {
                    "Level": {"title": "Level", "type": "integer", "minimum": 1, "deprecated": true},
                    "Old": {
                        "title": "Old", "type": "object", "deprecated": true,
                        "properties": {"n": {"type": "integer", "default": 3}}
                    },
                    "Plain": {
                        "title": "Plain", "type": "object",
                        "properties": {"level": {"$ref": "#/$defs/Level", "default": 2}}
                    }
                }
            })],
            true,
        )
        .1
        .remove(0);
        let expected: &str = r###"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[deprecated]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(title = "Level", minimum = 1)]
pub struct Level(i64);

json_schema_rs::constrained_newtype!(integer, Level, i64, TYPE_VALIDATORS, 0);

#[deprecated]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Old")]
pub struct Old {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 3)]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[expect(deprecated)]
impl Old {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn n_or_default(&self) -> i64 {
        self.n.unwrap_or(3)
    }
}

json_schema_rs::self_validating!(Old, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Plain")]
pub struct Plain {
    #[expect(deprecated)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 2)]
    pub level: Option<Level>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl ::core::cmp::Eq for Plain {}

impl Plain {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    #[expect(deprecated)]
    pub fn level_or_default(&self) -> i64 {
        self.level.map_or(2, Level::into_inner)
    }
}

json_schema_rs::self_validating!(Plain, TYPE_VALIDATORS, 2);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"$defs":{"Level":{"minimum":1,"type":"integer"},"Old":{"properties":{"n":{"type":"integer"}},"type":"object"},"Plain":{"properties":{"level":{"$ref":"#/$defs/Level"}},"type":"object"}}}"##,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/$defs/Level"],
        &["#/$defs/Old"],
        &["#/$defs/Plain"],
    ],
);

pub type Root = serde_json::Value;

"###;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_accessors_are_methods_of_their_own_struct() {
        let schema: Schema = Schema::try_from(r#"{"$defs":{"FooBar":{"type":"object","properties":{"baz":{"type":"integer","default":1}}},"Foo":{"type":"object","properties":{"bar_baz":{"type":"integer","default":2}}}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Foo {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 2)]
    pub bar_baz: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Foo {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn bar_baz_or_default(&self) -> i64 {
        self.bar_baz.unwrap_or(2)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooBar {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 1)]
    pub baz: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl FooBar {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn baz_or_default(&self) -> i64 {
        self.baz.unwrap_or(1)
    }
}

pub type Root = serde_json::Value;

"#;
        assert_eq!(expected, actual);
    }

    /// Control characters are escaped as Rust literals in renames and enum values.
    #[test]
    fn control_characters_in_keys_titles_and_enum_values_are_escaped() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "title": "T\u0001", "properties": {"k\rx": {"type": "string"}, "e": {"enum": [1, "a\u0001b", {"k\"": [null, true]}]}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum E {
    #[json_schema(value = "a\u{1}b")]
    AB,
    #[json_schema(value = 1)]
    N1,
    #[json_schema(value = {"k\"": [null, true]})]
    Object,
}

impl Serialize for E {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::AB => serde_json::json!("a\u{1}b"),
            Self::N1 => serde_json::json!(1),
            Self::Object => serde_json::json!({"k\"": [null, true]}),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for E {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("a\u{1}b")) {
            return Ok(Self::AB);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!({"k\"": [null, true]})) {
            return Ok(Self::Object);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "\"a\\u0001b\", 1, {\"k\\\"\":[null,true]}")))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "T\u{1}")]
pub struct T {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e: Option<E>,
    #[serde(rename = "k\rx", skip_serializing_if = "Option::is_none")]
    pub k_x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The root keeps `Root` although a property refers back to it; a title with no letters falls
    /// back to the key.
    #[test]
    fn root_keeps_root_name_when_a_property_refers_back_to_it() {
        let schema: Schema = Schema::try_from(r##"{"type":"object","properties":{"children":{"type":"array","items":{"$ref":"#"}},"z":{"title":"!!","type":"object","properties":{"q":{"type":"string"}}}}}"##).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "!!")]
pub struct Z {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Root>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<Z>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    // --- uniqueItems sets of generated item types ---

    /// The per-schema buffer for one schema generated with `setting`.
    fn generate_with_unique_items(schema_json: &str, setting: UniqueItemsType) -> String {
        let schema: Schema = Schema::try_from(schema_json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(setting)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        String::from_utf8(output.per_schema[0].clone()).unwrap()
    }

    /// A closed `Point` struct (two integers and an optional `Tag` whose unknown keys are
    /// typed booleans) as the item of a required `uniqueItems` array.
    const POINTS_SCHEMA: &str = r##"{"type":"object","properties":{"points":{"type":"array","items":{"$ref":"#/$defs/Point"},"uniqueItems":true}},"required":["points"],"$defs":{"Point":{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"tag":{"$ref":"#/$defs/Tag"}},"required":["x","y"],"additionalProperties":false},"Tag":{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":{"type":"boolean"}}}}"##;

    #[test]
    fn unique_items_hash_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: HashSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_btree_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::BTreeSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: BTreeSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "indexmap")]
    #[test]
    fn unique_items_index_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::IndexSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use indexmap::IndexSet;
use std::collections::BTreeMap;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for indexmap::IndexSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: IndexSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_vec_closed_struct_items_get_no_extra_derives() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::Vec);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub points: Vec<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_hash_set_string_enum_items_emit_set() {
        let schema_json: &str = r#"{"type":"object","properties":{"colors":{"type":"array","items":{"enum":["red","green"]},"uniqueItems":true}},"required":["colors"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Green,
    Red,
}

json_schema_rs::string_enum!(Color);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub colors: HashSet<Color>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_hash_set_union_items_derive_hash_eq_ord() {
        let schema_json: &str = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"oneOf":[{"type":"string"},{"type":"integer"}]},"uniqueItems":true}},"required":["ids"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum IdOneOf {
    String(String),
    Integer(i64),
}

json_schema_rs::deserialize_untagged!(IdOneOf { String(String), Integer(i64 = with(json_schema_rs::integer::deserialize)) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub ids: HashSet<IdOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `1` and `1.0` are equal JSON, but `i64` rejects `1.0` while the enum accepts both, so
    /// they would deserialize to different variants: the set would keep a duplicate.
    #[test]
    fn unique_items_set_union_of_integer_and_whole_number_enum_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"codes":{"type":"array","items":{"anyOf":[{"type":"integer"},{"enum":[1,"a"]}]},"uniqueItems":true}},"required":["codes"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum CodeVariant1 {
    #[json_schema(value = "a")]
    A,
    #[json_schema(value = 1)]
    N1,
}

impl Serialize for CodeVariant1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::A => serde_json::json!("a"),
            Self::N1 => serde_json::json!(1),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CodeVariant1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("a")) {
            return Ok(Self::A);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "\"a\", 1")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum CodeAnyOf {
    Integer(i64),
    Variant1(CodeVariant1),
}

json_schema_rs::deserialize_untagged!(CodeAnyOf { Integer(i64 = with(json_schema_rs::integer::deserialize)), Variant1(CodeVariant1) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub codes: Vec<CodeAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// An enum of fractional numbers never matches a whole number, so `1` vs `1.0` cannot split
    /// across the variants.
    #[test]
    #[expect(clippy::too_many_lines)]
    fn unique_items_hash_set_union_of_integer_and_fractional_enum_emits_set() {
        let schema_json: &str = r#"{"type":"object","properties":{"codes":{"type":"array","items":{"anyOf":[{"type":"integer"},{"enum":[1.5,"a"]}]},"uniqueItems":true}},"required":["codes"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum CodeVariant1 {
    #[json_schema(value = "a")]
    A,
    #[json_schema(value = 1.5)]
    N1Point5,
}

impl Serialize for CodeVariant1 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::A => serde_json::json!("a"),
            Self::N1Point5 => serde_json::json!(1.5),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CodeVariant1 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("a")) {
            return Ok(Self::A);
        }
        if json_schema_rs::json_equal(&value, &serde_json::json!(1.5)) {
            return Ok(Self::N1Point5);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "\"a\", 1.5")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum CodeAnyOf {
    Integer(i64),
    Variant1(CodeVariant1),
}

json_schema_rs::deserialize_untagged!(CodeAnyOf { Integer(i64 = with(json_schema_rs::integer::deserialize)), Variant1(CodeVariant1) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub codes: HashSet<CodeAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// One branch holding both an integer and a whole-number enum is fine: each position has
    /// one type, so `1` and `1.0` take the same branch (or `1.0` fails).
    #[test]
    #[expect(clippy::too_many_lines)]
    fn unique_items_hash_set_union_with_integer_and_whole_number_enum_in_one_branch_emits_set() {
        let schema_json: &str = r##"{"type":"object","properties":{"pairs":{"type":"array","items":{"anyOf":[{"$ref":"#/$defs/Pair"},{"type":"string"}]},"uniqueItems":true}},"required":["pairs"],"$defs":{"Pair":{"type":"object","properties":{"n":{"type":"integer"},"k":{"const":1}},"required":["n","k"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum K {
    #[json_schema(value = 1)]
    N1,
}

impl Serialize for K {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::N1 => serde_json::json!(1),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for K {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "1")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub k: K,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub n: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PairAnyOf {
    Pair(Pair),
    String(String),
}

json_schema_rs::deserialize_untagged!(PairAnyOf { Pair(Pair), String(String) });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub pairs: HashSet<PairAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_set_struct_item_with_float_field_falls_back_to_vec() {
        let schema_json: &str = r##"{"type":"object","properties":{"readings":{"type":"array","items":{"$ref":"#/$defs/Reading"},"uniqueItems":true}},"required":["readings"],"$defs":{"Reading":{"type":"object","properties":{"value":{"type":"number"}},"required":["value"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reading {
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub readings: Vec<Reading>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    /// Unknown keys kept as `serde_json::Value` (no `Hash`): the item stays in a `Vec`.
    #[test]
    fn unique_items_set_struct_item_capturing_untyped_unknown_keys_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"loose":{"type":"array","items":{"type":"object","properties":{"a":{"type":"string"}}},"uniqueItems":true}},"required":["loose"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Loose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub loose: Vec<Loose>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// Dropped unknown keys would make `{"a": "x", "b": 1}` and `{"a": "x"}` equal items.
    #[test]
    fn unique_items_set_struct_item_dropping_unknown_keys_falls_back_to_vec() {
        let schema: Schema = Schema::try_from(r#"{"type":"object","properties":{"loose":{"type":"array","items":{"type":"object","properties":{"a":{"type":"string"}}},"uniqueItems":true}},"required":["loose"]}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .unique_items(UniqueItemsType::HashSet)
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Loose {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub loose: Vec<Loose>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A set field ignores order, a JSON array does not: the outer item stays in a `Vec`, the
    /// inner field is still a set.
    #[test]
    fn unique_items_set_struct_item_with_set_field_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"groups":{"type":"array","items":{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"],"additionalProperties":false},"uniqueItems":true}},"required":["groups"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub tags: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub groups: Vec<Group>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A recursive item type is a set item when every type it reaches is.
    #[test]
    fn unique_items_hash_set_recursive_struct_items_emit_set() {
        let schema_json: &str = r##"{"type":"object","properties":{"nodes":{"type":"array","items":{"$ref":"#/$defs/Node"},"uniqueItems":true}},"required":["nodes"],"$defs":{"Node":{"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#/$defs/Node"}}},"required":["name","children"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: std::hash::Hash + Eq> UniqueSet for std::collections::HashSet<T> {
        type Item = T;
        fn with_capacity(capacity: usize) -> Self {
            Self::with_capacity(capacity)
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub children: Vec<Node>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub nodes: HashSet<Node>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `Uuid` parsing folds case and accepts several forms, so two different JSON strings can be
    /// one `Uuid`: the item stays in a `Vec`.
    #[cfg(feature = "uuid")]
    #[test]
    fn unique_items_set_uuid_items_fall_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"type":"string","format":"uuid"},"uniqueItems":true}},"required":["ids"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub ids: Vec<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    // --- Self-validating types ---

    /// The buffers of `schemas` generated with `self_validating` on.
    fn self_validating_buffers(schemas: &[serde_json::Value]) -> (Option<String>, Vec<String>) {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(true)
            .constrained_newtypes(false)
            .build();
        buffers(generate_rust(&schemas_of(schemas), &settings).expect("generate"))
    }

    #[test]
    fn default_settings_generate_self_validating_types() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected: bool = true;
        let actual: bool = settings.self_validating;
        assert_eq!(expected, actual);
    }

    /// Self-validating: a map of unnamed keys typed by a pattern keeps the root's validator, which
    /// checks the key names.
    #[test]
    fn self_validating_pattern_property_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string"}},
            "patternProperties": {"^x-": {"type": "integer", "minimum": 0}},
            "additionalProperties": false
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten, deserialize_with = "json_schema_rs::integer::deserialize")]
    pub additional: BTreeMap<String, i64>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"additionalProperties":false,"patternProperties":{"^x-":{"minimum":0,"type":"integer"}},"properties":{"name":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// A root array is a newtype that validates itself: no parent checks its `maxItems`.
    #[test]
    fn self_validating_root_array_is_newtype_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "array",
            "maxItems": 2,
            "items": {"type": "string"}
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"items":{"type":"string"},"maxItems":2,"type":"array"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Root(pub Vec<String>);

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

"##;
        assert_eq!(expected, actual);
    }

    /// A root map of integers is a newtype that reads its values by the integer rule.
    #[test]
    fn self_validating_root_map_of_integers_is_newtype_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "maxProperties": 2,
            "additionalProperties": {"type": "integer"}
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"additionalProperties":{"type":"integer"},"maxProperties":2,"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Root(#[serde(deserialize_with = "json_schema_rs::integer::deserialize")] pub BTreeMap<String, i64>);

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

"##;
        assert_eq!(expected, actual);
    }

    /// A root scalar its Rust type holds exactly stays a type alias.
    #[test]
    fn self_validating_root_plain_string_stays_alias_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({"type": "string"})])
            .1
            .remove(0);
        let expected: &str =
            "//! Generated by json-schema-rs. Do not edit manually.\n\npub type Root = String;\n\n";
        assert_eq!(expected, actual);
    }

    /// A key both `then` and `else` define is typed by a schema codegen builds, which sits at no
    /// location: the union has no validation target, and the parent's validation checks it.
    #[test]
    fn self_validating_then_else_shared_property_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"kind": {"type": "string"}},
            "if": {"properties": {"kind": {"const": "a"}}},
            "then": {"properties": {"v": {"type": "integer", "minimum": 1}}},
            "else": {"properties": {"v": {"type": "integer", "maximum": 0}}}
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum VAnyOf {
    Integer0(i64),
    Integer1(i64),
}

json_schema_rs::deserialize_untagged!(VAnyOf { Integer0(i64 = with(json_schema_rs::integer::deserialize)), Integer1(i64 = with(json_schema_rs::integer::deserialize)) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<VAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"else":{"properties":{"v":{"maximum":0,"type":"integer"}}},"if":{"properties":{"kind":{"const":"a"}}},"properties":{"kind":{"type":"string"}},"then":{"properties":{"v":{"minimum":1,"type":"integer"}}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// A variant named `Error` would make `Self::Error` ambiguous (the variant or the associated
    /// type), so `try_from` names the error type itself.
    #[test]
    fn self_validating_enum_with_error_variant_names_the_error_type_golden() {
        let actual: String = self_validating_buffers(&[
            serde_json::json!({"title": "Level", "enum": ["debug", "error"]}),
        ])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Error,
}

json_schema_rs::string_enum!(Level);

json_schema_rs::self_validating!(Level, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"enum":["debug","error"]}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_struct_gets_validate_and_try_from_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string", "minLength": 2}},
            "required": ["name"],
            "additionalProperties": false
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[json_schema(min_length = 2)]
    pub name: String,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"additionalProperties":false,"properties":{"name":{"minLength":2,"type":"string"}},"required":["name"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_any_of_union_deserializes_by_validation_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "anyOf": [
                {"type": "integer", "maximum": 10, "title": "Small"},
                {"type": "integer", "title": "Large"}
            ]
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Small(i64),
    Large(i64),
}

json_schema_rs::deserialize_union!(RootAnyOf, TYPE_VALIDATORS, AnyOf, Some(0), [1, 2], |value| {
    0 => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Small),
    _ => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Large),
});

json_schema_rs::self_validating!(RootAnyOf, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"anyOf":[{"maximum":10,"type":"integer"},{"type":"integer"}]}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
        &["#/anyOf/0"],
        &["#/anyOf/1"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// A `null` branch next to several others makes the union of the others optional.
    #[test]
    fn one_of_null_and_several_branches_is_optional_union_golden() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .build();
        let actual: String = buffers(
            generate_rust(
                &schemas_of(&[serde_json::json!({
                    "type": "object",
                    "properties": {"x": {"oneOf": [{"type": "null"}, {"type": "string"}, {"type": "integer"}]}}
                })]),
                &settings,
            )
            .expect("generate"),
        )
        .1
        .remove(0);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum XOneOf {
    String(String),
    Integer(i64),
}

json_schema_rs::deserialize_untagged!(XOneOf { String(String), Integer(i64 = with(json_schema_rs::integer::deserialize)) });

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, skip_serializing_if = "json_schema_rs::Nullable::is_absent")]
    pub x: json_schema_rs::Nullable<XOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The self-validating union of the non-`null` branches validates at the whole `oneOf`, so
    /// a `null` branch still counts toward `oneOf`'s one match.
    #[test]
    fn self_validating_root_one_of_null_and_several_branches_is_optional_union_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "oneOf": [{"type": "string"}, {"type": "null"}, {"type": "integer"}]
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
    String(String),
    Integer(i64),
}

json_schema_rs::deserialize_union!(RootOneOf, TYPE_VALIDATORS, OneOf, Some(0), [1, 2], |value| {
    0 => <String as Deserialize>::deserialize(value).map(Self::String),
    _ => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Integer),
});

json_schema_rs::self_validating!(RootOneOf, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"oneOf":[{"type":"string"},{"type":"null"},{"type":"integer"}]}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
        &["#/oneOf/0"],
        &["#/oneOf/2"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// `const` branches next to a free string merge into one enum variant; each still
    /// validates at its own branch.
    #[test]
    fn self_validating_any_of_const_branches_and_string_share_one_variant_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"parser": {"anyOf": [{"const": "flow"}, {"const": "babel"}, {"type": "string", "description": "Custom"}]}}
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ParserVariant0 {
    Babel,
    Flow,
}

json_schema_rs::string_enum!(ParserVariant0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum ParserAnyOf {
    Variant0(ParserVariant0),
    String(String),
}

json_schema_rs::deserialize_union!(ParserAnyOf, TYPE_VALIDATORS, AnyOf, Some(0), [1, 2, 3], |value| {
    0 | 1 => <ParserVariant0 as Deserialize>::deserialize(value).map(Self::Variant0),
    _ => <String as Deserialize>::deserialize(value).map(Self::String),
});

json_schema_rs::self_validating!(ParserAnyOf, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser: Option<ParserAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 4);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"parser":{"anyOf":[{"const":"flow"},{"const":"babel"},{"type":"string"}]}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/parser"],
        &["#/properties/parser/anyOf/0"],
        &["#/properties/parser/anyOf/1"],
        &["#/properties/parser/anyOf/2"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_one_of_string_branches_share_one_variant_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"x": {"oneOf": [{"enum": ["a", "b"]}, {"const": "b"}, {"type": "integer"}]}}
        })])
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum XVariant0 {
    A,
    B,
}

json_schema_rs::string_enum!(XVariant0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum XOneOf {
    Variant0(XVariant0),
    Integer(i64),
}

json_schema_rs::deserialize_union!(XOneOf, TYPE_VALIDATORS, OneOf, Some(0), [1, 2, 3], |value| {
    0 | 1 => <XVariant0 as Deserialize>::deserialize(value).map(Self::Variant0),
    _ => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Integer),
});

json_schema_rs::self_validating!(XOneOf, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<XOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 4);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"x":{"oneOf":[{"enum":["a","b"]},{"const":"b"},{"type":"integer"}]}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/x"],
        &["#/properties/x/oneOf/0"],
        &["#/properties/x/oneOf/1"],
        &["#/properties/x/oneOf/2"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_ref_with_not_is_its_own_type_golden() {
        let actual: String = self_validating_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {
                "a": {"$ref": "#/$defs/Item"},
                "b": {"$ref": "#/$defs/Item", "not": {"required": ["z"]}}
            },
            "$defs": {"Item": {"type": "object", "properties": {"z": {"type": "string"}}, "additionalProperties": false}}
        })])
        .1
        .remove(0);
        let expected: &str = r###"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<String>,
}

json_schema_rs::self_validating!(Item, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct B {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<String>,
}

json_schema_rs::self_validating!(B, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Item>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<B>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 2);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"$defs":{"Item":{"additionalProperties":false,"properties":{"z":{"type":"string"}},"type":"object"}},"properties":{"a":{"$ref":"#/$defs/Item"},"b":{"$ref":"#/$defs/Item","not":{"required":["z"]}}},"type":"object"}"##,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/$defs/Item"],
        &["#/properties/b"],
        &["#"],
    ],
);

"###;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_identical_inputs_without_uri_share_their_type() {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "additionalProperties": false
        });
        let actual: bool = self_validating_buffers(&[schema.clone(), schema])
            .0
            .is_some();
        let expected: bool = true;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_inputs_without_uri_differing_beyond_shape_are_not_shared() {
        let schema = |excluded: &str| -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {"a": {"type": "string"}},
                "not": {"required": [excluded]}
            })
        };
        let actual: Option<String> = self_validating_buffers(&[schema("b"), schema("c")]).0;
        let expected: Option<String> = None;
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_type_shared_by_two_inputs_golden() {
        let a: Schema = Schema::try_from(&serde_json::json!({
            "type": "object",
            "properties": {"thing": {"$ref": "b.json#/$defs/Thing"}}
        }))
        .expect("parse a");
        let b: Schema = Schema::try_from(&serde_json::json!({
            "type": "object",
            "properties": {"thing": {"$ref": "#/$defs/Thing"}},
            "$defs": {"Thing": {"type": "object", "properties": {"n": {"type": "integer", "minimum": 1}}, "additionalProperties": false}}
        }))
        .expect("parse b");
        let input: CodeGenInput<'_> = CodeGenInput::new()
            .schema_at("https://example.com/a.json", &a)
            .schema_at("https://example.com/b.json", &b);
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(true)
            .constrained_newtypes(false)
            .build();
        let (shared, _) = buffers(generate_rust(input, &settings).expect("generate"));
        let actual: Option<String> = shared;
        let expected: Option<String> = Some(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Thing {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
}

json_schema_rs::self_validating!(Thing, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "https://example.com/a.json",
            json: r"{}",
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
        json_schema_rs::EmbeddedDocument {
            uri: "https://example.com/b.json",
            json: r#"{"$defs":{"Thing":{"additionalProperties":false,"properties":{"n":{"minimum":1,"type":"integer"}},"type":"object"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["https://example.com/b.json#/$defs/Thing"],
    ],
);

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_schema_the_validator_cannot_compile_is_an_error() {
        let schema: Vec<Schema> = schemas_of(&[serde_json::json!({
            "type": "object",
            "properties": {"a": {"type": "string", "pattern": "("}}
        })]);
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(true)
            .constrained_newtypes(false)
            .build();
        let actual: String = generate_rust(&schema, &settings)
            .expect_err("invalid pattern")
            .to_string();
        let expected: &str = r#"schema at index 0: the schema cannot be embedded for self-validating types: /properties/a/pattern: invalid regular expression "(": Unbalanced parenthesis"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_the_validator_cannot_compile_generates_without_self_validating() {
        let schema: Vec<Schema> = schemas_of(&[serde_json::json!({
            "type": "object",
            "properties": {"a": {"type": "string", "pattern": "("}}
        })]);
        let actual: bool = generate_rust(&schema, &default_settings()).is_ok();
        let expected: bool = true;
        assert_eq!(expected, actual);
    }

    /// The buffers of `schemas` generated with constrained newtypes, and `self_validating`.
    fn newtype_buffers(
        schemas: &[serde_json::Value],
        self_validating: bool,
    ) -> (Option<String>, Vec<String>) {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(self_validating)
            .constrained_newtypes(true)
            .build();
        buffers(generate_rust(&schemas_of(schemas), &settings).expect("generate"))
    }

    #[test]
    fn default_settings_generate_constrained_newtypes() {
        let settings: CodeGenSettings = CodeGenSettings::builder().build();
        let expected: bool = true;
        let actual: bool = settings.constrained_newtypes;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtypes_off_keeps_plain_scalars_golden() {
        let schema: Vec<Schema> = schemas_of(&[serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "minLength": 1, "pattern": "^[A-Z]"},
                "age": {"type": "integer", "minimum": 1, "maximum": 150},
                "score": {"type": "number", "multipleOf": 0.5}
            }
        })]);
        let off: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .constrained_newtypes(false)
            .build();
        let actual: String = buffers(generate_rust(&schema, &off).expect("generate"))
            .1
            .remove(0);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub age: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(min_length = 1, pattern = "^[A-Z]")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_string_property_golden() {
        let actual: String = newtype_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"name": {"type": "string", "minLength": 1, "maxLength": 10, "pattern": "^[A-Z]"}},
            "required": ["name"],
            "additionalProperties": false
        })], false)
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 1, max_length = 10, pattern = "^[A-Z]")]
pub struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub name: Name,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"name":{"maxLength":10,"minLength":1,"pattern":"^[A-Z]","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/name"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// Both newtypes (two types: only one has `maxLength`) check the one pattern through one
    /// `TYPE_VALIDATORS` static, whose compiled schema holds the regex once (see the validator's
    /// `newtypes_with_equal_patterns_share_one_compiled_regex`).
    #[test]
    fn constrained_newtypes_with_equal_patterns_share_one_static_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {
                    "billing": {"type": "string", "pattern": "^[A-Z]{2}$"},
                    "shipping": {"type": "string", "pattern": "^[A-Z]{2}$", "maxLength": 2}
                },
                "required": ["billing", "shipping"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(pattern = "^[A-Z]{2}$")]
pub struct Billing(String);

json_schema_rs::constrained_newtype!(string, Billing, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(max_length = 2, pattern = "^[A-Z]{2}$")]
pub struct Shipping(String);

json_schema_rs::constrained_newtype!(string, Shipping, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub billing: Billing,
    pub shipping: Shipping,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"billing":{"pattern":"^[A-Z]{2}$","type":"string"},"shipping":{"maxLength":2,"pattern":"^[A-Z]{2}$","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/billing"],
        &["#/properties/shipping"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_integer_with_narrower_bounds_and_multiple_of_golden() {
        let actual: String = newtype_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"percent": {"type": "integer", "minimum": 0, "maximum": 100, "multipleOf": 5}},
            "required": ["percent"],
            "additionalProperties": false
        })], false)
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(minimum = 0, maximum = 100, multiple_of = 5)]
pub struct Percent(u8);

json_schema_rs::constrained_newtype!(integer, Percent, u8, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub percent: Percent,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"percent":{"maximum":100,"minimum":0,"multipleOf":5,"type":"integer"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/percent"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_number_with_bounds_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {"ratio": {"type": "number", "exclusiveMinimum": 0, "maximum": 1}},
                "required": ["ratio"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(maximum = 1, exclusive_minimum = 0)]
pub struct Ratio(f64);

json_schema_rs::constrained_newtype!(scalar, Ratio, f64, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub ratio: Ratio,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"ratio":{"exclusiveMinimum":0,"maximum":1,"type":"number"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/ratio"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_integer_bounds_its_type_holds_stay_plain_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {
                    "byte": {"type": "integer", "minimum": 0, "maximum": 255},
                    "length": {"type": "string", "minLength": 0}
                },
                "required": ["byte", "length"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = "//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(deserialize_with = \"json_schema_rs::integer::deserialize\")]
    pub byte: u8,
    #[json_schema(min_length = 0)]
    pub length: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_definition_is_named_after_its_key_golden() {
        let actual: String = newtype_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {
                "home": {"$ref": "#/$defs/PostalCode"},
                "work": {"$ref": "#/$defs/PostalCode"}
            },
            "additionalProperties": false,
            "$defs": {"PostalCode": {"type": "string", "description": "A five-digit code.", "pattern": "^[0-9]{5}$"}}
        })], false)
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

/// A five-digit code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(pattern = "^[0-9]{5}$")]
pub struct PostalCode(String);

json_schema_rs::constrained_newtype!(string, PostalCode, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub home: Option<PostalCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<PostalCode>,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$defs":{"PostalCode":{"pattern":"^[0-9]{5}$","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/$defs/PostalCode"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_root_scalar_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "title": "Port",
                "type": "integer",
                "minimum": 1,
                "maximum": 65535
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(title = "Port", minimum = 1, maximum = 65535)]
pub struct Port(u16);

json_schema_rs::constrained_newtype!(integer, Port, u16, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"maximum":65535,"minimum":1,"type":"integer"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_optional_fields_default_accessors_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {
                    "label": {"type": "string", "minLength": 1, "default": "none"},
                    "level": {"type": "integer", "minimum": 1, "maximum": 5, "default": 3}
                },
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 1)]
pub struct Label(String);

json_schema_rs::constrained_newtype!(string, Label, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(minimum = 1, maximum = 5)]
pub struct Level(u8);

json_schema_rs::constrained_newtype!(integer, Level, u8, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "none")]
    pub label: Option<Label>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 3)]
    pub level: Option<Level>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn label_or_default(&self) -> &str {
        self.label.as_deref().map_or("none", String::as_str)
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn level_or_default(&self) -> u8 {
        self.level.map_or(3, Level::into_inner)
    }
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"label":{"minLength":1,"type":"string"},"level":{"maximum":5,"minimum":1,"type":"integer"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/label"],
        &["#/properties/level"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_string_with_not_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {"word": {"type": "string", "not": {"const": "forbidden"}}},
                "required": ["word"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Word(String);

json_schema_rs::constrained_newtype!(string, Word, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub word: Word,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"word":{"not":{"const":"forbidden"},"type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/word"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_checked_format_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "email": {"type": "string", "format": "email"},
                    "note": {"type": "string", "format": "x-unknown"}
                },
                "required": ["email", "note"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Email(String);

json_schema_rs::constrained_newtype!(string, Email, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub email: Email,
    pub note: String,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$schema":"http://json-schema.org/draft-07/schema#","properties":{"email":{"format":"email","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/email"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_checked_content_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "properties": {
                    "doc": {"type": "string", "contentMediaType": "application/json"},
                    "blob": {"type": "string", "contentEncoding": "base32"}
                },
                "required": ["doc", "blob"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub struct Doc(String);

json_schema_rs::constrained_newtype!(string, Doc, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub blob: String,
    pub doc: Doc,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"$schema":"http://json-schema.org/draft-07/schema#","properties":{"doc":{"contentMediaType":"application/json","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/doc"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn content_is_not_a_constraint_in_2020_12_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {
                    "doc": {"type": "string", "contentMediaType": "application/json"}
                },
                "required": ["doc"],
                "additionalProperties": false
            })],
            false,
        )
        .1
        .remove(0);
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub doc: String,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn constrained_newtype_with_self_validating_types_share_one_static_golden() {
        let actual: String = newtype_buffers(
            &[serde_json::json!({
                "type": "object",
                "properties": {"name": {"type": "string", "minLength": 2}},
                "required": ["name"],
                "additionalProperties": false
            })],
            true,
        )
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 2)]
pub struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub name: Name,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 1);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"additionalProperties":false,"properties":{"name":{"minLength":2,"type":"string"}},"required":["name"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/name"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn constrained_newtype_uuid_with_pattern_golden() {
        let actual: String = newtype_buffers(&[serde_json::json!({
            "type": "object",
            "properties": {"id": {"type": "string", "format": "uuid", "pattern": "^[0-9a-f-]+$"}},
            "required": ["id"],
            "additionalProperties": false
        })], false)
        .1
        .remove(0);
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(pattern = "^[0-9a-f-]+$")]
pub struct Id(Uuid);

json_schema_rs::constrained_newtype!(parsed, Id, Uuid, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    pub id: Id,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"id":{"format":"uuid","pattern":"^[0-9a-f-]+$","type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/id"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    // --- Root type names ---

    /// The root type names `generate_rust` reports for `values`, generated together.
    fn root_types_of(values: &[serde_json::Value]) -> Vec<String> {
        generate_rust(&schemas_of(values), &default_settings())
            .unwrap()
            .root_types
    }

    #[test]
    fn root_types_names_a_titled_object_root_by_its_title() {
        let expected: Vec<String> = vec!["Config".to_string()];
        let actual: Vec<String> = root_types_of(&[serde_json::json!({
            "title": "Config", "type": "object", "properties": {"a": {"type": "string"}}
        })]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_types_names_an_untitled_object_root_root() {
        let expected: Vec<String> = vec!["Root".to_string()];
        let actual: Vec<String> = root_types_of(&[serde_json::json!({
            "type": "object", "properties": {"a": {"type": "string"}}
        })]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_types_names_the_alias_of_a_root_that_is_not_a_named_item() {
        let expected: Vec<String> = vec!["Tags".to_string()];
        let actual: Vec<String> = root_types_of(&[serde_json::json!({
            "title": "Tags", "type": "array", "items": {"type": "string"}
        })]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_types_has_one_name_per_schema_in_input_order() {
        let expected: Vec<String> = vec!["First".to_string(), "Second".to_string()];
        let actual: Vec<String> = root_types_of(&[
            serde_json::json!({"title": "First", "type": "object", "properties": {"a": {"type": "string"}}}),
            serde_json::json!({"title": "Second", "type": "object", "properties": {"b": {"type": "integer"}}}),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_types_names_a_root_shared_with_another_schema() {
        let shared_root: serde_json::Value = serde_json::json!({
            "title": "Same", "type": "object", "properties": {"a": {"type": "string"}}
        });
        let expected: Vec<String> = vec!["Same".to_string(), "Same".to_string()];
        let actual: Vec<String> = root_types_of(&[shared_root.clone(), shared_root]);
        assert_eq!(expected, actual);
    }

    // --- extra derives, extra attributes and type patches ---

    /// A root struct with a `$defs` struct and a string enum, generated with `settings`.
    fn customized(settings: &CodeGenSettings) -> Result<String, CodeGenError> {
        let json = r##"{"type":"object","properties":{"veg":{"$ref":"#/$defs/Veg"},"kind":{"enum":["a","b"]}},"required":["veg"],"$defs":{"Veg":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}"##;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let output: super::GenerateRustOutput = generate_rust(&[schema], settings)?;
        Ok(String::from_utf8(output.per_schema[0].clone()).unwrap())
    }

    /// The default settings (self-validation off) changed by `customize`.
    fn customized_settings(
        customize: impl FnOnce(
            crate::code_gen::settings::CodeGenSettingsBuilder,
        ) -> crate::code_gen::settings::CodeGenSettingsBuilder,
    ) -> CodeGenSettings {
        customize(
            CodeGenSettings::builder()
                .self_validating(false)
                .constrained_newtypes(false),
        )
        .build()
    }

    #[test]
    fn extra_derives_follow_each_items_own_derives() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.extra_derives(["schemars::JsonSchema"]));
        let actual: String = customized(&settings).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    A,
    B,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
pub struct Veg {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub veg: Veg,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn extra_derives_an_item_already_implements_are_not_repeated() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.extra_derives([
                "std::fmt::Debug",
                "Clone",
                "serde::Serialize",
                "serde::Deserialize",
            ])
        });
        let expected: String = customized(&default_settings()).unwrap();
        let actual: String = customized(&settings).unwrap();
        assert_eq!(expected, actual);
    }

    #[test]
    fn extra_attrs_follow_each_items_derive_line() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.extra_attrs(["non_exhaustive", "doc(alias = \"x\")"]));
        let actual: String = customized(&settings).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[non_exhaustive]
#[doc(alias = "x")]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    A,
    B,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[non_exhaustive]
#[doc(alias = "x")]
pub struct Veg {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[non_exhaustive]
#[doc(alias = "x")]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub veg: Veg,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_renames_a_type_and_every_use_of_it() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.patch(
                "Veg",
                TypePatch {
                    rename: Some("Vegetable".to_string()),
                    ..TypePatch::default()
                },
            )
        });
        let actual: String = customized(&settings).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    A,
    B,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Vegetable {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub veg: Vegetable,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_derives_and_attrs_apply_to_its_type_only() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.extra_derives(["schemars::JsonSchema"]).patch(
                "Kind",
                TypePatch {
                    rename: None,
                    derives: vec!["Default".to_string(), "schemars::JsonSchema".to_string()],
                    attrs: vec!["non_exhaustive".to_string()],
                },
            )
        });
        let actual: String = customized(&settings).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema, Default)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    A,
    B,
}

json_schema_rs::string_enum!(Kind);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
pub struct Veg {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    pub veg: Veg,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn extra_derives_skip_a_plain_type_alias() {
        let schema: Schema = serde_json::from_str(r#"{"type":"string"}"#).unwrap();
        let settings: CodeGenSettings =
            customized_settings(|b| b.extra_derives(["Hash"]).extra_attrs(["non_exhaustive"]));
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn extra_derives_on_a_constrained_newtype_keep_its_own_deserialize() {
        let schema: Schema = serde_json::from_str(
            r#"{"type":"object","properties":{"code":{"type":"string","minLength":2}}}"#,
        )
        .unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .extra_derives(["serde::Deserialize", "schemars::JsonSchema"])
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 2)]
pub struct Code(String);

json_schema_rs::constrained_newtype!(string, Code, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema, schemars::JsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Code>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"code":{"minLength":2,"type":"string"}}}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/code"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_for_a_type_that_is_not_generated_is_an_error() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.patch("Fruit", TypePatch::default()));
        let expected: &str =
            r"the settings cannot be applied: patch for `Fruit`: no generated type has that name";
        let actual: String = customized(&settings).unwrap_err().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_rename_to_a_keyword_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.patch(
                "Veg",
                TypePatch {
                    rename: Some("Self".to_string()),
                    ..TypePatch::default()
                },
            )
        });
        let expected: &str =
            r"the settings cannot be applied: patch for `Veg`: `Self` is not a Rust identifier";
        let actual: String = customized(&settings).unwrap_err().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_rename_to_a_non_identifier_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.patch(
                "Veg",
                TypePatch {
                    rename: Some("Green Veg".to_string()),
                    ..TypePatch::default()
                },
            )
        });
        let expected: &str = r"the settings cannot be applied: patch for `Veg`: `Green Veg` is not a Rust identifier";
        let actual: String = customized(&settings).unwrap_err().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn patch_rename_to_another_types_name_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.patch(
                "Veg",
                TypePatch {
                    rename: Some("Kind".to_string()),
                    ..TypePatch::default()
                },
            )
        });
        let expected: &str = r"the settings cannot be applied: a patch renames a type to `Kind`, which another generated type in its module has";
        let actual: String = customized(&settings).unwrap_err().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn patches_may_swap_two_types_names() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.patch(
                "Veg",
                TypePatch {
                    rename: Some("Kind".to_string()),
                    ..TypePatch::default()
                },
            )
            .patch(
                "Kind",
                TypePatch {
                    rename: Some("Veg".to_string()),
                    ..TypePatch::default()
                },
            )
        });
        let actual: String = customized(&settings).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Veg {
    A,
    B,
}

json_schema_rs::string_enum!(Veg);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Kind {
    pub name: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Veg>,
    pub veg: Kind,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn malformed_extra_derives_and_attrs_are_errors() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.extra_derives(["schemars::", "Json Schema"])
                .extra_attrs(["", "#[non_exhaustive]"])
                .patch(
                    "Veg",
                    TypePatch {
                        rename: None,
                        derives: vec!["Hash<T>".to_string()],
                        attrs: vec!["  ".to_string()],
                    },
                )
        });
        let expected: &str = r"the settings cannot be applied: derive `schemars::` is not a Rust path; derive `Json Schema` is not a Rust path; attribute `` is not the text inside `#[...]`; attribute `#[non_exhaustive]` is not the text inside `#[...]`; derive `Hash<T>` is not a Rust path; attribute `` is not the text inside `#[...]`";
        let actual: String = customized(&settings).unwrap_err().to_string();
        assert_eq!(expected, actual);
    }

    // --- conversions and replacements ---

    /// `json` generated with `settings`, or the error's message.
    fn converted(json: &str, settings: &CodeGenSettings) -> Result<String, String> {
        let schema: Schema = serde_json::from_str(json).unwrap();
        generate_rust(&[schema], settings)
            .map(|output: super::GenerateRustOutput| {
                String::from_utf8(output.per_schema[0].clone()).unwrap()
            })
            .map_err(|error: CodeGenError| error.to_string())
    }

    fn decimal_schema() -> serde_json::Value {
        serde_json::json!({"type": "string", "format": "decimal"})
    }

    #[test]
    /// A property, a `$ref`'d definition, array items, a nullable copy and additional properties that match a conversion (ignoring annotations) become the user's type, each with its schema marker.
    fn conversions_replace_matching_properties_items_and_additional_properties() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(decimal_schema(), "rust_decimal::Decimal"));
        let actual: Result<String, String> = converted(
            r##"{"type":"object","properties":{"price":{"type":"string","format":"decimal","description":"Cost."},"maybe":{"type":["string","null"],"format":"decimal"},"opt":{"$ref":"#/$defs/Dec"},"list":{"type":"array","items":{"$ref":"#/$defs/Dec"}},"other":{"type":"string"}},"required":["price","maybe"],"additionalProperties":{"type":"string","format":"decimal"},"$defs":{"Dec":{"type":"string","format":"decimal","title":"Dec"}}}"##,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal, r#"{"format":"decimal","type":"string"}"#);
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "Vec<converted_schemas::Decimal>")]
    pub list: Option<Vec<rust_decimal::Decimal>>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[json_schema(required, with = "Option<converted_schemas::Decimal>")]
    pub maybe: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "converted_schemas::Decimal")]
    pub opt: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other: Option<String>,
    /// Cost.
    #[json_schema(with = "converted_schemas::Decimal")]
    pub price: rust_decimal::Decimal,
    #[serde(flatten)]
    #[json_schema(with = "BTreeMap<String, converted_schemas::Decimal>")]
    pub additional: BTreeMap<String, rust_decimal::Decimal>,
}

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_matches_a_union_branch() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(decimal_schema(), "rust_decimal::Decimal"));
        let actual: Result<String, String> = converted(
            r#"{"anyOf":[{"type":"string","format":"decimal"},{"type":"integer"}]}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal, r#"{"format":"decimal","type":"string"}"#);
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Decimal(#[json_schema(with = "converted_schemas::Decimal")] rust_decimal::Decimal),
    Integer(i64),
}

json_schema_rs::deserialize_untagged!(RootAnyOf { Decimal(rust_decimal::Decimal), Integer(i64 = with(json_schema_rs::integer::deserialize)) });

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_union_deserializes_a_converted_branch_after_validating() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .convert(decimal_schema(), "rust_decimal::Decimal")
            .build();
        let actual: Result<String, String> = converted(
            r#"{"anyOf":[{"type":"string","format":"decimal"},{"type":"integer"}]}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal, r#"{"format":"decimal","type":"string"}"#);
}

#[derive(Debug, Clone, PartialEq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Decimal(#[json_schema(with = "converted_schemas::Decimal")] rust_decimal::Decimal),
    Integer(i64),
}

json_schema_rs::deserialize_union!(RootAnyOf, TYPE_VALIDATORS, AnyOf, Some(0), [1, 2], |value| {
    0 => <rust_decimal::Decimal as Deserialize>::deserialize(value).map(Self::Decimal),
    _ => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Integer),
});

json_schema_rs::self_validating!(RootAnyOf, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"anyOf":[{"format":"decimal","type":"string"},{"type":"integer"}]}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
        &["#/anyOf/0"],
        &["#/anyOf/1"],
    ],
);

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_struct_embeds_the_converted_subschema() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .convert(decimal_schema(), "rust_decimal::Decimal")
            .build();
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"price":{"type":"string","format":"decimal"}}}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal, r#"{"format":"decimal","type":"string"}"#);
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "converted_schemas::Decimal")]
    pub price: Option<rust_decimal::Decimal>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"price":{"format":"decimal","type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn converted_root_is_an_alias_to_the_users_type() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(decimal_schema(), "rust_decimal::Decimal"));
        let actual: Result<String, String> =
            converted(r#"{"type":"string","format":"decimal"}"#, &settings);
        let expected: Result<String, String> =
            Ok(r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = rust_decimal::Decimal;

"
            .to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn first_matching_conversion_wins() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.convert(decimal_schema(), "a::Decimal")
                .convert(decimal_schema(), "b::Decimal")
        });
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"price":{"type":"string","format":"decimal"}}}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal0, r#"{"format":"decimal","type":"string"}"#);
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "converted_schemas::Decimal0")]
    pub price: Option<a::Decimal>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_markers_with_the_same_name_get_suffixes() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.convert(decimal_schema(), "a::Decimal").convert(
                serde_json::json!({"type":"number","format":"decimal"}),
                "b::Decimal<'static>",
            )
        });
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"price":{"type":"string","format":"decimal"},"cost":{"type":"number","format":"decimal"}}}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

mod converted_schemas {
    json_schema_rs::schema_marker!(Decimal0, r#"{"format":"decimal","type":"string"}"#);
    json_schema_rs::schema_marker!(Decimal1, r#"{"format":"decimal","type":"number"}"#);
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "converted_schemas::Decimal1")]
    pub cost: Option<b::Decimal<'static>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(with = "converted_schemas::Decimal0")]
    pub price: Option<a::Decimal>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_does_not_match_a_subschema_with_another_keyword() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(decimal_schema(), "rust_decimal::Decimal"));
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"price":{"type":"string","format":"decimal","maxLength":3}}}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(max_length = 3)]
    pub price: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_to_a_non_type_is_an_error() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(decimal_schema(), "not a type"));
        let actual: Result<String, String> = converted(r#"{"type":"string"}"#, &settings);
        let expected: Result<String, String> = Err(
            r"the settings cannot be applied: conversion type `not a type` is not a Rust type"
                .to_string(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_of_a_boolean_schema_is_an_error() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(serde_json::json!(true), "X"));
        let actual: Result<String, String> = converted(r#"{"type":"string"}"#, &settings);
        let expected: Result<String, String> = Err(r"the settings cannot be applied: conversion schema true is a boolean schema, not an object".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_of_an_invalid_schema_is_an_error() {
        let settings: CodeGenSettings =
            customized_settings(|b| b.convert(serde_json::json!({"type":5}), "X"));
        let actual: Result<String, String> = converted(r#"{"type":"string"}"#, &settings);
        let expected: Result<String, String> = Err(r#"the settings cannot be applied: conversion schema {"type":5} does not parse: invalid JSON Schema: invalid type: integer `5`, expected a type name or an array of type names"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn replacement_drops_the_types_only_it_reached() {
        let settings: CodeGenSettings = customized_settings(|b| b.replace("Veg", "my::Veg"));
        let actual: Result<String, String> = converted(
            r##"{"type":"object","properties":{"veg":{"$ref":"#/$defs/Veg"},"inner":{"$ref":"#/$defs/Inner"}},"$defs":{"Veg":{"type":"object","properties":{"inner":{"$ref":"#/$defs/Inner"},"leaf":{"$ref":"#/$defs/Leaf"}}},"Inner":{"type":"object","properties":{"n":{"type":"integer"}}},"Leaf":{"type":"object","properties":{"n":{"type":"integer"}}}}}"##,
            &settings,
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Inner {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner: Option<Inner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veg: Option<my::Veg>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn replacing_a_root_type_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| b.replace("Root", "my::Root"));
        let actual: Result<String, String> = converted(r#"{"type":"object"}"#, &settings);
        let expected: Result<String, String> = Err(r"the settings cannot be applied: replacement for `Root`: a schema's root type cannot be replaced".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn replacing_an_unknown_type_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| b.replace("Nope", "my::Root"));
        let actual: Result<String, String> = converted(r#"{"type":"object"}"#, &settings);
        let expected: Result<String, String> = Err(r"the settings cannot be applied: replacement for `Nope`: no generated type has that name".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn replacement_by_a_non_type_is_an_error() {
        let settings: CodeGenSettings = customized_settings(|b| b.replace("Veg", "1x"));
        let actual: Result<String, String> = converted(
            r##"{"type":"object","properties":{"veg":{"$ref":"#/$defs/Veg"}},"$defs":{"Veg":{"type":"object","properties":{"n":{"type":"integer"}}}}}"##,
            &settings,
        );
        let expected: Result<String, String> = Err(
            r"the settings cannot be applied: replacement for `Veg`: `1x` is not a Rust type"
                .to_string(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_shapes() {
        let texts: [&str; 10] = [
            "rust_decimal::Decimal",
            "::my::Ipv4",
            "my::Wrapper<String, u8>",
            "Cow<'static, str>",
            "Vec<Vec<u8>>",
            "not a type",
            "Vec<>",
            "'a",
            "Vec<u8>>",
            "Vec<u8",
        ];
        let expected: Vec<bool> = vec![
            true, true, true, true, true, false, false, false, false, false,
        ];
        let actual: Vec<bool> = texts
            .iter()
            .map(|text: &&str| super::is_rust_type(text))
            .collect();
        assert_eq!(expected, actual);
    }

    // --- x-rust-type ---

    /// A schema whose `thing` property names `my_types::Thing` (crate `my-types`, `^1.2`).
    const RUST_TYPE_SCHEMA: &str = r#"{"type":"object","properties":{"thing":{"type":"object","properties":{"n":{"type":"integer"}},"x-rust-type":{"crate":"my-types","version":"^1.2","path":"my_types::Thing"}}}}"#;

    fn with_crate(version: &str, rename: Option<&str>) -> CodeGenSettings {
        let spec: crate::CrateSpec = crate::CrateSpec {
            version: version.parse().unwrap(),
            rename: rename.map(str::to_string),
        };
        customized_settings(|b| b.external_crate("my-types", spec))
    }

    fn with_unknown_crates(mode: crate::UnknownCrates) -> CodeGenSettings {
        customized_settings(|b| b.unknown_crates(mode))
    }

    #[test]
    fn rust_type_of_a_listed_crate_at_a_matching_version_is_used() {
        let actual: Result<String, String> =
            converted(RUST_TYPE_SCHEMA, &with_crate("1.4.0", None));
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<::my_types::Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_a_listed_crate_at_another_version_is_generated() {
        let actual: Result<String, String> = converted(RUST_TYPE_SCHEMA, &with_crate("2", None));
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Thing {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_a_crate_at_any_version_is_used() {
        let actual: Result<String, String> = converted(RUST_TYPE_SCHEMA, &with_crate("*", None));
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<::my_types::Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_a_crate_never_used_is_generated_even_when_unknown_crates_are_denied() {
        let spec: crate::CrateSpec = crate::CrateSpec {
            version: crate::CrateVersion::Never,
            rename: None,
        };
        let settings: CodeGenSettings = customized_settings(|b| {
            b.external_crate("my-types", spec)
                .unknown_crates(crate::UnknownCrates::Deny)
        });
        let actual: Result<String, String> = converted(RUST_TYPE_SCHEMA, &settings);
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Thing {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_a_renamed_crate_uses_the_new_name() {
        let actual: Result<String, String> =
            converted(RUST_TYPE_SCHEMA, &with_crate("1.2", Some("types-v1")));
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<::types_v1::Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_is_ignored_by_default() {
        let actual: Result<String, String> =
            converted(RUST_TYPE_SCHEMA, &customized_settings(|b| b));
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Thing {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_an_unknown_crate_is_used_when_allowed() {
        let actual: Result<String, String> = converted(
            RUST_TYPE_SCHEMA,
            &with_unknown_crates(crate::UnknownCrates::Allow),
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<::my_types::Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_of_an_unknown_crate_is_an_error_when_denied() {
        let actual: Result<String, String> = converted(
            RUST_TYPE_SCHEMA,
            &with_unknown_crates(crate::UnknownCrates::Deny),
        );
        let expected: Result<String, String> = Err(r"the settings cannot be applied: `x-rust-type` at `#/properties/thing` names crate `my-types` (version `^1.2`), which is not among the crates the output may use".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_parameters_are_generated_as_generic_arguments() {
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"pair":{"x-rust-type":{"crate":"my-types","version":"1","path":"my_types::Pair","parameters":[{"type":"object","title":"Left","properties":{"n":{"type":"integer"}}},true,{"type":"array","items":{"type":"string"}}]}}}}"#,
            &with_crate("1.0.0", None),
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Left")]
pub struct Left {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub n: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pair: Option<::my_types::Pair<Left, serde_json::Value, Vec<String>>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn rust_type_at_the_root_is_an_alias() {
        let actual: Result<String, String> = converted(
            r#"{"type":"object","x-rust-type":{"crate":"my-types","version":"*","path":"my_types::Thing"}}"#,
            &with_unknown_crates(crate::UnknownCrates::Allow),
        );
        let expected: Result<String, String> =
            Ok(r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = ::my_types::Thing;

"
            .to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn conversion_does_not_match_a_subschema_with_a_rust_type() {
        let settings: CodeGenSettings = customized_settings(|b| {
            b.convert(decimal_schema(), "rust_decimal::Decimal")
                .unknown_crates(crate::UnknownCrates::Allow)
        });
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"price":{"type":"string","format":"decimal","x-rust-type":{"crate":"my-types","version":"*","path":"my_types::Price"}}}}"#,
            &settings,
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<::my_types::Price>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn self_validating_struct_validates_a_rust_type_field_against_its_subschema() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .unknown_crates(crate::UnknownCrates::Allow)
            .build();
        let actual: Result<String, String> = converted(RUST_TYPE_SCHEMA, &settings);
        let expected: Result<String, String> = Ok(r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thing: Option<::my_types::Thing>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"thing":{"properties":{"n":{"type":"integer"}},"type":"object"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn malformed_rust_types_are_errors_once_honoured() {
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"a":{"x-rust-type":{"crate":"my-types","version":"1"}},"b":{"x-rust-type":{"crate":"my-types","version":"one","path":"my_types::B"}},"c":{"x-rust-type":{"crate":"my-types","version":"1","path":"other::C"}},"d":{"x-rust-type":{"crate":"my-types","version":"1","path":"my_types::D","parameters":{}}},"e":{"x-rust-type":{"crate":"my-types","version":"1","path":"my_types::E","parameters":[5]}},"f":{"x-rust-type":{"crate":"my-types","version":"1","path":"my_types::not a type"}}}}"#,
            &with_unknown_crates(crate::UnknownCrates::Allow),
        );
        let expected: Result<String, String> = Err(r"the settings cannot be applied: `x-rust-type` at `#/properties/f`: path `my_types::not a type` is not a Rust path in crate `my-types`; `x-rust-type` at `#/properties/e`: parameter 0 is not a schema object or `true`; `x-rust-type` at `#/properties/d`: `parameters` is not an array; `x-rust-type` at `#/properties/c`: path `other::C` is not a Rust path in crate `my-types`; `x-rust-type` at `#/properties/b`: `one` is not a version requirement: unexpected character 'o' while parsing major version number; `x-rust-type` at `#/properties/a` is malformed: missing field `path`".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn malformed_rust_type_is_ignored_by_default() {
        let actual: Result<String, String> = converted(
            r#"{"type":"object","properties":{"a":{"type":"string","x-rust-type":5}}}"#,
            &customized_settings(|b| b),
        );
        let expected: Result<String, String> = Ok(r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#.to_string());
        assert_eq!(expected, actual);
    }

    // --- map type ---

    /// The per-schema buffer for one schema generated with `map_type` and `unique_items`.
    fn generate_with_map_type(
        schema_json: &str,
        map_type: MapType,
        unique_items: UniqueItemsType,
    ) -> String {
        let schema: Schema = Schema::try_from(schema_json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .self_validating(false)
            .map_type(map_type)
            .unique_items(unique_items)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        String::from_utf8(output.per_schema[0].clone()).unwrap()
    }

    /// A struct with a map property and typed unknown keys.
    const MAPS_SCHEMA: &str = r#"{"type":"object","properties":{"labels":{"type":"object","additionalProperties":{"type":"integer"}}},"additionalProperties":{"type":"string"}}"#;

    /// `HashMap` holds a map property's entries and a struct's unknown keys.
    #[test]
    fn hash_map_type_emits_hash_map_for_maps_and_unknown_keys() {
        let actual: String =
            generate_with_map_type(MAPS_SCHEMA, MapType::HashMap, UniqueItemsType::Vec);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, i64>>,
    #[serde(flatten)]
    pub additional: HashMap<String, String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `IndexMap` holds a map property's entries and a struct's unknown keys.
    #[cfg(feature = "indexmap")]
    #[test]
    fn index_map_type_emits_index_map_for_maps_and_unknown_keys() {
        let actual: String =
            generate_with_map_type(MAPS_SCHEMA, MapType::IndexMap, UniqueItemsType::Vec);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use indexmap::IndexMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub labels: Option<IndexMap<String, i64>>,
    #[serde(flatten)]
    pub additional: IndexMap<String, String>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A root that is only a map becomes an alias of the map type.
    #[test]
    fn hash_map_type_emits_hash_map_root_alias() {
        let actual: String = generate_with_map_type(
            r#"{"type":"object","additionalProperties":{"type":"string"}}"#,
            MapType::HashMap,
            UniqueItemsType::Vec,
        );
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use std::collections::HashMap;

pub type Root = HashMap<String, String>;

";
        assert_eq!(expected, actual);
    }

    /// A struct keeping unknown keys in a `BTreeMap` derives `Hash` and `Ord`, so it is a set
    /// item.
    #[test]
    fn btree_map_type_keeps_struct_with_unknown_keys_a_set_item() {
        let actual: String =
            generate_with_map_type(POINTS_SCHEMA, MapType::BTreeMap, UniqueItemsType::BTreeSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    struct Visitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for Visitor<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(item) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array read into the set `S`.
    struct Unique<S>(S);

    impl<'de, S> Deserialize<'de> for Unique<S>
    where
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(Visitor(std::marker::PhantomData))
                .map(Unique)
        }
    }

    pub(super) fn required<'de, D, S>(deserializer: D) -> Result<S, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| unique.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: BTreeSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A struct keeping unknown keys in a `HashMap` has no `Hash` or `Ord`, so the
    /// `uniqueItems` array of it stays a `Vec`.
    #[test]
    fn hash_map_type_keeps_struct_with_unknown_keys_out_of_sets() {
        let actual: String =
            generate_with_map_type(POINTS_SCHEMA, MapType::HashMap, UniqueItemsType::BTreeSet);
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Tag {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: HashMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub x: i64,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub points: Vec<Point>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A map item has no `Hash` or `Ord` as a `HashMap`, so its `uniqueItems` array stays a
    /// `Vec`.
    #[test]
    fn hash_map_type_keeps_map_items_out_of_sets() {
        let actual: String = generate_with_map_type(
            r#"{"type":"object","properties":{"rows":{"type":"array","items":{"type":"object","additionalProperties":{"type":"string"}},"uniqueItems":true}},"required":["rows"]}"#,
            MapType::HashMap,
            UniqueItemsType::BTreeSet,
        );
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub rows: Vec<HashMap<String, String>>,
    #[serde(flatten)]
    pub additional: HashMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    // --- builders ---

    /// The per-schema buffer for one schema generated with builders, and `self_validating`.
    fn generate_with_builders(schema_json: &str, self_validating: bool) -> String {
        let schema: Schema = Schema::try_from(schema_json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .builders(true)
            .self_validating(self_validating)
            .constrained_newtypes(false)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        String::from_utf8(output.per_schema[0].clone()).unwrap()
    }

    /// Required and optional properties, a named struct field (through `super::`) and the
    /// unknown-keys map (a `use` in the `builder` module).
    #[test]
    fn builders_emit_required_optional_and_additional_fields() {
        let actual: String = generate_with_builders(
            r#"{"title":"Veg","type":"object","required":["name"],"properties":{"name":{"type":"string"},"weight":{"type":"integer","minimum":0,"maximum":255},"origin":{"type":"object","properties":{"country":{"type":"string"}}}},"additionalProperties":{"type":"integer"}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Origin {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub weight: Option<u8>,
    #[serde(flatten, deserialize_with = "json_schema_rs::integer::deserialize")]
    pub additional: BTreeMap<String, i64>,
}

/// The builders of this module's structs (`Name::builder()`).
pub mod builder {
    use std::collections::BTreeMap;

    json_schema_rs::builder!(Origin {
        optional country: String,
        defaulted additional: BTreeMap<String, serde_json::Value>,
    });

    json_schema_rs::builder!(Veg {
        required name: String,
        optional origin: super::Origin,
        optional weight: u8,
        defaulted additional: BTreeMap<String, i64>,
    });
}

"#;
        assert_eq!(expected, actual);
    }

    /// A self-validating struct's builder validates the built struct.
    #[test]
    fn builders_validate_self_validating_struct() {
        let actual: String = generate_with_builders(
            r#"{"type":"object","properties":{"name":{"type":"string","minLength":1}},"additionalProperties":false}"#,
            true,
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(min_length = 1)]
    pub name: Option<String>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"additionalProperties":false,"properties":{"name":{"minLength":1,"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

/// The builders of this module's structs (`Name::builder()`).
pub mod builder {
    json_schema_rs::builder!(@validated Root {
        optional name: String,
    });
}

"##;
        assert_eq!(expected, actual);
    }

    /// A struct without fields gets a builder with no setters.
    #[test]
    fn builders_emit_builder_for_struct_without_fields() {
        let actual: String =
            generate_with_builders(r#"{"type":"object","additionalProperties":false}"#, false);
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
}

/// The builders of this module's structs (`Name::builder()`).
pub mod builder {
    json_schema_rs::builder!(Root {});
}

";
        assert_eq!(expected, actual);
    }

    /// A buffer without structs has no `builder` module.
    #[test]
    fn builders_emit_nothing_without_structs() {
        let actual: String = generate_with_builders(r#"{"type":"string"}"#, false);
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Root = String;

";
        assert_eq!(expected, actual);
    }

    fn generate_with_schema_defaults(schema_json: &str, validating: bool) -> String {
        let schema: Schema = Schema::try_from(schema_json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .schema_defaults(true)
            .self_validating(validating)
            .constrained_newtypes(validating)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        String::from_utf8(output.per_schema[0].clone()).unwrap()
    }

    /// A struct's own `default` becomes its `impl Default`, in place of the empty-object derive.
    #[test]
    fn schema_defaults_struct_own_default() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string"},"weight":{"type":"integer","minimum":0,"maximum":255}},"default":{"name":"carrot","weight":3}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub weight: Option<u8>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::schema_default!(Veg, "{\"name\":\"carrot\",\"weight\":3}");

"#;
        assert_eq!(expected, actual);
    }

    /// A struct without its own `default` defaults to the object of its properties' `default`s.
    #[test]
    fn schema_defaults_struct_from_property_defaults() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string","default":"carrot"},"weight":{"type":"integer","minimum":0,"maximum":255}}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "carrot")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub weight: Option<u8>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Veg {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn name_or_default(&self) -> &str {
        self.name.as_deref().unwrap_or("carrot")
    }
}

json_schema_rs::schema_default!(Veg, "{\"name\":\"carrot\"}");

"#;
        assert_eq!(expected, actual);
    }

    /// Property defaults that leave a required property out are not a valid value: no impl.
    #[test]
    fn schema_defaults_skip_property_defaults_missing_required() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","required":["weight"],"properties":{"name":{"type":"string","default":"carrot"},"weight":{"type":"integer","minimum":0,"maximum":255}}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "carrot")]
    pub name: Option<String>,
    #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
    pub weight: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Veg {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn name_or_default(&self) -> &str {
        self.name.as_deref().unwrap_or("carrot")
    }
}

"#;
        assert_eq!(expected, actual);
    }

    /// A `default` the schema rejects gets no impl (the empty-object derive stays).
    #[test]
    fn schema_defaults_skip_default_invalid_against_schema() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string","minLength":3}},"default":{"name":"x"}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(min_length = 3)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A `default` whose string is not valid for its typed `format` gets no impl, though 2020-12 only annotates `format`.
    #[cfg(feature = "uuid")]
    #[test]
    fn schema_defaults_skip_typed_format_invalid_value() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"id":{"type":"string","format":"uuid"}},"default":{"id":"not a uuid"}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A schema-valid `default` that does not fit the Rust type (an integer past `i64`) gets no impl.
    #[test]
    fn schema_defaults_skip_integer_out_of_rust_range() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"weight":{"type":"integer"}},"default":{"weight":1e30}}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize", skip_serializing_if = "Option::is_none")]
    pub weight: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// A string enum's `default` variant.
    #[test]
    fn schema_defaults_string_enum() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"string","enum":["sweet potato","pea"],"default":"pea"}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Veg {
    #[serde(rename = "pea")]
    Pea,
    #[serde(rename = "sweet potato")]
    SweetPotato,
}

json_schema_rs::string_enum!(Veg);

json_schema_rs::schema_default!(Veg, "\"pea\"");

"#;
        assert_eq!(expected, actual);
    }

    /// A union's `default`, read through its untagged `Deserialize`.
    #[test]
    fn schema_defaults_union() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Size","type":["integer","string"],"default":"large"}"#,
            false,
        );
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Size {
    Integer(i64),
    String(String),
}

json_schema_rs::deserialize_untagged!(Size { Integer(i64 = with(json_schema_rs::integer::deserialize)), String(String) });

json_schema_rs::schema_default!(Size, "\"large\"");

"#;
        assert_eq!(expected, actual);
    }

    /// A constrained newtype's `default`.
    #[test]
    fn schema_defaults_newtype() {
        let actual: String = generate_with_schema_defaults(
            r##"{"title":"Veg","type":"object","properties":{"name":{"$ref":"#/$defs/name"}},"$defs":{"name":{"type":"string","minLength":2,"default":"pea"}}}"##,
            true,
        );
        let expected: &str = r###"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 2)]
pub struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TYPE_VALIDATORS, 0);

json_schema_rs::schema_default!(Name, "\"pea\"");

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Name>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Veg, TYPE_VALIDATORS, 1);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"$defs":{"name":{"minLength":2,"type":"string"}},"properties":{"name":{"$ref":"#/$defs/name"}},"type":"object"}"##,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/$defs/name"],
        &["#"],
    ],
);

"###;
        assert_eq!(expected, actual);
    }

    /// A self-validating struct's `default`.
    #[test]
    fn schema_defaults_self_validating_struct() {
        let actual: String = generate_with_schema_defaults(
            r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string","minLength":2}},"default":{"name":"pea"}}"#,
            true,
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 2)]
pub struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Veg {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Name>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Veg, TYPE_VALIDATORS, 1);

json_schema_rs::schema_default!(Veg, "{\"name\":\"pea\"}");

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"name":{"minLength":2,"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/name"],
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    // --- root name, visibility, shared path, header ---

    /// Every buffer of one generation with `settings`, shared first, joined with a marker line.
    fn generate_buffers(schemas: &[&str], settings: &CodeGenSettings) -> String {
        let parsed: Vec<Schema> = schemas
            .iter()
            .map(|json: &&str| Schema::try_from(*json).unwrap())
            .collect();
        let output: super::GenerateRustOutput = generate_rust(&parsed, settings).unwrap();
        output
            .shared
            .into_iter()
            .chain(output.per_schema)
            .map(|buffer: Vec<u8>| String::from_utf8(buffer).unwrap())
            .collect::<Vec<String>>()
            .join("// ---\n")
    }

    /// The root takes the given name over its title.
    #[test]
    fn root_name_outranks_title() {
        let actual: String = generate_buffers(
            &[r#"{"title":"Veg","type":"object","properties":{"name":{"type":"string"}}}"#],
            &CodeGenSettings::builder().root_name("Config").build(),
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Veg")]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Config, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"name":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

"##;
        assert_eq!(expected, actual);
    }

    /// A given root name is used as written: a union root gets no `OneOf` suffix, and a
    /// `$defs` key with the same name gives way.
    #[test]
    fn root_name_used_as_written() {
        let actual: String = generate_buffers(
            &[
                r##"{"oneOf":[{"type":"integer"},{"$ref":"#/$defs/Config"}],"$defs":{"Config":{"type":"object","properties":{"name":{"type":"string"}}}}}"##,
            ],
            &CodeGenSettings::builder().root_name("Config").build(),
        );
        let expected: &str = r###"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Config0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Config0, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum Config {
    Integer(i64),
    Config0(Config0),
}

json_schema_rs::deserialize_union!(Config, TYPE_VALIDATORS, OneOf, Some(1), [2, 0], |value| {
    0 => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Integer),
    _ => <Config0 as Deserialize>::deserialize(value).map(Self::Config0),
});

json_schema_rs::self_validating!(Config, TYPE_VALIDATORS, 1);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"$defs":{"Config":{"properties":{"name":{"type":"string"}},"type":"object"}},"oneOf":[{"type":"integer"},{"$ref":"#/$defs/Config"}]}"##,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/$defs/Config"],
        &["#"],
        &["#/oneOf/0"],
    ],
);

"###;
        assert_eq!(expected, actual);
    }

    /// A root that is not a named item (a type alias) takes the given name too.
    #[test]
    fn root_name_names_root_alias() {
        let actual: String = generate_buffers(
            &[r#"{"type":"array","items":{"type":"string"}}"#],
            &CodeGenSettings::builder()
                .root_name("Names")
                .self_validating(false)
                .build(),
        );
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

pub type Names = Vec<String>;

";
        assert_eq!(expected, actual);
    }

    /// A root name or shared path that is not Rust is a customization error.
    #[test]
    fn root_name_and_shared_path_rejected_when_not_rust() {
        let schema: Schema = Schema::try_from(r#"{"type":"object"}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .root_name("my-config")
            .shared_path("crate::bad path")
            .build();
        let actual: String = generate_rust(&[schema], &settings).unwrap_err().to_string();
        let expected: &str = r"the settings cannot be applied: root name `my-config` is not a Rust identifier; shared path `crate::bad path` is not a Rust path";
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_paths_rejected_when_not_rust() {
        let schema: Schema = Schema::try_from(r#"{"type":"object"}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .crate_path("my facade")
            .macro_crate_path("::")
            .build();
        let actual: String = generate_rust(&[schema], &settings).unwrap_err().to_string();
        let expected: &str = r"the settings cannot be applied: crate path `my facade` is not a Rust path; macro crate path `::` is not a Rust path";
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_paths_rejected_when_relative() {
        let schema: Schema = Schema::try_from(r#"{"type":"object"}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .crate_path("super::jsr")
            .macro_crate_path("self::macros")
            .build();
        let actual: String = generate_rust(&[schema], &settings).unwrap_err().to_string();
        let expected: &str = r"the settings cannot be applied: crate path `super::jsr` must name a crate: an extern crate path (`my_sdk::jsr`, `::my_sdk`) or a `crate::` path; macro crate path `self::macros` must name a crate: an extern crate path (`my_sdk::jsr`, `::my_sdk`) or a `crate::` path";
        assert_eq!(expected, actual);
    }

    #[test]
    fn crate_path_rejected_with_keyword_segment() {
        let schema: Schema = Schema::try_from(r#"{"type":"object"}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().crate_path("jsr::type").build();
        let actual: String = generate_rust(&[schema], &settings).unwrap_err().to_string();
        let expected: &str =
            r"the settings cannot be applied: crate path `jsr::type` is not a Rust path";
        assert_eq!(expected, actual);
    }

    /// With crate paths set, every path to the library and to the derives goes through them:
    /// the derives, the `#[json_schema(crate = ...)]` attribute, the macros, `Nullable`, the
    /// integer readers, the unique-items helpers and the embedded validators.
    #[test]
    #[expect(clippy::too_many_lines)]
    fn crate_paths_reach_every_library_item() {
        let schema: &str = r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1},"kind":{"enum":["a","b"]},"size":{"anyOf":[{"type":"integer"},{"type":"string"}]},"note":{"type":["string","null"]},"ids":{"type":"array","items":{"type":"integer"},"uniqueItems":true},"level":{"enum":[1,"x"]}}}"#;
        let actual: String = generate_buffers(
            &[schema],
            &CodeGenSettings::builder()
                .crate_path("crate::facade::jsr")
                .macro_crate_path("crate::facade::jsr_macro")
                .unique_items(UniqueItemsType::BTreeSet)
                .builders(true)
                .build(),
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }

    impl<T: Ord> UniqueSet for std::collections::BTreeSet<T> {
        type Item = T;
        fn with_capacity(_capacity: usize) -> Self {
            Self::new()
        }
        fn insert_new(&mut self, item: T) -> bool {
            self.insert(item)
        }
    }

    /// Reads integer items by JSON Schema's integer rule (`1.0` is the integer 1).
    struct IntegerVisitor<S>(std::marker::PhantomData<S>);

    impl<'de, S> serde::de::Visitor<'de> for IntegerVisitor<S>
    where
        S: UniqueSet,
        S::Item: crate::facade::jsr::integer::DeserializeInteger<'de>,
    {
        type Value = S;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an array with unique items")
        }

        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<S, A::Error> {
            let mut set: S = S::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
            let mut index: usize = 0;
            while let Some(crate::facade::jsr::integer::Integer(item)) = seq.next_element()? {
                if !set.insert_new(item) {
                    return Err(serde::de::Error::custom(format_args!(
                        "duplicate item at index {index} of an array with uniqueItems"
                    )));
                }
                index += 1;
            }
            Ok(set)
        }
    }

    /// An array of integers read into the set `S`.
    struct UniqueIntegers<S>(S);

    impl<'de, S> Deserialize<'de> for UniqueIntegers<S>
    where
        S: UniqueSet,
        S::Item: crate::facade::jsr::integer::DeserializeInteger<'de>,
    {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer
                .deserialize_seq(IntegerVisitor(std::marker::PhantomData))
                .map(UniqueIntegers)
        }
    }

    pub(super) fn optional_integer<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: crate::facade::jsr::integer::DeserializeInteger<'de>,
    {
        UniqueIntegers::deserialize(deserializer).map(|unique: UniqueIntegers<S>| Some(unique.0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, crate::facade::jsr_macro::ToJsonSchema)]
#[json_schema(crate = "crate::facade::jsr")]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    A,
    B,
}

crate::facade::jsr::string_enum!(Kind);

crate::facade::jsr::self_validating!(Kind, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, crate::facade::jsr_macro::ToJsonSchema)]
#[json_schema(crate = "crate::facade::jsr")]
pub enum Level {
    #[json_schema(value = "x")]
    X,
    #[json_schema(value = 1)]
    N1,
}

impl Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value: serde_json::Value = match self {
            Self::X => serde_json::json!("x"),
            Self::N1 => serde_json::json!(1),
        };
        value.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if crate::facade::jsr::json_equal(&value, &serde_json::json!("x")) {
            return Ok(Self::X);
        }
        if crate::facade::jsr::json_equal(&value, &serde_json::json!(1)) {
            return Ok(Self::N1);
        }
        Err(serde::de::Error::custom(format!("value {value} not in enum (allowed: {})", "\"x\", 1")))
    }
}

crate::facade::jsr::self_validating!(Level, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, crate::facade::jsr_macro::ToJsonSchema)]
#[json_schema(crate = "crate::facade::jsr")]
#[serde(transparent)]
#[json_schema(min_length = 1)]
pub struct Name(String);

crate::facade::jsr::constrained_newtype!(string, Name, TYPE_VALIDATORS, 2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, crate::facade::jsr_macro::ToJsonSchema)]
#[json_schema(crate = "crate::facade::jsr")]
#[serde(untagged)]
pub enum SizeAnyOf {
    Integer(i64),
    String(String),
}

crate::facade::jsr::deserialize_union!(SizeAnyOf, TYPE_VALIDATORS, AnyOf, Some(3), [4, 5], |value| {
    0 => crate::facade::jsr::integer::deserialize::<_, i64>(value).map(Self::Integer),
    _ => <String as Deserialize>::deserialize(value).map(Self::String),
});

crate::facade::jsr::self_validating!(SizeAnyOf, TYPE_VALIDATORS, 3);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, crate::facade::jsr_macro::Deserialize, crate::facade::jsr_macro::ToJsonSchema)]
#[json_schema(crate = "crate::facade::jsr")]
pub struct Root {
    #[serde(default, deserialize_with = "unique_items::optional_integer", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub ids: Option<BTreeSet<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    pub name: Name,
    #[serde(default, skip_serializing_if = "crate::facade::jsr::Nullable::is_absent")]
    pub note: crate::facade::jsr::Nullable<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SizeAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

crate::facade::jsr::self_validating!(Root, TYPE_VALIDATORS, 6);

static TYPE_VALIDATORS: crate::facade::jsr::TypeValidators<'static> = crate::facade::jsr::TypeValidators::new(
    None,
    &[
        crate::facade::jsr::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"ids":{"items":{"type":"integer"},"type":"array","uniqueItems":true},"kind":{"enum":["a","b"]},"level":{"enum":[1,"x"]},"name":{"minLength":1,"type":"string"},"note":{"type":["null","string"]},"size":{"anyOf":[{"type":"integer"},{"type":"string"}]}},"required":["name"],"type":"object"}"#,
            kind: crate::facade::jsr::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/kind"],
        &["#/properties/level"],
        &["#/properties/name"],
        &["#/properties/size"],
        &["#/properties/size/anyOf/0"],
        &["#/properties/size/anyOf/1"],
        &["#"],
    ],
);

/// The builders of this module's structs (`Name::builder()`).
pub mod builder {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    crate::facade::jsr::builder!(@validated Root {
        optional ids: BTreeSet<i64>,
        optional kind: super::Kind,
        optional level: super::Level,
        required name: super::Name,
        defaulted note: crate::facade::jsr::Nullable<String>,
        optional size: super::SizeAnyOf,
        defaulted additional: BTreeMap<String, serde_json::Value>,
    });
}

"##;
        assert_eq!(expected, actual);
    }

    /// Crate visibility on every kind of item: structs and their fields, string enums,
    /// unions, constrained newtypes, the builder module and the re-exports of shared items.
    #[test]
    fn visibility_crate_on_every_item() {
        let schema: &str = r#"{"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1},"kind":{"enum":["a","b"]},"size":{"anyOf":[{"type":"integer"},{"type":"string"}]}}}"#;
        let actual: String = generate_buffers(
            &[schema, schema],
            &CodeGenSettings::builder()
                .visibility(Visibility::Crate)
                .builders(true)
                .dedupe_mode(DedupeMode::Full)
                .build(),
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    A,
    B,
}

json_schema_rs::string_enum!(Kind);

json_schema_rs::self_validating!(Kind, TYPE_VALIDATORS, 0);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
#[json_schema(min_length = 1)]
pub(crate) struct Name(String);

json_schema_rs::constrained_newtype!(string, Name, TYPE_VALIDATORS, 1);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub(crate) enum SizeAnyOf {
    Integer(i64),
    String(String),
}

json_schema_rs::deserialize_union!(SizeAnyOf, TYPE_VALIDATORS, AnyOf, Some(2), [3, 4], |value| {
    0 => json_schema_rs::integer::deserialize::<_, i64>(value).map(Self::Integer),
    _ => <String as Deserialize>::deserialize(value).map(Self::String),
});

json_schema_rs::self_validating!(SizeAnyOf, TYPE_VALIDATORS, 2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub(crate) struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<Kind>,
    pub(crate) name: Name,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) size: Option<SizeAnyOf>,
    #[serde(flatten)]
    pub(crate) additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 5);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"kind":{"enum":["a","b"]},"name":{"minLength":1,"type":"string"},"size":{"anyOf":[{"type":"integer"},{"type":"string"}]}},"required":["name"],"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#/properties/kind"],
        &["#/properties/name"],
        &["#/properties/size"],
        &["#/properties/size/anyOf/0"],
        &["#/properties/size/anyOf/1"],
        &["#"],
    ],
);

/// The builders of this module's structs (`Name::builder()`).
pub(crate) mod builder {
    use std::collections::BTreeMap;

    json_schema_rs::builder!(@validated Root {
        optional kind: super::Kind,
        required name: super::Name,
        optional size: super::SizeAnyOf,
        defaulted additional: BTreeMap<String, serde_json::Value>,
    });
}

// ---
//! Generated by json-schema-rs. Do not edit manually.

pub(crate) use super::shared::Root;

// ---
//! Generated by json-schema-rs. Do not edit manually.

pub(crate) use super::shared::Root;

"##;
        assert_eq!(expected, actual);
    }

    /// A root type alias and a root newtype are crate-visible too.
    #[test]
    fn visibility_crate_on_root_alias_and_newtype() {
        let actual: String = generate_buffers(
            &[
                r#"{"type":"array","items":{"type":"string"}}"#,
                r#"{"type":"array","items":{"type":"string"},"minItems":1}"#,
            ],
            &CodeGenSettings::builder()
                .visibility(Visibility::Crate)
                .build(),
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"items":{"type":"string"},"type":"array"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub(crate) struct Root(pub(crate) Vec<String>);

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

// ---
//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"items":{"type":"string"},"minItems":1,"type":"array"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(transparent)]
pub(crate) struct Root(pub(crate) Vec<String>);

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

"##;
        assert_eq!(expected, actual);
    }

    /// Per-schema buffers re-export the shared items through the given path.
    #[test]
    fn shared_path_used_by_re_exports() {
        let schema: &str = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let actual: String = generate_buffers(
            &[schema, schema],
            &CodeGenSettings::builder()
                .dedupe_mode(DedupeMode::Full)
                .shared_path("crate::models::common")
                .build(),
        );
        let expected: &str = r##"//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"id":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

// ---
//! Generated by json-schema-rs. Do not edit manually.

pub use crate::models::common::Root;

// ---
//! Generated by json-schema-rs. Do not edit manually.

pub use crate::models::common::Root;

"##;
        assert_eq!(expected, actual);
    }

    /// The header opens every buffer, one `//` comment per line, an empty line as `//`.
    #[test]
    fn header_opens_every_buffer() {
        let schema: &str = r#"{"type":"object","properties":{"id":{"type":"string"}}}"#;
        let actual: String = generate_buffers(
            &[schema, schema],
            &CodeGenSettings::builder()
                .dedupe_mode(DedupeMode::Full)
                .header("Copyright Example.\n\nRegenerate with `make models`.")
                .build(),
        );
        let expected: &str = r##"// Copyright Example.
//
// Regenerate with `make models`.
//! Generated by json-schema-rs. Do not edit manually.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, json_schema_rs_macro::Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

json_schema_rs::self_validating!(Root, TYPE_VALIDATORS, 0);

static TYPE_VALIDATORS: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(
    None,
    &[
        json_schema_rs::EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"id":{"type":"string"}},"type":"object"}"#,
            kind: json_schema_rs::EmbeddedDocumentKind::Parsed(None),
        },
    ],
    &[
        &["#"],
    ],
);

// ---
// Copyright Example.
//
// Regenerate with `make models`.
//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

// ---
// Copyright Example.
//
// Regenerate with `make models`.
//! Generated by json-schema-rs. Do not edit manually.

pub use super::shared::Root;

"##;
        assert_eq!(expected, actual);
    }
}
