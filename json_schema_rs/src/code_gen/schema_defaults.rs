//! The schema `default`s generated types implement `Default` from
//! ([`CodeGenSettings::schema_defaults`](super::CodeGenSettings::schema_defaults)).
//!
//! A type's default is its own schema's `default`, else, for a struct, the object of its
//! properties' `default`s when at least one has one. Codegen writes an `impl Default` only for a
//! value it has checked twice: against the type's schema, as the type's own validation would,
//! and against the Rust type itself (every value fits where it goes, every required field is
//! there, a typed `format` string is valid for its format, which a draft that only annotates
//! formats does not check). A value either check rejects, or a type whose shape codegen cannot
//! see (an external type), gets no impl, so the generated `default()` never panics.

use super::rust_validators::{self, EmbeddedSet};
use super::type_graph::{Annotations, StringFormat, TypeGraph, TypeId, TypeNode, ValidationTarget};
use crate::json_schema::IntegerSemantics;
use crate::validator::format::Format;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// How deep [`fits`] follows a value into nested types before giving up (no impl).
const MAX_DEPTH: usize = 64;

/// Per item that gets an `impl Default`, its default as compact JSON. `items` are the named items
/// and newtypes the plan emits.
pub(crate) fn schema_defaults(
    graph: &TypeGraph,
    sets: &[EmbeddedSet],
    items: impl IntoIterator<Item = TypeId>,
) -> HashMap<TypeId, String> {
    let mut candidates: Vec<(TypeId, Value)> = Vec::new();
    for id in items {
        let Some(value) = candidate(graph, id) else {
            continue;
        };
        if graph.default_targets[id.index()].is_some() && fits(graph, id, &value, 0) {
            candidates.push((id, value));
        }
    }
    let checks: Vec<(&ValidationTarget, &Value)> = candidates
        .iter()
        .map(|(id, value)| {
            let target: &ValidationTarget = graph.default_targets[id.index()]
                .as_ref()
                .expect("a candidate has a target");
            (target, value)
        })
        .collect();
    let valid: Vec<bool> = rust_validators::defaults_satisfy(sets, &checks);
    candidates
        .into_iter()
        .zip(valid)
        .filter(|(_, valid)| *valid)
        .map(|((id, value), _)| (id, value.to_string()))
        .collect()
}

/// The annotations of item `id`: a named item's own, or its newtype's.
fn annotations(graph: &TypeGraph, id: TypeId) -> Option<&Annotations> {
    match graph.node(id) {
        TypeNode::Object(o) => Some(&o.annotations),
        TypeNode::Enum(e) => Some(&e.annotations),
        TypeNode::Union(u) => Some(&u.annotations),
        _ => graph.newtypes[id.index()]
            .as_deref()
            .map(|n| &n.annotations),
    }
}

/// The default of item `id`, before any check.
fn candidate(graph: &TypeGraph, id: TypeId) -> Option<Value> {
    if let Some(value) = annotations(graph, id).and_then(|a| a.default.clone()) {
        return Some(value);
    }
    let TypeNode::Object(object) = graph.node(id) else {
        return None;
    };
    let fields: Map<String, Value> = object
        .fields
        .iter()
        .filter_map(|f| f.default.clone().map(|d| (f.json_key.clone(), d)))
        .collect();
    (!fields.is_empty()).then_some(Value::Object(fields))
}

/// Whether `value` deserializes into the Rust type of node `id`, as far as its shape decides
/// (the schema check covers the rest). `false` past [`MAX_DEPTH`].
fn fits(graph: &TypeGraph, id: TypeId, value: &Value, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    let deeper = |id: TypeId, value: &Value| fits(graph, id, value, depth + 1);
    match graph.node(id) {
        TypeNode::Any => true,
        TypeNode::Pending | TypeNode::External { .. } => false,
        TypeNode::Null => value.is_null(),
        TypeNode::Boolean => value.is_boolean(),
        TypeNode::Integer(width, semantics) => integer_fits(value, width.range(), *semantics),
        TypeNode::Number => value.is_number(),
        TypeNode::String { format, .. } => value.as_str().is_some_and(|text| match format {
            StringFormat::Plain => true,
            StringFormat::Uuid => Format::Uuid.is_valid(text),
            StringFormat::DateTime => Format::DateTime.is_valid(text),
            StringFormat::Date => Format::Date.is_valid(text),
            StringFormat::Ipv4 => {
                Format::Ipv4.is_valid(text) && text.parse::<std::net::Ipv4Addr>().is_ok()
            }
            StringFormat::Ipv6 => {
                Format::Ipv6.is_valid(text) && text.parse::<std::net::Ipv6Addr>().is_ok()
            }
        }),
        TypeNode::Array { items, .. } => value
            .as_array()
            .is_some_and(|values| values.iter().all(|v| deeper(*items, v))),
        TypeNode::Map { values } => value
            .as_object()
            .is_some_and(|map| map.values().all(|v| deeper(*values, v))),
        TypeNode::Option(inner) => value.is_null() || deeper(*inner, value),
        TypeNode::Box(inner) => deeper(*inner, value),
        TypeNode::Enum(e) => e.values.contains(value),
        TypeNode::Union(u) => u.variants.iter().any(|v| deeper(v.ty, value)),
        TypeNode::Object(object) => {
            let Some(map) = value.as_object() else {
                return false;
            };
            let required_present: bool = object
                .fields
                .iter()
                .all(|f| !f.required || map.contains_key(&f.json_key));
            required_present
                && map.iter().all(|(key, v)| {
                    match object.fields.iter().find(|f| f.json_key == *key) {
                        Some(field) => deeper(field.ty, v),
                        None => match object.additional {
                            Some(additional) => deeper(additional, v),
                            None => !object.deny_unknown,
                        },
                    }
                })
        }
    }
}

/// Whether `value` is an integer in `range`: written without a fraction, or (with numeric
/// semantics) a float with a zero fraction.
fn integer_fits(value: &Value, (min, max): (i128, i128), semantics: IntegerSemantics) -> bool {
    let Value::Number(number) = value else {
        return false;
    };
    let exact: Option<i128> = number
        .as_i64()
        .map(i128::from)
        .or_else(|| number.as_u64().map(i128::from));
    let integer: Option<i128> = exact.or_else(|| match semantics {
        IntegerSemantics::Lexical => None,
        IntegerSemantics::Numeric => number.as_f64().filter(|f| f.fract() == 0.0).and_then(|f| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a zero-fraction float in the i128 range converts exactly; out-of-range saturates and fails the range check"
            )]
            let i: i128 = f as i128;
            #[expect(
                clippy::cast_precision_loss,
                clippy::float_cmp,
                reason = "compares the saturated conversion back to the float, exactly"
            )]
            let same: bool = i as f64 == f;
            same.then_some(i)
        }),
    });
    integer.is_some_and(|i| (min..=max).contains(&i))
}
