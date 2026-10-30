//! Passes over a built [`TypeGraph`], run in this order by the backend:
//!
//! 0. [`collapse_wrapper_cycles`]: an `Option` / `Box` cycle (no type information) becomes any
//!    value.
//! 1. [`order_variants`]: integers before numbers in every union (serde tries untagged variants
//!    in order, and a number variant would capture integers).
//! 2. [`dedup`]: structurally identical nodes become one (partition refinement, so it is exact
//!    on recursive types too).
//! 3. [`break_cycles`]: a `Box` on each back-edge that would make a type contain itself by value.
//! 4. [`reachable`]: which nodes each schema uses, for placement in shared or per-schema output.
//!
//! Every pass is iterative (explicit stacks and work lists).

use super::{Annotations, TypeGraph, TypeId, TypeNode, scalar_json_type};
use crate::code_gen::settings::DedupeMode;
use crate::json_schema::JsonType;
use std::collections::{BTreeSet, HashMap};

/// Puts integer variants before number variants in every union, keeping the rest in order.
pub(crate) fn order_variants(graph: &mut TypeGraph) {
    for index in 0..graph.nodes.len() {
        let TypeNode::Union(ref union) = graph.nodes[index] else {
            continue;
        };
        let kinds: Vec<Option<JsonType>> = union
            .variants
            .iter()
            .map(|v| scalar_json_type(graph.node(graph.peel(v.ty))))
            .collect();
        let Some(first_number) = kinds.iter().position(|k| *k == Some(JsonType::Number)) else {
            continue;
        };
        let TypeNode::Union(ref mut union) = graph.nodes[index] else {
            continue;
        };
        let mut variants = std::mem::take(&mut union.variants).into_iter().zip(kinds);
        let mut ordered = Vec::new();
        let mut integers = Vec::new();
        let mut rest = Vec::new();
        for (i, (variant, kind)) in variants.by_ref().enumerate() {
            if i < first_number {
                ordered.push(variant);
            } else if kind == Some(JsonType::Integer) {
                integers.push(variant);
            } else {
                rest.push(variant);
            }
        }
        ordered.extend(integers);
        ordered.extend(rest);
        union.variants = ordered;
    }
}

/// Replaces every cycle made only of `Option` / `Box` nodes (a value that is optional of
/// itself, e.g. `{"anyOf": [{"$ref": "#"}, {"type": "null"}]}`) with [`TypeNode::Any`]: it holds
/// no type information, like a pure `$ref` loop. Runs first, so later passes can peel wrappers
/// without a visited set.
pub(crate) fn collapse_wrapper_cycles(graph: &mut TypeGraph) {
    // 0: unvisited, 1: on the current chain, 2: done.
    let mut state: Vec<u8> = vec![0; graph.nodes.len()];
    for start in 0..graph.nodes.len() {
        if state[start] != 0 {
            continue;
        }
        let mut chain: Vec<usize> = Vec::new();
        let mut current: usize = start;
        loop {
            match state[current] {
                0 => {
                    state[current] = 1;
                    chain.push(current);
                    match graph.nodes[current] {
                        TypeNode::Option(inner) | TypeNode::Box(inner) => current = inner.index(),
                        _ => break,
                    }
                }
                1 => {
                    // `current` is on this chain: everything from it on is a wrapper cycle.
                    let from: usize = chain.iter().position(|&i| i == current).unwrap_or(0);
                    for &i in &chain[from..] {
                        graph.nodes[i] = TypeNode::Any;
                    }
                    break;
                }
                _ => break,
            }
        }
        for i in chain {
            state[i] = 2;
        }
    }
}

/// The parts of a node's identity that are not its children, as text (deterministic and
/// cheap to hash). `mode` decides whether documentation takes part.
fn local_signature(node: &TypeNode, mode: DedupeMode) -> String {
    let annotations = |a: &Annotations| -> String {
        match mode {
            DedupeMode::Full => format!(
                "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
                a.title,
                a.description,
                a.comment,
                a.examples.as_ref().map(|e| serde_json::to_string(e).ok()),
                a.deprecated,
                a.id
            ),
            DedupeMode::Functional | DedupeMode::Disabled => format!("{:?}", a.title),
        }
    };
    match node {
        TypeNode::Object(o) => {
            let fields: Vec<String> = o
                .fields
                .iter()
                .map(|f| {
                    let docs: String = match mode {
                        DedupeMode::Full => format!("{:?}|{:?}", f.description, f.deprecated),
                        DedupeMode::Functional | DedupeMode::Disabled => String::new(),
                    };
                    format!(
                        "{:?}|{}|{:?}|{docs}",
                        f.json_key,
                        f.required,
                        f.default.as_ref().map(ToString::to_string)
                    )
                })
                .collect();
            format!(
                "object|{fields:?}|{}|{}|{}",
                o.deny_unknown,
                o.additional.is_some(),
                annotations(&o.annotations)
            )
        }
        TypeNode::Enum(e) => {
            let values: Vec<String> = e.values.iter().map(ToString::to_string).collect();
            format!("enum|{values:?}|{}", annotations(&e.annotations))
        }
        TypeNode::Union(u) => {
            let titles: Vec<&Option<String>> = u.variants.iter().map(|v| &v.title).collect();
            format!(
                "union|{:?}|{titles:?}|{}",
                u.kind,
                annotations(&u.annotations)
            )
        }
        TypeNode::Pending
        | TypeNode::Any
        | TypeNode::Null
        | TypeNode::Boolean
        | TypeNode::Integer(_)
        | TypeNode::Number(_)
        | TypeNode::String { .. }
        | TypeNode::Array { .. }
        | TypeNode::Map { .. }
        | TypeNode::Option(_)
        | TypeNode::Box(_) => {
            // Children are compared by class; drop them from the local text.
            let mut copy: TypeNode = node.clone();
            for child in copy.children_mut() {
                *child = TypeId(0);
            }
            format!("{copy:?}")
        }
    }
}

/// Merges structurally identical nodes. Nodes start in classes by [`local_signature`]; classes
/// are split by their children's classes until stable (partition refinement), so two recursive
/// types are equal exactly when their unfoldings are. Every id in the graph is then rewritten to
/// its class's lowest id, and name proposals are pooled there. With [`DedupeMode::Disabled`]
/// nothing is merged.
pub(crate) fn dedup(graph: &mut TypeGraph, mode: DedupeMode) {
    if mode == DedupeMode::Disabled {
        return;
    }
    let mut class: Vec<u32> = {
        let mut ids: HashMap<String, u32> = HashMap::new();
        graph
            .nodes
            .iter()
            .map(|node| {
                let next: u32 = u32::try_from(ids.len()).expect("fewer than 2^32 classes");
                *ids.entry(local_signature(node, mode)).or_insert(next)
            })
            .collect()
    };
    let mut count: usize = class.iter().collect::<BTreeSet<_>>().len();
    loop {
        let mut ids: HashMap<(u32, Vec<u32>), u32> = HashMap::new();
        let next_class: Vec<u32> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let key: (u32, Vec<u32>) = (
                    class[i],
                    node.children().iter().map(|c| class[c.index()]).collect(),
                );
                let next: u32 = u32::try_from(ids.len()).expect("fewer than 2^32 classes");
                *ids.entry(key).or_insert(next)
            })
            .collect();
        let next_count: usize = ids.len();
        class = next_class;
        if next_count == count {
            break;
        }
        count = next_count;
    }
    // Representative: the lowest id of each class.
    let mut representative: HashMap<u32, TypeId> = HashMap::new();
    for (i, c) in class.iter().enumerate() {
        representative
            .entry(*c)
            .or_insert(TypeId(u32::try_from(i).expect("fewer than 2^32 nodes")));
    }
    let canon: Vec<TypeId> = class.iter().map(|c| representative[c]).collect();
    for (i, &target) in canon.iter().enumerate() {
        if target.index() != i {
            let moved = std::mem::take(&mut graph.proposals[i]);
            graph.proposals[target.index()].extend(moved);
        }
    }
    for node in &mut graph.nodes {
        for child in node.children_mut() {
            *child = canon[child.index()];
        }
    }
    for roots in &mut graph.schemas {
        roots.root = canon[roots.root.index()];
        for (_, id) in &mut roots.definitions {
            *id = canon[id.index()];
        }
    }
}

/// A by-value slot: (index in [`TypeNode::children`], the named node it holds).
type Slot = (usize, TypeId);

/// The ids a node holds by value (not behind `Vec`, map or `Box`), with `Option`s peeled: each
/// entry is (slot index in [`TypeNode::children`], target named node).
fn inline_slots(graph: &TypeGraph, node: &TypeNode) -> Vec<Slot> {
    let slots: Vec<TypeId> = match node {
        TypeNode::Object(o) => o.fields.iter().map(|f| f.ty).collect(),
        TypeNode::Union(u) => u.variants.iter().map(|v| v.ty).collect(),
        _ => Vec::new(),
    };
    slots
        .into_iter()
        .enumerate()
        .filter_map(|(slot, id)| {
            let id: TypeId = graph.peel(id);
            graph.node(id).is_named().then_some((slot, id))
        })
        .collect()
}

/// `id` with the named node at the bottom of its `Option` chain wrapped in `Box` (iterative:
/// the chain is rebuilt from the bottom up).
fn boxed(graph: &mut TypeGraph, id: TypeId) -> TypeId {
    let mut options: usize = 0;
    let mut bottom: TypeId = id;
    while let TypeNode::Option(inner) = graph.node(bottom) {
        bottom = *inner;
        options += 1;
    }
    let mut out: TypeId = graph.push(TypeNode::Box(bottom));
    for _ in 0..options {
        out = graph.push(TypeNode::Option(out));
    }
    out
}

/// Places a `Box` on every back-edge of a depth-first walk over by-value containment (struct
/// fields and union variants, through `Option`), so no type contains itself by value. `Vec`,
/// maps and `Box` already give indirection and are not followed. The walk starts at each
/// schema's root, then its definitions, and visits slots in order, so the boxed edges are the
/// same on every run.
pub(crate) fn break_cycles(graph: &mut TypeGraph) {
    let starts: Vec<TypeId> = graph
        .schemas
        .iter()
        .flat_map(|s| std::iter::once(s.root).chain(s.definitions.iter().map(|(_, id)| *id)))
        .collect();
    let mut visited: BTreeSet<TypeId> = BTreeSet::new();
    let mut active: BTreeSet<TypeId> = BTreeSet::new();
    // Frames: (node, its inline slots as found on entry, next one to examine). The slots are
    // fixed per frame: boxing a slot must not shift the ones after it.
    let mut stack: Vec<(TypeId, Vec<Slot>, usize)> = Vec::new();
    // Every node reachable by any edge is a potential start too (types behind `Vec` / maps).
    let mut pending: Vec<TypeId> = starts.into_iter().rev().collect();
    while let Some(start) = pending.pop() {
        if visited.contains(&start) {
            continue;
        }
        stack.push((start, inline_slots(graph, graph.node(start)), 0));
        visited.insert(start);
        active.insert(start);
        while let Some((node, slots, next)) = stack.last_mut() {
            let node: TypeId = *node;
            if *next >= slots.len() {
                // Done with this node: its non-inline children start their own walks.
                for child in graph.node(node).children().into_iter().rev() {
                    let target: TypeId = graph.peel(child);
                    if !visited.contains(&target) {
                        pending.push(target);
                    }
                }
                active.remove(&node);
                stack.pop();
                continue;
            }
            let (slot, target) = slots[*next];
            *next += 1;
            if active.contains(&target) {
                let original: TypeId = graph.node(node).children()[slot];
                let replacement: TypeId = boxed(graph, original);
                let mut node_value: TypeNode = graph.nodes[node.index()].clone();
                *node_value.children_mut()[slot] = replacement;
                graph.nodes[node.index()] = node_value;
            } else if visited.insert(target) {
                active.insert(target);
                let target_slots: Vec<Slot> = inline_slots(graph, graph.node(target));
                stack.push((target, target_slots, 0));
            }
        }
    }
}

/// Every node reachable from each schema's root and definitions, per schema.
pub(crate) fn reachable(graph: &TypeGraph) -> Vec<BTreeSet<TypeId>> {
    graph
        .schemas
        .iter()
        .map(|roots| {
            let mut seen: BTreeSet<TypeId> = BTreeSet::new();
            let mut stack: Vec<TypeId> = std::iter::once(roots.root)
                .chain(roots.definitions.iter().map(|(_, id)| *id))
                .collect();
            while let Some(id) = stack.pop() {
                if seen.insert(id) {
                    stack.extend(graph.node(id).children());
                }
            }
            seen
        })
        .collect()
}
