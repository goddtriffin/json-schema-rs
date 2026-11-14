//! Which nodes of several compiled schemas validate identically: what lets codegen merge two
//! self-validating types whose schemas sit at different locations, or in different documents,
//! but validate every instance the same, errors included (see design.md, *Self-validating
//! types*).
//!
//! Two nodes are equivalent when their own content is equal (keywords and their payloads, with
//! patterns, `enum` lists and `properties` names compared by content, and the canonical URI of
//! every `$ref` target, which starts the absolute keyword location of errors past it) and their
//! children are equivalent, position by position. That is the coarsest partition stable under
//! refinement, computed as `dedup` computes it for types: start from classes by own content,
//! split by the children's classes until nothing splits. Evaluation is a function of a node's
//! content and its children's evaluation, so equivalent nodes give every instance the same
//! verdict, errors and annotations, through cycles too. A node's own target URI is not part of
//! it: an entry is evaluated from its own frame, where no `$ref` has been crossed yet, and a
//! `$ref` edge carries its target's URI.
//!
//! A schema that tracks the dynamic scope (`$dynamicRef`, `$recursiveRef`) is left out: there a
//! node's evaluation also depends on the resources it is reached through.

use super::compiled::{CompiledSchema, Keyword, KnownProperties, NodeId};
use crate::json_schema::IntegerSemantics;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::{BuildHasherDefault, Hasher};

/// Per schema, the validation class of each of its nodes (classes are shared across the
/// schemas: equal classes validate identically), or `None` for a schema that tracks the dynamic
/// scope.
pub(crate) fn validation_classes(schemas: &[&CompiledSchema]) -> Vec<Option<Vec<u32>>> {
    let included: Vec<bool> = schemas.iter().map(|s| !s.uses_dynamic_scope).collect();
    // Every included node, with a global index: (schema, node).
    let mut offsets: Vec<usize> = Vec::with_capacity(schemas.len());
    let mut total: usize = 0;
    for (schema, &included) in schemas.iter().zip(&included) {
        offsets.push(total);
        if included {
            total += schema.nodes.len();
        }
    }
    // Children of every node, flattened: node `i`'s are `children[starts[i]..starts[i + 1]]`.
    let mut starts: Vec<usize> = Vec::with_capacity(total + 1);
    let mut children: Vec<usize> = Vec::new();
    let mut class: Vec<u32> = Vec::with_capacity(total);
    {
        let mut ids: HashMap<String, u32> = HashMap::new();
        let mut shape: String = String::new();
        let mut edges: Vec<NodeId> = Vec::new();
        for ((schema, &included), &offset) in schemas.iter().zip(&included).zip(&offsets) {
            if !included {
                continue;
            }
            for index in 0..schema.nodes.len() {
                shape.clear();
                edges.clear();
                node_shape(schema, index, &mut shape, &mut edges);
                let own: u32 = if let Some(&own) = ids.get(&shape) {
                    own
                } else {
                    let next: u32 = u32::try_from(ids.len()).expect("fewer than 2^32 classes");
                    ids.insert(shape.clone(), next);
                    next
                };
                class.push(own);
                starts.push(children.len());
                children.extend(edges.iter().map(|c| offset + c.index()));
            }
        }
    }
    starts.push(children.len());
    let mut count: usize = class.iter().max().map_or(0, |&m| m as usize + 1);
    // Each round's keys, flattened the same way with the node's own class first: node `i`'s key
    // is `keys[starts[i] + i..starts[i + 1] + i + 1]`.
    let mut keys: Vec<u32> = Vec::with_capacity(total + children.len());
    loop {
        keys.clear();
        for (i, &own) in class.iter().enumerate() {
            keys.push(own);
            keys.extend(children[starts[i]..starts[i + 1]].iter().map(|&c| class[c]));
        }
        let mut ids: HashMap<&[u32], u32, BuildHasherDefault<WordHasher>> =
            HashMap::with_capacity_and_hasher(count, BuildHasherDefault::default());
        let next_class: Vec<u32> = (0..class.len())
            .map(|i| {
                let key: &[u32] = &keys[starts[i] + i..=starts[i + 1] + i];
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
    schemas
        .iter()
        .zip(&included)
        .zip(&offsets)
        .map(|((schema, &included), &offset)| {
            included.then(|| class[offset..offset + schema.nodes.len()].to_vec())
        })
        .collect()
}

/// A fast hasher for the refinement's keys (slices of class numbers, hashed once per node per
/// round): a multiply-rotate mix per word. The keys are our own numbers, so the flooding
/// resistance of the default hasher buys nothing here.
#[derive(Default)]
struct WordHasher(u64);

impl Hasher for WordHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    /// A slice of integers arrives here as its bytes: mixed eight at a time.
    fn write(&mut self, bytes: &[u8]) {
        let (words, remainder): (&[[u8; 8]], &[u8]) = bytes.as_chunks::<8>();
        for word in words {
            self.write_u64(u64::from_le_bytes(*word));
        }
        let mut rest: [u8; 8] = [0; 8];
        rest[..remainder.len()].copy_from_slice(remainder);
        self.write_u64(u64::from_le_bytes(rest));
    }

    fn write_u32(&mut self, word: u32) {
        self.write_u64(u64::from(word));
    }

    fn write_u64(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_usize(&mut self, word: usize) {
        self.write_u64(word as u64);
    }
}

/// Writes node `index`'s own content to `out` and pushes its children to `edges`, in keyword
/// order. Every child's position is fixed by `out` (it records each keyword and how many
/// children it has), so equal `out` and equivalent `edges` mean equal evaluation.
#[expect(clippy::too_many_lines, reason = "one arm per keyword")]
fn node_shape(schema: &CompiledSchema, index: usize, out: &mut String, edges: &mut Vec<NodeId>) {
    let node = &schema.nodes[index];
    let _ = write!(out, "{}|", node.has_unevaluated);
    for keyword in &node.keywords {
        match keyword {
            // The commonest keywords are written by hand: `Debug` formatting dominates the cost.
            Keyword::False => out.push('F'),
            Keyword::UniqueItems => out.push('U'),
            Keyword::Type {
                types,
                integer_semantics,
            } => {
                out.push_str("Type(");
                for ty in types.iter() {
                    out.push_str(ty.as_str());
                    out.push(',');
                }
                out.push(match integer_semantics {
                    IntegerSemantics::Lexical => 'L',
                    IntegerSemantics::Numeric => 'N',
                });
                out.push(')');
            }
            Keyword::MinLength(n) => {
                let _ = write!(out, "minLength{n}");
            }
            Keyword::MaxLength(n) => {
                let _ = write!(out, "maxLength{n}");
            }
            Keyword::MinItems(n) => {
                let _ = write!(out, "minItems{n}");
            }
            Keyword::MaxItems(n) => {
                let _ = write!(out, "maxItems{n}");
            }
            Keyword::MinProperties(n) => {
                let _ = write!(out, "minProperties{n}");
            }
            Keyword::MaxProperties(n) => {
                let _ = write!(out, "maxProperties{n}");
            }
            Keyword::Required(required) => {
                out.push_str("Required(");
                for name in &required.names {
                    let _ = write!(out, "{}:{name},", name.len());
                }
                out.push(')');
            }
            Keyword::Const(_)
            | Keyword::Minimum(_)
            | Keyword::Maximum(_)
            | Keyword::ExclusiveMinimum(_)
            | Keyword::ExclusiveMaximum(_)
            | Keyword::MultipleOf(_)
            | Keyword::Format(_)
            | Keyword::Content(_)
            | Keyword::DependentRequired { .. } => {
                let _ = write!(out, "{keyword:?}");
            }
            Keyword::Enum(id) => {
                let _ = write!(out, "Enum({:?})", schema.enum_values(*id));
            }
            Keyword::Pattern(id) => {
                let _ = write!(out, "Pattern({:?})", schema.pattern(*id).source);
            }
            Keyword::UserFormat(id) => {
                let _ = write!(out, "UserFormat({:?})", schema.user_format(*id).name);
            }
            Keyword::NoAdditionalProperties(known) => {
                out.push_str("NoAdditionalProperties");
                known_shape(schema, known, out);
            }
            Keyword::Ref {
                target,
                keyword,
                ref_str,
            } => {
                let uri: Option<&str> = schema.target_uris.get(target).map(|u| &**u);
                let _ = write!(out, "Ref({keyword:?}, {ref_str:?}, {uri:?})");
                edges.push(*target);
            }
            Keyword::DynamicRef(dynamic) => {
                let _ = write!(
                    out,
                    "DynamicRef({:?}, {:?})",
                    dynamic.anchor, dynamic.ref_str
                );
                edges.push(dynamic.fallback);
            }
            Keyword::RecursiveRef { fallback, ref_str } => {
                let _ = write!(out, "RecursiveRef({ref_str:?})");
                edges.push(*fallback);
            }
            Keyword::Properties(id) | Keyword::ClosedProperties(id) => {
                let closed: bool = matches!(keyword, Keyword::ClosedProperties(_));
                let _ = write!(out, "Properties({closed}");
                for (name, node) in schema.property_table(*id).iter() {
                    let _ = write!(out, ", {name:?}");
                    edges.push(node);
                }
                out.push(')');
            }
            Keyword::PatternProperties(list) => {
                out.push_str("PatternProperties(");
                for (regex, node) in list {
                    let _ = write!(out, "{:?}, ", schema.pattern(*regex).source);
                    edges.push(*node);
                }
                out.push(')');
            }
            Keyword::AdditionalProperties { node, known } => {
                out.push_str("AdditionalProperties");
                known_shape(schema, known, out);
                edges.push(*node);
            }
            Keyword::PropertyNames(node) => {
                out.push_str("PropertyNames");
                edges.push(*node);
            }
            Keyword::DependentSchemas {
                dependencies,
                keyword,
            } => {
                let _ = write!(out, "DependentSchemas({keyword:?}");
                for (name, node) in dependencies {
                    let _ = write!(out, ", {name:?}");
                    edges.push(*node);
                }
                out.push(')');
            }
            Keyword::Items { node, keyword } => {
                let _ = write!(out, "Items({keyword:?})");
                edges.push(*node);
            }
            Keyword::PrefixItems(items) => {
                let _ = write!(
                    out,
                    "PrefixItems({}, {}, {:?}, {:?})",
                    items.prefix.len(),
                    items.rest.is_some(),
                    items.prefix_keyword,
                    items.rest_keyword
                );
                edges.extend(items.prefix.iter().copied().chain(items.rest));
            }
            Keyword::Contains(contains) => {
                let _ = write!(
                    out,
                    "Contains({}, {:?}, {}, {:?})",
                    contains.min, contains.max, contains.annotates, contains.min_keyword
                );
                edges.push(contains.node);
            }
            Keyword::AllOf(nodes) => {
                let _ = write!(out, "AllOf({})", nodes.len());
                edges.extend(nodes.iter().copied());
            }
            Keyword::AnyOf(union) | Keyword::OneOf(union) => {
                let any: bool = matches!(keyword, Keyword::AnyOf(_));
                let _ = write!(
                    out,
                    "Union({any}, {}, {:?})",
                    union.nodes.len(),
                    union.discriminator
                );
                edges.extend(union.nodes.iter().copied());
            }
            Keyword::Not(node) => {
                out.push_str("Not");
                edges.push(*node);
            }
            Keyword::IfThenElse {
                condition,
                then,
                otherwise,
            } => {
                let _ = write!(out, "If({}, {})", then.is_some(), otherwise.is_some());
                edges.extend(std::iter::once(*condition).chain(*then).chain(*otherwise));
            }
            Keyword::UnevaluatedProperties(node) => {
                out.push_str("UnevaluatedProperties");
                edges.push(*node);
            }
            Keyword::UnevaluatedItems(node) => {
                out.push_str("UnevaluatedItems");
                edges.push(*node);
            }
        }
        out.push(';');
    }
}

/// The members `additionalProperties` skips, by content: `properties` names and pattern sources.
fn known_shape(schema: &CompiledSchema, known: &KnownProperties, out: &mut String) {
    out.push('(');
    if let Some(id) = known.names {
        for (name, _) in schema.property_table(id).iter() {
            let _ = write!(out, "{name:?}, ");
        }
    }
    out.push('|');
    for regex in &known.patterns {
        let _ = write!(out, "{:?}, ", schema.pattern(*regex).source);
    }
    out.push(')');
}

#[cfg(test)]
mod tests {
    use super::validation_classes;
    use crate::json_schema::{Registry, Schema};
    use crate::validator::compiled::{CompiledEntries, CompiledSchema};
    use serde_json::{Value, json};

    /// `document` (no URI of its own) compiled with one entry per pointer.
    fn compile(document: &Value, pointers: &[&str]) -> CompiledEntries {
        let schema: Schema = serde_json::from_value(document.clone()).expect("test schema parses");
        let uris: Vec<String> = pointers
            .iter()
            .map(|pointer| format!("json-schema:///#{pointer}"))
            .collect();
        let mut builder = Registry::builder();
        for uri in &uris {
            builder = builder.entry(uri);
        }
        let registry: Registry<'static> = builder.build_owned(schema).expect("registry builds");
        let entries: Vec<[&str; 1]> = uris.iter().map(|uri| [uri.as_str()]).collect();
        let entries: Vec<&[&str]> = entries.iter().map(<[&str; 1]>::as_slice).collect();
        CompiledSchema::with_entries(&registry, &entries).expect("test schema compiles")
    }

    /// Whether the schemas at `a` and `b` in `document` validate identically.
    fn equivalent(document: &Value, a: &str, b: &str) -> bool {
        let (schema, entries) = compile(document, &[a, b]);
        let classes: Vec<u32> = validation_classes(&[&schema])
            .remove(0)
            .expect("no dynamic scope");
        classes[entries[0][0].index()] == classes[entries[1][0].index()]
    }

    #[test]
    fn identical_subschemas_at_different_locations_are_equivalent() {
        let document: Value = json!({"properties": {
            "a": {"type": "object", "properties": {"x": {"type": "string", "minLength": 1}}},
            "b": {"type": "object", "properties": {"x": {"type": "string", "minLength": 1}}}
        }});
        let expected: bool = true;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_do_not_tell_subschemas_apart() {
        let document: Value = json!({"properties": {
            "a": {"title": "A", "description": "one", "type": "string"},
            "b": {"title": "B", "type": "string"}
        }});
        let expected: bool = true;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_different_nested_keyword_value_tells_subschemas_apart() {
        let document: Value = json!({"properties": {
            "a": {"properties": {"x": {"minLength": 1}}},
            "b": {"properties": {"x": {"minLength": 2}}}
        }});
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_different_property_name_tells_subschemas_apart() {
        let document: Value = json!({"properties": {
            "a": {"properties": {"x": {"type": "string"}}, "additionalProperties": false},
            "b": {"properties": {"y": {"type": "string"}}, "additionalProperties": false}
        }});
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_different_pattern_tells_subschemas_apart() {
        let document: Value = json!({"properties": {
            "a": {"pattern": "^a"},
            "b": {"pattern": "^b"}
        }});
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_different_enum_tells_subschemas_apart() {
        let document: Value = json!({"properties": {
            "a": {"enum": ["x", "y"]},
            "b": {"enum": ["x", "z"]}
        }});
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn references_to_the_same_target_are_equivalent() {
        let document: Value = json!({
            "$defs": {"T": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/T"}, "b": {"$ref": "#/$defs/T"}}
        });
        let expected: bool = true;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn references_to_identical_targets_at_different_locations_are_not_equivalent() {
        // Errors past a `$ref` carry the target's absolute location.
        let document: Value = json!({
            "$defs": {"T": {"type": "string"}, "U": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/T"}, "b": {"$ref": "#/$defs/U"}}
        });
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/properties/a", "/properties/b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn identical_reference_targets_are_equivalent_as_entries() {
        // An entry is evaluated from its own frame: its location as a target does not matter.
        let document: Value = json!({
            "$defs": {"T": {"type": "string"}, "U": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/T"}, "b": {"$ref": "#/$defs/U"}}
        });
        let expected: bool = true;
        let actual: bool = equivalent(&document, "/$defs/T", "/$defs/U");
        assert_eq!(expected, actual);
    }

    #[test]
    fn identical_recursive_subschemas_are_equivalent() {
        let document: Value = json!({"$defs": {
            "A": {"properties": {"next": {"$ref": "#/$defs/A"}}},
            "B": {"properties": {"next": {"$ref": "#/$defs/B"}}}
        }});
        // Each refers to its own location, so errors past the `$ref` differ.
        let expected: bool = false;
        let actual: bool = equivalent(&document, "/$defs/A", "/$defs/B");
        assert_eq!(expected, actual);
    }

    #[test]
    fn identical_schemas_in_two_compilations_are_equivalent() {
        let document = |minimum: u64| -> Value {
            json!({"properties": {"a": {"items": {"minimum": minimum}}}})
        };
        let (first, first_entries) = compile(&document(1), &["/properties/a"]);
        let (second, second_entries) = compile(&document(1), &["/properties/a"]);
        let (third, third_entries) = compile(&document(2), &["/properties/a"]);
        let classes: Vec<Option<Vec<u32>>> = validation_classes(&[&first, &second, &third]);
        let class = |at: usize, entries: &[Box<[crate::validator::NodeId]>]| -> u32 {
            classes[at].as_ref().expect("no dynamic scope")[entries[0][0].index()]
        };
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            class(0, &first_entries) == class(1, &second_entries),
            class(0, &first_entries) == class(2, &third_entries),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_schema_with_dynamic_scope_has_no_classes() {
        let document: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$dynamicAnchor": "node",
            "properties": {"child": {"$dynamicRef": "#node"}}
        });
        let (schema, _) = compile(&document, &[""]);
        let expected: Vec<Option<Vec<u32>>> = vec![None];
        let actual: Vec<Option<Vec<u32>>> = validation_classes(&[&schema]);
        assert_eq!(expected, actual);
    }
}
