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
use super::input::{CachingRetriever, CodeGenInput};
use super::settings::{CodeGenSettings, DedupeMode, UniqueItemsType};
use super::type_graph::passes;
use super::type_graph::{
    self, Annotations, EnumNode, Field, FloatWidth, IntWidth, NameProposal, NameTier, ObjectNode,
    StringFormat, TypeGraph, TypeId, TypeNode, UnionKind, UnionNode,
};
use crate::json_schema::{Registry, Schema};
use crate::number;
use crate::sanitizers::{
    enum_variant_name_from_value, sanitize_field_name, sanitize_struct_name, to_pascal_case,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
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
        let conditionals: type_graph::Conditionals =
            type_graph::Conditionals::collect(&schemas, &registries)?;
        let mut graph: TypeGraph =
            type_graph::build(&schemas, &registries, &conditionals, settings)?;
        passes::collapse_wrapper_cycles(&mut graph);
        passes::order_variants(&mut graph);
        passes::dedup(&mut graph, settings.dedupe_mode);
        passes::break_cycles(&mut graph);
        let plan: Plan = Plan::new(&graph, settings);
        let shared: Option<Vec<u8>> = if plan.has_shared() {
            Some(render_buffer(&graph, &plan, Home::Shared)?)
        } else {
            None
        };
        let per_schema: Vec<Vec<u8>> = (0..schemas.len())
            .map(|index| render_buffer(&graph, &plan, Home::Schema(index)))
            .collect::<CodeGenResult<Vec<Vec<u8>>>>()?;
        Ok(GenerateRustOutput { shared, per_schema })
    }
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
const RESERVED_NAMES: [&str; 16] = [
    "BTreeMap",
    "BTreeSet",
    "Box",
    "Deserialize",
    "Err",
    "HashSet",
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
    /// Per node, whether it can be a set item (all `false` without a set type).
    set_item: Vec<bool>,
    /// The structs and unions that derive `Hash`, `Eq` and `Ord`.
    ord_items: BTreeSet<TypeId>,
}

impl Plan {
    fn new(graph: &TypeGraph, settings: &CodeGenSettings) -> Plan {
        let reach: Vec<BTreeSet<TypeId>> = passes::reachable(graph);
        // Which schemas use each named node.
        let mut users: BTreeMap<TypeId, BTreeSet<usize>> = BTreeMap::new();
        for (schema, ids) in reach.iter().enumerate() {
            for &id in ids {
                if graph.node(id).is_named() {
                    users.entry(id).or_default().insert(schema);
                }
            }
        }
        // Shared: used by two or more schemas, and everything such an item reaches.
        let mut shared: BTreeSet<TypeId> = BTreeSet::new();
        if settings.dedupe_mode != DedupeMode::Disabled {
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
                if graph.node(id).is_named() {
                    shared.insert(id);
                }
                stack.extend(graph.node(id).children());
            }
        }
        let uses_shared: Vec<bool> = reach
            .iter()
            .map(|ids| ids.iter().any(|id| shared.contains(id)))
            .collect();
        let home_of = |id: TypeId| -> Home {
            if shared.contains(&id) {
                Home::Shared
            } else {
                Home::Schema(*users[&id].first().expect("a named node has a user"))
            }
        };
        // Every item that wants a name: (best proposal, key, home).
        let mut candidates: Vec<(NameProposal, ItemKey, Home)> = Vec::new();
        for &id in users.keys() {
            candidates.push((best_proposal(graph, id), ItemKey::Node(id), home_of(id)));
        }
        for (schema, roots) in graph.schemas.iter().enumerate() {
            if !graph.node(graph.peel(roots.root)).is_named() {
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
        let names: Vec<String> = allocate_names(&wanted, clash);
        let mut items: HashMap<TypeId, (Home, String)> = HashMap::new();
        let mut root_aliases: Vec<Option<String>> = vec![None; graph.schemas.len()];
        for ((_, key, home), name) in candidates.into_iter().zip(names) {
            match key {
                ItemKey::Node(id) => {
                    items.insert(id, (home, name));
                }
                ItemKey::RootAlias(schema) => root_aliases[schema] = Some(name),
            }
        }
        let set_kind: Option<SetKind> = SetKind::from_setting(settings.unique_items);
        let set_item: Vec<bool> = if set_kind.is_some() {
            set_item_nodes(graph)
        } else {
            vec![false; graph.nodes.len()]
        };
        let ord_items: BTreeSet<TypeId> = ord_items(graph, &set_item);
        Plan {
            items,
            root_aliases,
            set_kind,
            set_item,
            ord_items,
        }
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
        })
}

/// The base name of a named node from its best proposal: a union reached through a property or
/// title gets an `AnyOf` / `OneOf` suffix; a `$defs` key is used as written.
fn item_base_name(graph: &TypeGraph, id: TypeId, proposal: &NameProposal) -> String {
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

/// Final names for `wanted` items (base name and home, in allocation order). Items whose base
/// names are equal and that `clash` (would be visible in one module) form a group; when a group
/// has two or more items, or its name is reserved, every item in it gets a numeric suffix
/// (`Name0`, `Name1`) in allocation order. A suffixed name already used where the item is
/// visible moves to the next number.
fn allocate_names(wanted: &[(String, Home)], clash: impl Fn(Home, Home) -> bool) -> Vec<String> {
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
    for root in &roots {
        *group_size.entry(*root).or_insert(0) += 1;
    }
    let suffixed: Vec<bool> = wanted
        .iter()
        .zip(&roots)
        .map(|((base, _), root)| group_size[root] > 1 || RESERVED_NAMES.contains(&base.as_str()))
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
fn unique_idents(bases: &[String], separator: &str) -> Vec<String> {
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
    /// The set types `uniqueItems` fields use (each gets a `UniqueSet` impl).
    unique_sets: BTreeSet<SetKind>,
    /// The `unique_items` deserialize functions fields use.
    unique_shapes: BTreeSet<FieldShape>,
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

/// Renders the Rust type of `id` as seen from `home`, recording imports. Iterative: a type is
/// rendered as a token stream of open / close parts so deep nesting cannot overflow.
fn rust_type(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    imports: &mut Imports,
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
            out.push_str(name);
            if *item_home == Home::Shared && home != Home::Shared {
                imports.shared.insert(name.clone());
            }
            continue;
        }
        match graph.node(id) {
            TypeNode::Pending | TypeNode::Any => out.push_str("serde_json::Value"),
            TypeNode::Null => out.push_str("()"),
            TypeNode::Boolean => out.push_str("bool"),
            TypeNode::Integer(w) => out.push_str(int_name(*w)),
            TypeNode::Number(FloatWidth::F32) => out.push_str("f32"),
            TypeNode::Number(FloatWidth::F64) => out.push_str("f64"),
            TypeNode::String { format, .. } => out.push_str(string_type(*format, imports)),
            TypeNode::Array { items, .. } => {
                out.push_str("Vec<");
                stack.push(Part::Text(">"));
                stack.push(Part::Node(*items));
            }
            TypeNode::Map { values } => {
                imports.uses.insert("std::collections::BTreeMap");
                out.push_str("BTreeMap<String, ");
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
            TypeNode::Object(_) | TypeNode::Enum(_) | TypeNode::Union(_) => {
                unreachable!("every reachable named node has a planned name")
            }
        }
    }
    out
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

/// The Rust type for a string: `Uuid` for `format: "uuid"` with the `uuid` feature, else
/// `String`.
fn string_type(format: StringFormat, imports: &mut Imports) -> &'static str {
    match format {
        #[cfg(feature = "uuid")]
        StringFormat::Uuid => {
            imports.uses.insert("uuid::Uuid");
            "Uuid"
        }
        #[cfg(not(feature = "uuid"))]
        StringFormat::Uuid => {
            let _ = imports;
            "String"
        }
        StringFormat::Plain => "String",
    }
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
/// Blocked: any value (`serde_json::Value`, no `Hash`), floats (no `Eq`), a `Uuid` (parsing
/// folds case and accepts several forms, so different strings are equal `Uuid`s), a struct that
/// drops unknown keys (two objects differing only there would be equal) or keeps them as
/// `serde_json::Value`, a struct with a set field (sets ignore order, arrays do not), and a
/// union where one branch reaches an integer type and another a whole number in an enum: `1`
/// and `1.0` are equal JSON, but the integer type rejects `1.0`, so they can deserialize to
/// different variants.
fn set_item_nodes(graph: &TypeGraph) -> Vec<bool> {
    let mut parents: Vec<Vec<TypeId>> = vec![Vec::new(); graph.nodes.len()];
    for (i, node) in graph.nodes.iter().enumerate() {
        let id: TypeId = TypeId(u32::try_from(i).expect("fewer than 2^32 nodes"));
        for child in node.children() {
            parents[child.index()].push(id);
        }
    }
    let integer: Vec<bool> = reaching(graph, &parents, |n| matches!(n, TypeNode::Integer(_)));
    let whole_enum: Vec<bool> = reaching(
        graph,
        &parents,
        |n| matches!(n, TypeNode::Enum(e) if has_whole_number(e)),
    );
    let blocked = |node: &TypeNode| -> bool {
        match node {
            TypeNode::Pending | TypeNode::Any | TypeNode::Number(_) => true,
            #[cfg(feature = "uuid")]
            TypeNode::String {
                format: StringFormat::Uuid,
                ..
            } => true,
            TypeNode::Object(o) => {
                (!o.deny_unknown && o.additional.is_none())
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

/// Doc comment lines: empty if `s` is `None` or blank; else one per non-empty trimmed line.
fn doc_lines(s: Option<&str>) -> Vec<String> {
    let Some(trimmed) = s.map(str::trim) else {
        return Vec::new();
    };
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // A bare carriage return is not allowed in a Rust doc comment.
        .map(|line| line.replace('\r', " "))
        .collect()
}

/// Doc comment lines for an `# Examples` section (compact JSON per example).
fn examples_doc_lines(examples: Option<&[Value]>) -> Vec<String> {
    let Some(examples) = examples.filter(|e| !e.is_empty()) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = vec![String::new(), "# Examples".to_string(), String::new()];
    lines.extend(examples.iter().map(Value::to_string));
    lines
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

/// Doc comments for an item from its annotations (description, then examples).
fn item_docs(annotations: &Annotations) -> String {
    let mut out: String = String::new();
    for line in doc_lines(annotations.description.as_deref()) {
        let _ = writeln!(out, "/// {line}");
    }
    for line in examples_doc_lines(annotations.examples.as_deref()) {
        if line.is_empty() {
            out.push_str("///\n");
        } else {
            let _ = writeln!(out, "/// {line}");
        }
    }
    out
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
        (TypeNode::Integer(width), Value::Number(n)) => {
            // An integer value (`3` or `3.0`): its ceiling and floor agree.
            let exact: i128 = number::integer_lower_bound(n, false)?;
            if number::integer_upper_bound(n, false)? != exact {
                return None;
            }
            let (lo, hi): (i128, i128) = int_bounds(*width);
            (lo..=hi).contains(&exact).then(|| exact.to_string())
        }
        (TypeNode::Number(width), Value::Number(n)) => {
            let f: f64 = n.as_f64()?;
            #[expect(clippy::cast_possible_truncation, reason = "checks the value fits f32")]
            let fits: bool = *width == FloatWidth::F64 || (f as f32).is_finite();
            fits.then(|| format!("{f:?}"))
        }
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
            format!("self.{name}.unwrap_or(Some({literal}))"),
        ),
        (FieldShape::OptionalNullable, true) => (
            "Option<&str>".to_string(),
            format!(
                "match &self.{name} {{\n            Some(value) => value.as_deref(),\n            None => Some({literal}),\n        }}"
            ),
        ),
        (FieldShape::Plain | FieldShape::RequiredNullable, _) => return None,
    };
    Some(format!(
        "    /// The field's value, or its schema `default` when the key is absent.\n    #[must_use]\n    pub fn {name}_or_default(&self) -> {ret} {{\n        {body}\n    }}\n"
    ))
}

/// The values an integer width holds.
fn int_bounds(width: IntWidth) -> (i128, i128) {
    match width {
        IntWidth::U8 => (0, i128::from(u8::MAX)),
        IntWidth::U16 => (0, i128::from(u16::MAX)),
        IntWidth::U32 => (0, i128::from(u32::MAX)),
        IntWidth::U64 => (0, i128::from(u64::MAX)),
        IntWidth::I8 => (i128::from(i8::MIN), i128::from(i8::MAX)),
        IntWidth::I16 => (i128::from(i16::MIN), i128::from(i16::MAX)),
        IntWidth::I32 => (i128::from(i32::MIN), i128::from(i32::MAX)),
        IntWidth::I64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
    }
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
    /// Optional, nullable: `Option<Option<T>>`: `None` absent, `Some(None)` `null`.
    OptionalNullable,
}

/// One rendered field.
#[derive(Debug)]
struct RenderedField {
    name: String,
    ty: String,
    attrs: String,
    /// `pub fn <name>_or_default(&self) -> T { ... }` when the field has a default accessor.
    default_accessor: Option<String>,
}

/// Renders the fields of `object`.
fn render_fields(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    object: &ObjectNode,
    imports: &mut Imports,
) -> Vec<RenderedField> {
    let mut bases: Vec<String> = object
        .fields
        .iter()
        .map(|f| sanitize_field_name(&f.json_key))
        .collect();
    if object.additional.is_some() {
        bases.push("additional".to_string());
    }
    let names: Vec<String> = unique_idents(&bases, "");
    let mut out: Vec<RenderedField> = Vec::new();
    for (field, name) in object.fields.iter().zip(&names) {
        out.push(render_field(graph, plan, home, field, name, imports));
    }
    if let Some(additional) = object.additional {
        let value: String = rust_type(graph, plan, home, additional, imports);
        imports.uses.insert("std::collections::BTreeMap");
        out.push(RenderedField {
            name: names.last().expect("additional field name").clone(),
            ty: format!("BTreeMap<String, {value}>"),
            attrs: "    #[serde(flatten)]\n".to_string(),
            default_accessor: None,
        });
    }
    out
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
    shape: FieldShape,
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
        imports.unique_shapes.insert(shape);
        let item_ty: String = rust_type(graph, plan, home, *items, imports);
        return (format!("{set_name}<{item_ty}>"), Some(kind));
    }
    (rust_type(graph, plan, home, value_id, imports), None)
}

/// The serde arguments for a field's presence and nullability (`shape`), reading a set field
/// through the `unique_items` helper that rejects a duplicate item.
fn shape_serde_args(
    shape: FieldShape,
    set: Option<SetKind>,
    imports: &mut Imports,
    serde_args: &mut Vec<String>,
) {
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
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
        (FieldShape::Plain, None) => {}
        (FieldShape::Optional, None) => {
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
        (FieldShape::RequiredNullable, None) => {
            serde_args.push("deserialize_with = \"Option::deserialize\"".to_string());
        }
        (FieldShape::OptionalNullable, None) => {
            imports.deserialize_some = true;
            serde_args.push("default".to_string());
            serde_args.push("deserialize_with = \"deserialize_some\"".to_string());
            serde_args.push("skip_serializing_if = \"Option::is_none\"".to_string());
        }
    }
}

fn render_field(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    field: &Field,
    name: &str,
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
        (false, true, _) | (false, false, true) => FieldShape::OptionalNullable,
    };
    let (value_ty, set): (String, Option<SetKind>) =
        field_value_type(graph, plan, home, value_id, shape, imports);
    let ty: String = match shape {
        FieldShape::Plain => value_ty.clone(),
        FieldShape::Optional | FieldShape::RequiredNullable => format!("Option<{value_ty}>"),
        // `T` already holds `null`: one `Option` marks absence.
        FieldShape::OptionalNullable if holds_null => format!("Option<{value_ty}>"),
        FieldShape::OptionalNullable => format!("Option<Option<{value_ty}>>"),
    };
    let mut serde_args: Vec<String> = Vec::new();
    if name != field.json_key {
        serde_args.push(format!("rename = \"{}\"", escape(&field.json_key)));
    }
    // `default` is an annotation: deserializing never fills it in (absent stays absent, and a
    // required key stays required). An optional scalar field gets an accessor that applies it.
    let default_accessor: Option<String> = field.default.as_ref().and_then(|value: &Value| {
        let node: &TypeNode = graph.node(value_id);
        let literal: String = default_literal(value, node)?;
        render_default_accessor(name, shape, node, &value_ty, &literal)
    });
    shape_serde_args(shape, set, imports, &mut serde_args);
    let mut attrs: String = String::new();
    for line in doc_lines(field.description.as_deref()) {
        let _ = writeln!(attrs, "    /// {line}");
    }
    attrs.push_str(&deprecated_attr(
        field.deprecated,
        field.description.as_deref(),
        "    ",
    ));
    if !serde_args.is_empty() {
        let _ = writeln!(attrs, "    #[serde({})]", serde_args.join(", "));
    }
    let mut schema_args: Vec<String> = Vec::new();
    if shape == FieldShape::RequiredNullable {
        schema_args.push("required".to_string());
    }
    match graph.node(value_id) {
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
    if let Some(arg) = field.default.as_ref().and_then(default_annotation) {
        schema_args.push(arg);
    }
    if !schema_args.is_empty() {
        let _ = writeln!(attrs, "    #[json_schema({})]", schema_args.join(", "));
    }
    RenderedField {
        name: name.to_string(),
        ty,
        attrs,
        default_accessor,
    }
}

const STRUCT_DERIVE: &str =
    "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]";
/// A struct or union that is (or is reached by) a set item: Rust equality, hashing and order
/// agree with JSON equality for it (see [`set_item_nodes`]).
const ORD_STRUCT_DERIVE: &str = "#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]";
/// Unit variants, one per distinct JSON value: equality, hashing and order are exact and cheap.
const STRING_ENUM_DERIVE: &str = "#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]";
const VALUE_ENUM_DERIVE: &str = "#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]";

/// The derive line for the struct or union `id`.
fn struct_derive(plan: &Plan, id: TypeId) -> &'static str {
    if plan.ord_items.contains(&id) {
        ORD_STRUCT_DERIVE
    } else {
        STRUCT_DERIVE
    }
}

fn render_struct(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    name: &str,
    object: &ObjectNode,
    imports: &mut Imports,
) -> String {
    let fields: Vec<RenderedField> = render_fields(graph, plan, home, object, imports);
    let default_accessors: Vec<String> = fields
        .iter()
        .filter_map(|f| f.default_accessor.clone())
        .collect();
    let mut out: String = String::new();
    let a: &Annotations = &object.annotations;
    out.push_str(&item_docs(a));
    out.push_str(&deprecated_attr(a.deprecated, a.description.as_deref(), ""));
    out.push_str(struct_derive(plan, id));
    out.push('\n');
    if let Some(ref title) = a.title {
        let _ = writeln!(out, "#[json_schema(title = \"{}\")]", escape(title));
    }
    if let Some(ref id) = a.id {
        let _ = writeln!(out, "#[json_schema(id = \"{}\")]", escape(id));
    }
    if object.deny_unknown {
        out.push_str("#[serde(deny_unknown_fields)]\n");
    }
    let _ = writeln!(out, "pub struct {name} {{");
    for field in fields {
        out.push_str(&field.attrs);
        let _ = writeln!(out, "    pub {}: {},", field.name, field.ty);
    }
    out.push_str("}\n\n");
    if !default_accessors.is_empty() {
        let _ = writeln!(out, "impl {name} {{");
        out.push_str(&default_accessors.join("\n"));
        out.push_str("}\n\n");
    }
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

fn render_enum(name: &str, e: &EnumNode) -> String {
    let bases: Vec<String> = e.values.iter().map(value_variant_base).collect();
    let variants: Vec<String> = unique_idents(&bases, "");
    let mut out: String = item_docs(&e.annotations);
    if e.is_string_enum() {
        out.push_str(STRING_ENUM_DERIVE);
        out.push('\n');
        let _ = writeln!(out, "pub enum {name} {{");
        for (value, variant) in e.values.iter().zip(&variants) {
            let raw: &str = value.as_str().expect("string enum value");
            if raw != variant {
                let _ = writeln!(out, "    #[serde(rename = \"{}\")]", escape(raw));
            }
            let _ = writeln!(out, "    {variant},");
        }
        out.push_str("}\n\n");
        return out;
    }
    // Values of other JSON types: serde goes through `serde_json::Value` and compares by JSON
    // Schema equality (`1` equals `1.0`).
    out.push_str(VALUE_ENUM_DERIVE);
    out.push('\n');
    let _ = writeln!(out, "pub enum {name} {{");
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
            "        if json_schema_rs::json_equal(&value, &serde_json::json!({})) {{\n            return Ok(Self::{variant});\n        }}",
            json_tokens(value)
        );
    }
    let _ = writeln!(
        out,
        "        Err(serde::de::Error::custom(format!(\"{{value}} is not a {name} value\")))\n    }}\n}}\n"
    );
    out
}

/// The variant name for a union branch: its title, the named type a `$ref` points to, the
/// kind of a scalar, else `Variant{i}`.
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
    let kind: Option<&str> = match graph.node(target) {
        TypeNode::Null => Some("Null"),
        TypeNode::Boolean => Some("Boolean"),
        TypeNode::Integer(_) => Some("Integer"),
        TypeNode::Number(_) => Some("Number"),
        TypeNode::String { .. } => Some("String"),
        TypeNode::Array { .. } => Some("Array"),
        TypeNode::Map { .. } => Some("Object"),
        TypeNode::Any => Some("Value"),
        TypeNode::Object(_) if union.kind == UnionKind::TypeArray => Some("Object"),
        _ => None,
    };
    kind.map_or_else(|| format!("Variant{index}"), str::to_string)
}

fn render_union(
    graph: &TypeGraph,
    plan: &Plan,
    home: Home,
    id: TypeId,
    name: &str,
    union: &UnionNode,
    imports: &mut Imports,
) -> String {
    let bases: Vec<String> = (0..union.variants.len())
        .map(|i| variant_base(graph, plan, union, i))
        .collect();
    let names: Vec<String> = unique_idents(&bases, "");
    let mut out: String = item_docs(&union.annotations);
    out.push_str(struct_derive(plan, id));
    out.push('\n');
    out.push_str("#[serde(untagged)]\n");
    if union.kind == UnionKind::OneOf {
        out.push_str("#[json_schema(one_of)]\n");
    }
    let _ = writeln!(out, "pub enum {name} {{");
    for (variant, variant_name) in union.variants.iter().zip(&names) {
        let ty: String = rust_type(graph, plan, home, variant.ty, imports);
        let _ = writeln!(out, "    {variant_name}({ty}),");
    }
    out.push_str("}\n\n");
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

/// Renders one output buffer.
fn render_buffer(graph: &TypeGraph, plan: &Plan, home: Home) -> CodeGenResult<Vec<u8>> {
    let mut imports: Imports = Imports::default();
    let mut body: String = String::new();
    let items: Vec<TypeId> = ordered_items(graph, plan, home);
    for &id in &items {
        let name: &str = plan.name(id);
        debug_assert_eq!(plan.home(id), home);
        let item: String = match graph.node(id) {
            TypeNode::Object(o) => render_struct(graph, plan, home, id, name, o, &mut imports),
            TypeNode::Enum(e) => render_enum(name, e),
            TypeNode::Union(u) => render_union(graph, plan, home, id, name, u, &mut imports),
            _ => unreachable!("only named nodes are items"),
        };
        body.push_str(&item);
    }
    if let Home::Schema(s) = home {
        let roots = &graph.schemas[s];
        if let Some(ref alias) = plan.root_aliases[s] {
            let ty: String = rust_type(graph, plan, home, roots.root, &mut imports);
            let _ = writeln!(body, "pub type {alias} = {ty};\n");
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
    writeln!(
        out,
        "//! Generated by json-schema-rs. Do not edit manually."
    )?;
    writeln!(out)?;
    // Only items (and the `deserialize_some` helper) use serde: a buffer holding just a type
    // alias or re-exports of shared items needs no import.
    let serde_import: bool = !items.is_empty() || imports.deserialize_some;
    if serde_import {
        writeln!(out, "use serde::{{Deserialize, Serialize}};")?;
    }
    for path in &imports.uses {
        writeln!(out, "use {path};")?;
    }
    if serde_import || !imports.uses.is_empty() {
        writeln!(out)?;
    }
    if !imports.shared.is_empty() {
        for name in &imports.shared {
            writeln!(out, "pub use super::shared::{name};")?;
        }
        writeln!(out)?;
    }
    if imports.deserialize_some {
        out.write_all(DESERIALIZE_SOME.as_bytes())?;
    }
    if !imports.unique_shapes.is_empty() {
        out.write_all(unique_items_module(&imports).as_bytes())?;
    }
    out.write_all(body.as_bytes())?;
    Ok(out)
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

/// The `unique_items` helper module for a buffer: the `UniqueSet` impls and the deserialize
/// functions its `uniqueItems` set fields use. A duplicate item is a deserialize error, so an
/// invalid instance is never silently turned into a different, valid one.
fn unique_items_module(imports: &Imports) -> String {
    let mut out: String = String::from(UNIQUE_ITEMS_HEAD);
    for kind in &imports.unique_sets {
        out.push_str(kind.helper_impl());
    }
    out.push_str(UNIQUE_ITEMS_VISITOR);
    for shape in &imports.unique_shapes {
        out.push_str(match shape {
            FieldShape::Plain => UNIQUE_ITEMS_REQUIRED,
            FieldShape::Optional => UNIQUE_ITEMS_OPTIONAL,
            FieldShape::RequiredNullable => UNIQUE_ITEMS_NULLABLE,
            FieldShape::OptionalNullable => UNIQUE_ITEMS_OPTIONAL_NULLABLE,
        });
    }
    out.push_str("}\n\n");
    out
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
    ) -> Result<Option<Option<S>>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Option::<Unique<S>>::deserialize(deserializer)
            .map(|value: Option<Unique<S>>| Some(value.map(|unique: Unique<S>| unique.0)))
    }
";

#[cfg(test)]
mod tests {
    use super::CodeGenError;
    use super::{CodeGenBackend, CodeGenInput, RustBackend, generate_rust};
    use crate::code_gen::settings::{
        CodeGenSettings, DedupeMode, ModelNameSource, UniqueItemsType, UnknownProperties,
    };
    use crate::json_schema::{JsonSchema, JsonType, RegistryError, Schema};

    fn default_settings() -> CodeGenSettings {
        CodeGenSettings::builder().build()
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Bee")]
pub struct Bee {
    pub x: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
            ),
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use super::shared::Bee;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use super::shared::Thing;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Parent")]
pub struct Parent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Child>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Bee")]
pub struct Bee {
    pub x: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
    fn optional_property_type_string_or_null_emits_option_option_string() {
        let json = r#"{"type":"object","properties":{"name":{"type":["string","null"]}}}"#;
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The order of a type array does not matter: `["null", "string"]` reads as `["string", "null"]`.
    #[test]
    fn optional_property_type_null_or_string_emits_option_option_string() {
        let json = r#"{"type":"object","properties":{"name":{"type":["null","string"]}}}"#;
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Id {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    pub city: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
pub enum Status {
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum FooAnyOf {
    String(String),
    Variant1(FooVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum FooOneOf {
    String(String),
    Variant1(FooVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
    Variant0(RootVariant0),
    Variant1(RootVariant1),
}

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
pub enum Level {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
pub enum A {
    #[serde(rename = "x")]
    X,
    #[serde(rename = "y")]
    Y,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub t: T,
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        Err(serde::de::Error::custom(format!("{value} is not a Tag value")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub count: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub byte: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_exclusive_bounds_emits_u8() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","exclusiveMinimum":-1,"exclusiveMaximum":256}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_fractional_bounds_rounds_inward() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":-0.5,"maximum":255.9}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_u64_range_emits_u64() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":0,"maximum":18446744073709551615}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: u64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_with_range_beyond_64_bits_emits_i64_fallback() {
        let json = r#"{"type":"object","properties":{"n":{"type":"integer","minimum":-1,"maximum":18446744073709551615}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_exclusive_bounds_in_f32_range_emits_f32() {
        let json = r#"{"type":"object","properties":{"n":{"type":"number","exclusiveMinimum":0,"exclusiveMaximum":1}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: f32,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_bound_beyond_f32_emits_f64() {
        let json = r#"{"type":"object","properties":{"n":{"type":"number","minimum":0,"maximum":1e39}},"required":["n"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_with_minimum_maximum_f32_range_emits_f32() {
        let json = r#"{"type":"object","properties":{"value":{"type":"number","minimum":0.5,"maximum":100.5}},"required":["value"]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub value: f32,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unique_items(UniqueItemsType::HashSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
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

    pub(super) fn optional<'de, D, S>(deserializer: D) -> Result<Option<S>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Unique::deserialize(deserializer).map(|unique: Unique<S>| Some(unique.0))
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
    ) -> Result<Option<Option<S>>, D::Error>
    where
        D: serde::Deserializer<'de>,
        S: UniqueSet,
        S::Item: Deserialize<'de>,
    {
        Option::<Unique<S>>::deserialize(deserializer)
            .map(|value: Option<Unique<S>>| Some(value.map(|unique: Unique<S>| unique.0)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub floats: Option<Vec<f64>>,
    #[serde(default, deserialize_with = "unique_items::optional", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub ids: Option<HashSet<i64>>,
    #[serde(default, deserialize_with = "unique_items::optional_nullable", skip_serializing_if = "Option::is_none")]
    #[json_schema(unique_items)]
    pub maybe: Option<Option<HashSet<bool>>>,
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
            .unique_items(UniqueItemsType::BTreeSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unique_items(UniqueItemsType::IndexSet)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Items {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(unique_items)]
    pub items: Vec<Items>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub counts: Vec<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Items {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub items: Vec<Items>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street_address: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "foo-bar", skip_serializing_if = "Option::is_none")]
    pub foo_bar: Option<String>,
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
            .model_name_source(ModelNameSource::PropertyKeyFirst)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "FooBar")]
pub struct Address {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema.clone(), schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Addr1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A root type
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Line one
/// Line two
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Inner type
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Nested {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
pub enum Status {
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// First
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// First
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Second
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
    fn examples_golden_with_examples_emits_doc() {
        let json: &str = r#"{"type":"object","properties":{"x":{"type":"string"}},"required":["x"],"examples":[{"x":"foo"}]}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::default();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// {"x":"foo"}
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let settings: CodeGenSettings = CodeGenSettings::default();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// "a"
/// 1
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let settings: CodeGenSettings = CodeGenSettings::default();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// "open"
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub enum Status {
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let settings: CodeGenSettings = CodeGenSettings::default();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// 1
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#
                .to_string(),
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// 2
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

///
/// # Examples
///
/// 1
#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            None,
            vec![
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Functional)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[s1, s2], &settings).unwrap();
        let actual: (Option<String>, Vec<String>) = buffers(output);
        let expected: (Option<String>, Vec<String>) = (
            Some(
                r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Full)
            .build();
        let settings_func: CodeGenSettings = CodeGenSettings::builder()
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
            .flatten_single_entry_combinators(true)
            .build();
        let expected: bool = true;
        let actual: bool = settings.flatten_single_entry_combinators;
        assert_eq!(expected, actual);
    }

    #[test]
    fn builder_sets_unique_items() {
        let settings: CodeGenSettings = CodeGenSettings::builder()
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
}

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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
}

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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum AAnyOf {
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
pub enum S {
    #[serde(rename = "b")]
    B,
    #[serde(rename = "c")]
    C,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<N>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
    pub value: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct A {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<Box<B>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Node")]
pub struct Node {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<Node>>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct P {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    pub extra: i64,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<P>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Item")]
pub struct Item1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
    fn reserved_struct_name_gets_numeric_suffix() {
        let json: &str = r#"{"type":"object","properties":{"s":{"title":"String","type":"object","properties":{"a":{"type":"string"}}}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = default_settings();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "String")]
pub struct String0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<String0>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(rename = "fooBar", skip_serializing_if = "Option::is_none")]
    pub foo_bar0: Option<String>,
    #[serde(rename = "foo_bar", skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
    Dog(Dog),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum PetOneOf {
    Cat(Cat),
    Dog(Dog),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum RootAnyOf {
    Variant0(RootVariant0),
}

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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum RootOneOf {
    Variant0(RootVariant0),
}

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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum PetOneOf {
    Cat(Cat),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Cat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meow: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Dog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bark: Option<bool>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum PetAnyOf {
    Cat(Cat),
    Dog(Dog),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pet: Option<PetAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// The flatten setting leaves a two-entry `anyOf` of a type and `null` as a nullable `Option`.
    #[test]
    fn flatten_enabled_nullable_any_of_stays_option() {
        let json: &str = r#"{"type":"object","properties":{"name":{"anyOf":[{"type":"string"},{"type":"null"}]}}}"#;
        let schema: Schema = serde_json::from_str(json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
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
            .flatten_single_entry_combinators(true)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Named")]
pub struct Named {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

    /// An optional `oneOf` of `null` and a type is an optional nullable field: `Option<Option<T>>`.
    #[test]
    fn optional_one_of_null_and_type_emits_optional_nullable_field() {
        let json: &str = r#"{"type":"object","properties":{"name":{"oneOf":[{"type":"null"},{"type":"string"}]}}}"#;
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum V {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        Err(serde::de::Error::custom(format!("{value} is not a Level value")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        Err(serde::de::Error::custom(format!("{value} is not a Version value")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub id: String,
    #[serde(flatten)]
    pub additional: BTreeMap<String, i64>,
}

";
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    /// `then` / `else` properties become optional fields; a key in both is the `anyOf` of the two.
    /// The `if` schema itself adds no field.
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum SharedAnyOf {
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<i64>,
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .dedupe_mode(DedupeMode::Disabled)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Point")]
pub struct Point0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "Point")]
pub struct Point1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unknown_properties(UnknownProperties::Capture)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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
        let expected: &str = r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    pub n: u8,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

";
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct RootValue {
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
pub enum Shape {
    Variant0(ShapeVariant0),
    Variant1(ShapeVariant1),
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant0 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct ShapeVariant1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[serde(untagged)]
#[json_schema(one_of)]
pub enum ShapeOneOf {
    Variant0(ShapeVariant0),
    Variant1(ShapeVariant1),
}

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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct B {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<Box<Root>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 300)]
    pub b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[json_schema(default = "x")]
    pub a: String,
    #[serde(deserialize_with = "Option::deserialize")]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = -3)]
    pub m: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    #[json_schema(default = "z")]
    pub n: Option<Option<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn m_or_default(&self) -> Option<i64> {
        self.m.unwrap_or(Some(-3))
    }

    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn n_or_default(&self) -> Option<&str> {
        match &self.n {
            Some(value) => value.as_deref(),
            None => Some("z"),
        }
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct O {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o: Option<O>,
    #[serde(default, deserialize_with = "deserialize_some", skip_serializing_if = "Option::is_none")]
    pub z: Option<Option<String>>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn float_default_outside_f32_gets_no_accessor() {
        let schema: Schema = Schema::try_from(r#"{"type": "object", "properties": {"f": {"type": "number", "minimum": 0, "maximum": 1, "default": 1e300}, "g": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.5}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Root {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 1e+300)]
    pub f: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[json_schema(default = 0.5)]
    pub g: Option<f32>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl Root {
    /// The field's value, or its schema `default` when the key is absent.
    #[must_use]
    pub fn g_or_default(&self) -> f32 {
        self.g.unwrap_or(0.5)
    }
}

"#;
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_accessors_are_methods_of_their_own_struct() {
        let schema: Schema = Schema::try_from(r#"{"$defs":{"FooBar":{"type":"object","properties":{"baz":{"type":"integer","default":1}}},"Foo":{"type":"object","properties":{"bar_baz":{"type":"integer","default":2}}}}}"#).unwrap();
        let output: super::GenerateRustOutput =
            generate_rust(&[schema], &default_settings()).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: &str = r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct Foo {
    #[serde(skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
pub struct FooBar {
    #[serde(skip_serializing_if = "Option::is_none")]
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
        Err(serde::de::Error::custom(format!("{value} is not a E value")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
#[json_schema(title = "!!")]
pub struct Z {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]
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

    /// The start of the `unique_items` helper module, before the set impls.
    const UNIQUE_ITEMS_TEST_HEAD: &str = "/// Deserializes `uniqueItems` arrays into sets: a duplicate item is an error, never dropped.
mod unique_items {
    use serde::Deserialize;

    /// A set that reports whether an inserted item was new.
    pub(super) trait UniqueSet: Default {
        type Item;
        fn with_capacity(capacity: usize) -> Self;
        fn insert_new(&mut self, item: Self::Item) -> bool;
    }
";

    const UNIQUE_ITEMS_TEST_HASH_SET: &str = "
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

    const UNIQUE_ITEMS_TEST_BTREE_SET: &str = "
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

    #[cfg(feature = "indexmap")]
    const UNIQUE_ITEMS_TEST_INDEX_SET: &str = "
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

    /// The visitor and the `required` function, closing the module.
    const UNIQUE_ITEMS_TEST_REQUIRED_TAIL: &str = r#"
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

"#;

    /// The per-schema buffer for one schema generated with `setting`.
    fn generate_with_unique_items(schema_json: &str, setting: UniqueItemsType) -> String {
        let schema: Schema = Schema::try_from(schema_json).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder().unique_items(setting).build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        String::from_utf8(output.per_schema[0].clone()).unwrap()
    }

    /// A closed `Point` struct (two integers and an optional `Tag` whose unknown keys are
    /// typed booleans) as the item of a required `uniqueItems` array.
    const POINTS_SCHEMA: &str = r##"{"type":"object","properties":{"points":{"type":"array","items":{"$ref":"#/$defs/Point"},"uniqueItems":true}},"required":["points"],"$defs":{"Point":{"type":"object","properties":{"x":{"type":"integer"},"y":{"type":"integer"},"tag":{"$ref":"#/$defs/Tag"}},"required":["x","y"],"additionalProperties":false},"Tag":{"type":"object","properties":{"name":{"type":"string"}},"additionalProperties":{"type":"boolean"}}}}"##;

    /// The `Point` and `Tag` items of [`POINTS_SCHEMA`] with `derive`.
    fn points_items(derive: &str) -> String {
        format!(
            r#"{derive}
pub struct Tag {{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, bool>,
}}

{derive}
#[serde(deny_unknown_fields)]
pub struct Point {{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Tag>,
    pub x: i64,
    pub y: i64,
}}

"#
        )
    }

    const ORD_DERIVE: &str = "#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]";
    const PLAIN_DERIVE: &str =
        "#[derive(Debug, Clone, Serialize, Deserialize, json_schema_rs_macro::ToJsonSchema)]";

    #[test]
    fn unique_items_hash_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{items}{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: HashSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#,
            items = points_items(ORD_DERIVE)
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_btree_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::BTreeSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_BTREE_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{items}{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: BTreeSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#,
            items = points_items(ORD_DERIVE)
        );
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "indexmap")]
    #[test]
    fn unique_items_index_set_closed_struct_items_derive_hash_eq_ord() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::IndexSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use indexmap::IndexSet;
use std::collections::BTreeMap;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_INDEX_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{items}{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub points: IndexSet<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#,
            items = points_items(ORD_DERIVE)
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_vec_closed_struct_items_get_no_extra_derives() {
        let actual: String = generate_with_unique_items(POINTS_SCHEMA, UniqueItemsType::Vec);
        let expected: String = format!(
            r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;

{items}{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub points: Vec<Point>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

",
            items = points_items(PLAIN_DERIVE)
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_hash_set_string_enum_items_emit_set() {
        let schema_json: &str = r#"{"type":"object","properties":{"colors":{"type":"array","items":{"enum":["red","green"]},"uniqueItems":true}},"required":["colors"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{ORD_DERIVE}
pub enum Colors {{
    #[serde(rename = "green")]
    Green,
    #[serde(rename = "red")]
    Red,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub colors: HashSet<Colors>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_hash_set_union_items_derive_hash_eq_ord() {
        let schema_json: &str = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"oneOf":[{"type":"string"},{"type":"integer"}]},"uniqueItems":true}},"required":["ids"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{ORD_DERIVE}
#[serde(untagged)]
#[json_schema(one_of)]
pub enum IdsOneOf {{
    String(String),
    Integer(i64),
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub ids: HashSet<IdsOneOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// `1` and `1.0` are equal JSON, but `i64` rejects `1.0` while the enum accepts both, so
    /// they would deserialize to different variants: the set would keep a duplicate.
    #[test]
    fn unique_items_set_union_of_integer_and_whole_number_enum_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"codes":{"type":"array","items":{"anyOf":[{"type":"integer"},{"enum":[1,"a"]}]},"uniqueItems":true}},"required":["codes"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum CodesVariant1 {{
    #[json_schema(value = "a")]
    A,
    #[json_schema(value = 1)]
    N1,
}}

impl Serialize for CodesVariant1 {{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{
        let value: serde_json::Value = match self {{
            Self::A => serde_json::json!("a"),
            Self::N1 => serde_json::json!(1),
        }};
        value.serialize(serializer)
    }}
}}

impl<'de> Deserialize<'de> for CodesVariant1 {{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("a")) {{
            return Ok(Self::A);
        }}
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {{
            return Ok(Self::N1);
        }}
        Err(serde::de::Error::custom(format!("{{value}} is not a CodesVariant1 value")))
    }}
}}

{PLAIN_DERIVE}
#[serde(untagged)]
pub enum CodesAnyOf {{
    Integer(i64),
    Variant1(CodesVariant1),
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub codes: Vec<CodesAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// An enum of fractional numbers never matches a whole number, so `1` vs `1.0` cannot split
    /// across the variants.
    #[test]
    fn unique_items_hash_set_union_of_integer_and_fractional_enum_emits_set() {
        let schema_json: &str = r#"{"type":"object","properties":{"codes":{"type":"array","items":{"anyOf":[{"type":"integer"},{"enum":[1.5,"a"]}]},"uniqueItems":true}},"required":["codes"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum CodesVariant1 {{
    #[json_schema(value = "a")]
    A,
    #[json_schema(value = 1.5)]
    N1Point5,
}}

impl Serialize for CodesVariant1 {{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{
        let value: serde_json::Value = match self {{
            Self::A => serde_json::json!("a"),
            Self::N1Point5 => serde_json::json!(1.5),
        }};
        value.serialize(serializer)
    }}
}}

impl<'de> Deserialize<'de> for CodesVariant1 {{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!("a")) {{
            return Ok(Self::A);
        }}
        if json_schema_rs::json_equal(&value, &serde_json::json!(1.5)) {{
            return Ok(Self::N1Point5);
        }}
        Err(serde::de::Error::custom(format!("{{value}} is not a CodesVariant1 value")))
    }}
}}

{ORD_DERIVE}
#[serde(untagged)]
pub enum CodesAnyOf {{
    Integer(i64),
    Variant1(CodesVariant1),
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub codes: HashSet<CodesAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// One branch holding both an integer and a whole-number enum is fine: each position has
    /// one type, so `1` and `1.0` take the same branch (or `1.0` fails).
    #[test]
    fn unique_items_hash_set_union_with_integer_and_whole_number_enum_in_one_branch_emits_set() {
        let schema_json: &str = r##"{"type":"object","properties":{"pairs":{"type":"array","items":{"anyOf":[{"$ref":"#/$defs/Pair"},{"type":"string"}]},"uniqueItems":true}},"required":["pairs"],"$defs":{"Pair":{"type":"object","properties":{"n":{"type":"integer"},"k":{"const":1}},"required":["n","k"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, json_schema_rs_macro::ToJsonSchema)]
pub enum K {{
    #[json_schema(value = 1)]
    N1,
}}

impl Serialize for K {{
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {{
        let value: serde_json::Value = match self {{
            Self::N1 => serde_json::json!(1),
        }};
        value.serialize(serializer)
    }}
}}

impl<'de> Deserialize<'de> for K {{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {{
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;
        if json_schema_rs::json_equal(&value, &serde_json::json!(1)) {{
            return Ok(Self::N1);
        }}
        Err(serde::de::Error::custom(format!("{{value}} is not a K value")))
    }}
}}

{ORD_DERIVE}
#[serde(deny_unknown_fields)]
pub struct Pair {{
    pub k: K,
    pub n: i64,
}}

{ORD_DERIVE}
#[serde(untagged)]
pub enum PairsAnyOf {{
    Pair(Pair),
    String(String),
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub pairs: HashSet<PairsAnyOf>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unique_items_set_struct_item_with_float_field_falls_back_to_vec() {
        let schema_json: &str = r##"{"type":"object","properties":{"readings":{"type":"array","items":{"$ref":"#/$defs/Reading"},"uniqueItems":true}},"required":["readings"],"$defs":{"Reading":{"type":"object","properties":{"value":{"type":"number"}},"required":["value"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;

{PLAIN_DERIVE}
#[serde(deny_unknown_fields)]
pub struct Reading {{
    pub value: f64,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub readings: Vec<Reading>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"
        );
        assert_eq!(expected, actual);
    }

    /// Unknown keys kept as `serde_json::Value` (no `Hash`): the item stays in a `Vec`.
    #[test]
    fn unique_items_set_struct_item_capturing_untyped_unknown_keys_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"loose":{"type":"array","items":{"type":"object","properties":{"a":{"type":"string"}}},"uniqueItems":true}},"required":["loose"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;

{PLAIN_DERIVE}
pub struct Loose {{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub loose: Vec<Loose>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// Dropped unknown keys would make `{"a": "x", "b": 1}` and `{"a": "x"}` equal items.
    #[test]
    fn unique_items_set_struct_item_dropping_unknown_keys_falls_back_to_vec() {
        let schema: Schema = Schema::try_from(r#"{"type":"object","properties":{"loose":{"type":"array","items":{"type":"object","properties":{"a":{"type":"string"}}},"uniqueItems":true}},"required":["loose"]}"#).unwrap();
        let settings: CodeGenSettings = CodeGenSettings::builder()
            .unique_items(UniqueItemsType::HashSet)
            .unknown_properties(UnknownProperties::Drop)
            .build();
        let output: super::GenerateRustOutput = generate_rust(&[schema], &settings).unwrap();
        let actual: String = String::from_utf8(output.per_schema[0].clone()).unwrap();
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};

{PLAIN_DERIVE}
pub struct Loose {{
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub loose: Vec<Loose>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// A set field ignores order, a JSON array does not: the outer item stays in a `Vec`, the
    /// inner field is still a set.
    #[test]
    fn unique_items_set_struct_item_with_set_field_falls_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"groups":{"type":"array","items":{"type":"object","properties":{"tags":{"type":"array","items":{"type":"string"},"uniqueItems":true}},"required":["tags"],"additionalProperties":false},"uniqueItems":true}},"required":["groups"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{PLAIN_DERIVE}
#[serde(deny_unknown_fields)]
pub struct Groups {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub tags: HashSet<String>,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub groups: Vec<Groups>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// A recursive item type is a set item when every type it reaches is.
    #[test]
    fn unique_items_hash_set_recursive_struct_items_emit_set() {
        let schema_json: &str = r##"{"type":"object","properties":{"nodes":{"type":"array","items":{"$ref":"#/$defs/Node"},"uniqueItems":true}},"required":["nodes"],"$defs":{"Node":{"type":"object","properties":{"name":{"type":"string"},"children":{"type":"array","items":{"$ref":"#/$defs/Node"}}},"required":["name","children"],"additionalProperties":false}}}"##;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r#"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use std::collections::HashSet;

{UNIQUE_ITEMS_TEST_HEAD}{UNIQUE_ITEMS_TEST_HASH_SET}{UNIQUE_ITEMS_TEST_REQUIRED_TAIL}{ORD_DERIVE}
#[serde(deny_unknown_fields)]
pub struct Node {{
    pub children: Vec<Node>,
    pub name: String,
}}

{PLAIN_DERIVE}
pub struct Root {{
    #[serde(deserialize_with = "unique_items::required")]
    #[json_schema(unique_items)]
    pub nodes: HashSet<Node>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"#
        );
        assert_eq!(expected, actual);
    }

    /// `Uuid` parsing folds case and accepts several forms, so two different JSON strings can be
    /// one `Uuid`: the item stays in a `Vec`.
    #[cfg(feature = "uuid")]
    #[test]
    fn unique_items_set_uuid_items_fall_back_to_vec() {
        let schema_json: &str = r#"{"type":"object","properties":{"ids":{"type":"array","items":{"type":"string","format":"uuid"},"uniqueItems":true}},"required":["ids"]}"#;
        let actual: String = generate_with_unique_items(schema_json, UniqueItemsType::HashSet);
        let expected: String = format!(
            r"//! Generated by json-schema-rs. Do not edit manually.

use serde::{{Deserialize, Serialize}};
use std::collections::BTreeMap;
use uuid::Uuid;

{PLAIN_DERIVE}
pub struct Root {{
    #[json_schema(unique_items)]
    pub ids: Vec<Uuid>,
    #[serde(flatten)]
    pub additional: BTreeMap<String, serde_json::Value>,
}}

"
        );
        assert_eq!(expected, actual);
    }
}
