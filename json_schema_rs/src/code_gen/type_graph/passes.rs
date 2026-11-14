//! Passes over a built [`TypeGraph`], run in this order by the backend:
//!
//! 0. [`collapse_wrapper_cycles`]: an `Option` / `Box` cycle (no type information) becomes any
//!    value.
//! 1. [`order_variants`]: integers before numbers in every union (serde tries untagged variants
//!    in order, and a number variant would capture integers).
//! 2. [`dedup`]: structurally identical nodes become one (partition refinement, so it is exact
//!    on recursive types too).
//! 3. [`break_cycles`]: a `Box` on each back-edge that would make a type contain itself by value.
//! 4. [`box_large_values`]: a `Box` on each field or union variant holding a large struct or
//!    union by value.
//! 5. [`reachable`]: which nodes each schema uses, for placement in shared or per-schema output.
//!
//! Every pass is iterative (explicit stacks and work lists).

use super::{
    Annotations, TypeGraph, TypeId, TypeNode, UnionKind, ValidationTarget, scalar_json_type,
};
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
        | TypeNode::Integer(..)
        | TypeNode::Number
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

/// Each node's own content ([`local_signature`]) as a number: equal numbers, equal content.
/// Computed once for [`merge_candidates`] and [`dedup`]; empty with [`DedupeMode::Disabled`].
pub(crate) fn local_classes(graph: &TypeGraph, mode: DedupeMode) -> Vec<u32> {
    if mode == DedupeMode::Disabled {
        return Vec::new();
    }
    let mut ids: HashMap<String, u32> = HashMap::new();
    graph
        .nodes
        .iter()
        .map(|node| {
            let next: u32 = u32::try_from(ids.len()).expect("fewer than 2^32 classes");
            *ids.entry(local_signature(node, mode)).or_insert(next)
        })
        .collect()
}

/// The nodes whose validation must be compared for [`dedup`]: a node with a target whose own
/// content (`local`, from [`local_classes`]) equals another node's with a different target. Every
/// other node either merges with no one, or only with nodes of the same target, so its location
/// identity ([`ValidationTarget::key`]) decides as well, without compiling its schema.
pub(crate) fn merge_candidates(graph: &TypeGraph, local: &[u32]) -> Vec<bool> {
    // Per local class: the first target key seen, and whether a second one differs.
    let mut groups: HashMap<u32, (String, bool)> = HashMap::new();
    for (target, &class) in graph.targets.iter().zip(local) {
        if let Some(target) = target {
            let key: String = target.key();
            groups
                .entry(class)
                .and_modify(|(first, differs)| *differs |= *first != key)
                .or_insert((key, false));
        }
    }
    graph
        .targets
        .iter()
        .zip(local)
        .map(|(target, class)| target.is_some() && groups[class].1)
        .collect()
}

/// Merges structurally identical nodes. Nodes start in classes by [`local_signature`] and, for a
/// node with a validation target, its validation key (`validation_keys`, one per node or empty:
/// two self-validating types merge only when they validate the same); `local` holds the
/// signatures as [`local_classes`] numbers it, and is empty with [`DedupeMode::Disabled`]. Classes are split by their
/// children's classes until stable (partition refinement), so two recursive types are equal
/// exactly when their unfoldings are. Every id in the graph is then rewritten to its class's
/// lowest id, and name proposals are pooled there. Returns, per node, the node it was merged into
/// (its own id when it was not; empty with [`DedupeMode::Disabled`], where nothing is merged).
pub(crate) fn dedup(
    graph: &mut TypeGraph,
    local: &[u32],
    validation_keys: &[Option<String>],
) -> Vec<TypeId> {
    record_variant_targets(graph);
    if local.is_empty() {
        return Vec::new();
    }
    let mut class: Vec<u32> = {
        let mut ids: HashMap<(u32, Option<&str>), u32> = HashMap::new();
        local
            .iter()
            .enumerate()
            .map(|(i, &own)| {
                let next: u32 = u32::try_from(ids.len()).expect("fewer than 2^32 classes");
                let key: Option<&str> = validation_keys.get(i).and_then(Option::as_deref);
                *ids.entry((own, key)).or_insert(next)
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
    canon
}

/// Records, per `anyOf` / `oneOf` union, its variants' validation targets as built: the union's
/// walk built them, so the union's registry resolves them and its compiled branches are their
/// entries. Merging may give a variant's type the target of an equivalent schema elsewhere,
/// even in another document; the union keeps validating its variants at its own branches.
fn record_variant_targets(graph: &mut TypeGraph) {
    let mut recorded: HashMap<TypeId, Vec<Option<ValidationTarget>>> = HashMap::new();
    for (i, node) in graph.nodes.iter().enumerate() {
        if let TypeNode::Union(union) = node
            && union.kind != UnionKind::TypeArray
        {
            let targets: Vec<Option<ValidationTarget>> = union
                .variants
                .iter()
                .map(|v| graph.targets[graph.peel(v.ty).index()].clone())
                .collect();
            if targets.iter().any(Option::is_some) {
                let id: TypeId = TypeId(u32::try_from(i).expect("fewer than 2^32 nodes"));
                recorded.insert(id, targets);
            }
        }
    }
    graph.variant_targets = recorded;
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
            graph.is_named(id).then_some((slot, id))
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

/// The estimated size in bytes above which a struct or union held by value in a field or
/// union variant is boxed instead (see [`box_large_values`]). Measured on the benchmark's
/// generated types (#120): deserializing got faster as the limit fell to 256 (deep-nesting 29%,
/// k8s-deployment 21-33%, many-defs 21%, nothing slower), while 128 and 64 traded a few percent
/// between fixtures (many-defs 5% slower, k8s-deployment 3% faster) for more allocations.
const BOX_ABOVE_BYTES: u64 = 256;

/// Whether `Option<T>` is no larger than `T` for the node (Rust stores `None` in a niche of `T`).
fn has_niche(node: &TypeNode) -> bool {
    matches!(
        node,
        TypeNode::Boolean
            | TypeNode::String { .. }
            | TypeNode::Array { .. }
            | TypeNode::Map { .. }
            | TypeNode::Box(_)
            | TypeNode::Enum(_)
    )
}

/// The estimated size of `id` held by value, from the sizes of the named nodes it reaches by
/// value (`sizes`, filled for every one of them). Follows `Option` chains only.
fn value_size(graph: &TypeGraph, id: TypeId, sizes: &[Option<u64>]) -> u64 {
    let mut extra: u64 = 0;
    let mut id: TypeId = id;
    while let TypeNode::Option(inner) = graph.node(id) {
        if !has_niche(graph.node(*inner)) {
            extra += 8;
        }
        id = *inner;
    }
    let own: u64 = match graph.node(id) {
        TypeNode::Pending | TypeNode::Any => 32,
        TypeNode::Null => 0,
        TypeNode::Boolean | TypeNode::Enum(_) => 1,
        TypeNode::Integer(..) | TypeNode::Number | TypeNode::Box(_) => 8,
        TypeNode::String { .. } | TypeNode::Map { .. } => 24,
        TypeNode::Array { unique, .. } => {
            if *unique {
                56
            } else {
                24
            }
        }
        TypeNode::Option(_) => unreachable!("Option chains are followed above"),
        TypeNode::Object(_) | TypeNode::Union(_) => {
            sizes[id.index()].expect("a node's by-value children are sized before it")
        }
    };
    own + extra
}

/// The struct or union at the bottom of `id`'s `Option` chain, if any (not behind a `Box`).
fn by_value_target(graph: &TypeGraph, mut id: TypeId) -> Option<TypeId> {
    while let TypeNode::Option(inner) = graph.node(id) {
        id = *inner;
    }
    matches!(graph.node(id), TypeNode::Object(_) | TypeNode::Union(_)).then_some(id)
}

/// Boxes every struct or union held by value in a field or union variant whose estimated size
/// is above [`BOX_ABOVE_BYTES`], so no generated value is large. Codegen nests every
/// non-recursive definition by value, so without it a type's size is the sum of everything it
/// reaches: `generated/many-defs`'s root was 458 KB, and deserializing a valid instance
/// overflowed a debug build's 8 MB main thread and a release build's 2 MB thread (#120); boxed,
/// it needs 64 KB and reads 21% faster (less to move through each nested visitor). Sizes
/// are estimated for a 64-bit target from the node kinds alone (machine-independent output),
/// children first (after [`break_cycles`], by-value containment has no cycles), so a parent's
/// size counts its boxed children as pointers.
pub(crate) fn box_large_values(graph: &mut TypeGraph) {
    let mut sizes: Vec<Option<u64>> = vec![None; graph.nodes.len()];
    let mut entered: Vec<bool> = vec![false; graph.nodes.len()];
    let mut stack: Vec<TypeId> = (0..graph.nodes.len())
        .rev()
        .filter_map(|index| {
            let id: TypeId = TypeId(u32::try_from(index).expect("fewer than 2^32 nodes"));
            matches!(graph.node(id), TypeNode::Object(_) | TypeNode::Union(_)).then_some(id)
        })
        .collect();
    while let Some(&id) = stack.last() {
        if sizes[id.index()].is_some() {
            stack.pop();
            continue;
        }
        let slots: Vec<TypeId> = match graph.node(id) {
            TypeNode::Object(o) => o.fields.iter().map(|f| f.ty).collect(),
            TypeNode::Union(u) => u.variants.iter().map(|v| v.ty).collect(),
            _ => unreachable!("only structs and unions are stacked"),
        };
        if !entered[id.index()] {
            // First visit: size every struct or union this one holds by value first.
            entered[id.index()] = true;
            for slot in slots.iter().rev() {
                if let Some(target) = by_value_target(graph, *slot)
                    && sizes[target.index()].is_none()
                {
                    stack.push(target);
                }
            }
            continue;
        }
        stack.pop();
        let mut node_value: TypeNode = graph.nodes[id.index()].clone();
        let held: Vec<&mut TypeId> = match &mut node_value {
            TypeNode::Object(o) => o.fields.iter_mut().map(|f| &mut f.ty).collect(),
            TypeNode::Union(u) => u.variants.iter_mut().map(|v| &mut v.ty).collect(),
            _ => unreachable!("only structs and unions are stacked"),
        };
        for slot in held {
            if let Some(target) = by_value_target(graph, *slot)
                && sizes[target.index()].is_some_and(|size| size > BOX_ABOVE_BYTES)
            {
                *slot = boxed(graph, *slot);
            }
        }
        let size: u64 = match &node_value {
            TypeNode::Object(o) => {
                let fields: u64 = o
                    .fields
                    .iter()
                    .map(|f| value_size(graph, f.ty, &sizes))
                    .sum();
                fields + if o.additional.is_some() { 24 } else { 0 }
            }
            TypeNode::Union(u) => {
                8 + u
                    .variants
                    .iter()
                    .map(|v| value_size(graph, v.ty, &sizes))
                    .max()
                    .unwrap_or(0)
            }
            _ => unreachable!("only structs and unions are stacked"),
        };
        graph.nodes[id.index()] = node_value;
        sizes.resize(graph.nodes.len(), None);
        entered.resize(graph.nodes.len(), false);
        sizes[id.index()] = Some(size);
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
