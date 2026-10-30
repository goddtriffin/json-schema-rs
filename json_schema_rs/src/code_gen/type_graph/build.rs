//! Builds a [`TypeGraph`] from parsed schemas and their `$ref` registries.
//!
//! **Conjunctions.** A node is built from a *conjunction*: the set of schema objects an instance
//! must satisfy at once. A property schema, its `$ref` target (with its siblings in 2019-09 and
//! later) and every `allOf` branch are one conjunction, so `allOf` merging, `$ref` siblings and
//! `allOf` of a `$ref` (#18) are the same operation. Flattening follows `$ref` and `allOf` with a
//! visited set, so a circular `allOf` or `$ref` chain simply contributes each schema once, and a
//! loop that carries no keywords at all (`{"$ref": "#"}`) is an empty conjunction: any value.
//!
//! **Identity.** A conjunction's key is the sorted set of its schema objects (by address, with
//! the keywords already consumed), plus a type mask used to split nullable and multi-type
//! schemas. Each key gets one [`TypeId`], reserved before its node is built, so recursion
//! through `$ref` reaches an id that already exists and the walk terminates (#35). The walk is an
//! explicit work queue: no recursion, whatever the schema depth.

use super::{
    Annotations, ArrayConstraints, Field, FloatWidth, IntWidth, NameProposal, NameTier, ObjectNode,
    SchemaRoots, StringConstraints, StringFormat, TypeGraph, TypeId, TypeNode, UnionKind,
    UnionNode, Variant,
};
use crate::code_gen::settings::{CodeGenSettings, ModelNameSource, UnknownProperties};
use crate::code_gen::{CodeGenError, CodeGenResult};
use crate::json_schema::{
    JsonSchema, JsonSchemaSettings, JsonType, RefSiblings, Registry, Schema, SchemaRef, Scope,
    TypeSet,
};
use crate::number;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Keywords of a schema object a conjunct no longer contributes (already expanded, or not
/// carried into a union's variants).
mod skip {
    pub(super) const REF: u8 = 1;
    pub(super) const ALL_OF: u8 = 1 << 1;
    pub(super) const ANNOTATIONS: u8 = 1 << 2;
    pub(super) const ANY_OF: u8 = 1 << 3;
    pub(super) const ONE_OF: u8 = 1 << 4;
}

/// One schema object of a conjunction.
#[derive(Debug, Clone)]
struct Conjunct<'r> {
    schema: &'r JsonSchema,
    /// The resource the schema is evaluated in.
    scope: Scope,
    /// JSON Pointer of the schema within its document.
    pointer: String,
    /// Keywords already consumed ([`skip`]).
    skip: u8,
}

impl Conjunct<'_> {
    fn key(&self) -> (usize, u8) {
        (std::ptr::from_ref(self.schema) as usize, self.skip)
    }

    fn skips(&self, bit: u8) -> bool {
        self.skip & bit != 0
    }
}

/// A node to build: the seed conjuncts and the JSON types it may hold.
#[derive(Debug, Clone)]
struct Request<'r> {
    conjuncts: Vec<Conjunct<'r>>,
    mask: TypeSet,
}

/// The flattened conjunction of a request.
#[derive(Debug, Clone)]
struct Flat<'r> {
    /// Contributing conjuncts in discovery order (seed first).
    conjuncts: Vec<Conjunct<'r>>,
    /// A `false` schema is part of the conjunction: nothing is valid.
    never: bool,
}

/// Identity of a node: sorted conjunct keys plus the type mask.
type NodeKey = (Vec<(usize, u8)>, TypeSet, bool);

/// A name proposal a request carries to the node it creates: tier and text. The origin is the
/// request's pointer.
#[derive(Debug, Clone)]
struct Context {
    schema_index: usize,
    /// Where the request was made (for name ordering).
    origin: String,
    /// The property key (or derived text) the node is reached under, if any.
    key_name: Option<String>,
    /// Whether this is the root of a schema (proposes `Root`).
    is_root: bool,
}

/// `then` / `else` subschemas, parsed before the walk so they live as long as the registries
/// (the typed model keeps `if` / `then` / `else` raw), plus, for each schema with both, the
/// properties both define merged as `{"anyOf": [then's, else's]}`.
#[derive(Debug, Default)]
pub(crate) struct Conditionals {
    /// Parsed schemas, boxed so their addresses stay fixed while the list grows: nested
    /// `then` / `else` values are found by address inside them.
    #[expect(clippy::vec_box, reason = "addresses of parsed schemas must not move")]
    parsed: Vec<Box<JsonSchema>>,
    /// Raw `then` / `else` value address → index in `parsed`.
    index: HashMap<usize, usize>,
    /// (`then` index, `else` index, property key) → index in `parsed` of the merged property.
    merged: HashMap<(usize, usize, String), usize>,
}

impl Conditionals {
    /// Parses every `then` / `else` reachable from `schemas` and the `$ref` targets of
    /// `registries`, including those nested in other `then` / `else` values.
    pub(crate) fn collect(
        schemas: &[JsonSchema],
        registries: &[Registry<'_>],
    ) -> CodeGenResult<Conditionals> {
        let mut out: Conditionals = Conditionals::default();
        let mut found: Vec<(usize, Value)> = Vec::new();
        let mut stack: Vec<&JsonSchema> = schemas.iter().collect();
        for registry in registries {
            for target in registry.ref_targets() {
                if let SchemaRef::Object(s) = target.schema {
                    stack.push(s);
                }
            }
        }
        while let Some(s) = stack.pop() {
            push_typed_children(s, &mut stack);
            queue_conditionals(s, &mut found);
        }
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut next: usize = 0;
        while next < found.len() {
            let (address, ref value) = found[next];
            next += 1;
            if out.index.contains_key(&address) {
                continue;
            }
            let parsed: Box<JsonSchema> = Box::new(
                JsonSchema::new_from_serde_value(value, &JsonSchemaSettings::default()).map_err(
                    |e| CodeGenError::InvalidSubschema {
                        reason: e.to_string(),
                    },
                )?,
            );
            out.index.insert(address, out.parsed.len());
            out.parsed.push(parsed);
        }
        // Nested `then` / `else` inside parsed values: their addresses are inside the boxes.
        let mut scan: usize = 0;
        while scan < out.parsed.len() {
            let mut nested: Vec<(usize, Value)> = Vec::new();
            let mut inner: Vec<&JsonSchema> = vec![out.parsed[scan].as_ref()];
            while let Some(s) = inner.pop() {
                push_typed_children(s, &mut inner);
                queue_conditionals(s, &mut nested);
            }
            scan += 1;
            for (address, value) in nested {
                if out.index.contains_key(&address) {
                    continue;
                }
                let parsed: Box<JsonSchema> = Box::new(
                    JsonSchema::new_from_serde_value(&value, &JsonSchemaSettings::default())
                        .map_err(|e| CodeGenError::InvalidSubschema {
                            reason: e.to_string(),
                        })?,
                );
                out.index.insert(address, out.parsed.len());
                out.parsed.push(parsed);
            }
        }
        // Pair up `then` and `else` of the same schema.
        let mut stack: Vec<&JsonSchema> = schemas.iter().collect();
        for registry in registries {
            for target in registry.ref_targets() {
                if let SchemaRef::Object(s) = target.schema {
                    stack.push(s);
                }
            }
        }
        stack.extend(out.parsed.iter().map(AsRef::as_ref));
        while let Some(s) = stack.pop() {
            push_typed_children(s, &mut stack);
            if let (Some(t), Some(e)) = (out.position(s, "then"), out.position(s, "else")) {
                pairs.push((t, e));
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        for (then_index, else_index) in pairs {
            let keys: Vec<String> = out.parsed[then_index]
                .properties
                .keys()
                .filter(|k| out.parsed[else_index].properties.contains_key(*k))
                .cloned()
                .collect();
            for key in keys {
                let merged: JsonSchema = JsonSchema {
                    any_of: Some(vec![
                        out.parsed[then_index].properties[&key].clone(),
                        out.parsed[else_index].properties[&key].clone(),
                    ]),
                    ..Default::default()
                };
                out.merged
                    .insert((then_index, else_index, key), out.parsed.len());
                out.parsed.push(Box::new(merged));
            }
        }
        Ok(out)
    }

    /// Index in `parsed` of `s`'s `keyword` (`then` / `else`) value.
    fn position(&self, s: &JsonSchema, keyword: &str) -> Option<usize> {
        let value: &Value = s.extensions.get(keyword)?;
        self.index
            .get(&(std::ptr::from_ref(value) as usize))
            .copied()
    }

    /// `s`'s parsed `keyword` (`then` / `else`) schema.
    fn get(&self, s: &JsonSchema, keyword: &str) -> Option<&JsonSchema> {
        self.position(s, keyword).map(|i| self.parsed[i].as_ref())
    }

    /// The merged schema for `key` defined in both `s`'s `then` and `else`.
    fn merged(&self, s: &JsonSchema, key: &str) -> Option<&JsonSchema> {
        let t: usize = self.position(s, "then")?;
        let e: usize = self.position(s, "else")?;
        self.merged
            .get(&(t, e, key.to_string()))
            .map(|&i| self.parsed[i].as_ref())
    }
}

/// Queues `s`'s raw `then` / `else` object values with their addresses.
fn queue_conditionals(s: &JsonSchema, found: &mut Vec<(usize, Value)>) {
    for keyword in ["then", "else"] {
        if let Some(value @ Value::Object(_)) = s.extensions.get(keyword) {
            found.push((std::ptr::from_ref(value) as usize, value.clone()));
        }
    }
}

/// Pushes the typed subschema objects of `s` (every position the model types).
fn push_typed_children<'a>(s: &'a JsonSchema, stack: &mut Vec<&'a JsonSchema>) {
    let maps = [s.defs.as_ref(), s.definitions.as_ref()];
    for map in maps.into_iter().flatten() {
        stack.extend(map.values().filter_map(Schema::as_object));
    }
    stack.extend(s.properties.values().filter_map(Schema::as_object));
    stack.extend(s.additional_properties.iter().filter_map(Schema::as_object));
    stack.extend(s.items.iter().filter_map(Schema::as_object));
    for list in [&s.all_of, &s.any_of, &s.one_of].into_iter().flatten() {
        stack.extend(list.iter().filter_map(Schema::as_object));
    }
}

/// Appends a JSON Pointer token (RFC 6901 escaped) to `pointer`.
fn pointer_push(pointer: &str, token: &str) -> String {
    let escaped: String = token.replace('~', "~0").replace('/', "~1");
    format!("{pointer}/{escaped}")
}

/// Builds the type graph for `schemas`; `registries[i]` is `schemas[i]`'s registry.
pub(crate) fn build(
    schemas: &[JsonSchema],
    registries: &[Registry<'_>],
    conditionals: &Conditionals,
    settings: &CodeGenSettings,
) -> CodeGenResult<TypeGraph> {
    let mut graph: TypeGraph = TypeGraph::default();
    let mut memo: HashMap<(usize, NodeKey), TypeId> = HashMap::new();
    for (index, (schema, registry)) in schemas.iter().zip(registries).enumerate() {
        let mut builder: Builder<'_, '_> = Builder {
            graph: &mut graph,
            memo: &mut memo,
            registry,
            conditionals,
            settings,
            schema_index: index,
            queue: Vec::new(),
        };
        let roots: SchemaRoots = builder
            .build_schema(schema)
            .map_err(|e| CodeGenError::Batch {
                index,
                source: Box::new(e),
            })?;
        graph.schemas.push(roots);
    }
    Ok(graph)
}

struct Builder<'g, 'r> {
    graph: &'g mut TypeGraph,
    memo: &'g mut HashMap<(usize, NodeKey), TypeId>,
    registry: &'r Registry<'r>,
    conditionals: &'r Conditionals,
    settings: &'g CodeGenSettings,
    schema_index: usize,
    /// Reserved ids still to build.
    queue: Vec<(TypeId, Flat<'r>, TypeSet, Context)>,
}

impl<'r> Builder<'_, 'r> {
    fn build_schema(&mut self, schema: &'r JsonSchema) -> CodeGenResult<SchemaRoots> {
        let root_scope: Scope = self.registry.enter(self.registry.root_scope(), schema);
        let root_conjunct: Conjunct<'r> = Conjunct {
            schema,
            scope: root_scope,
            pointer: String::new(),
            skip: 0,
        };
        let root: TypeId = self.request(
            Request {
                conjuncts: vec![root_conjunct],
                mask: all_types(),
            },
            &Context {
                schema_index: self.schema_index,
                origin: String::new(),
                key_name: None,
                is_root: true,
            },
        )?;
        let mut definitions: Vec<(String, TypeId)> = Vec::new();
        for (container, map) in [
            ("$defs", &schema.defs),
            ("definitions", &schema.definitions),
        ] {
            let Some(map) = map else { continue };
            for (key, def) in map {
                let Schema::Object(def) = def else { continue };
                let pointer: String = pointer_push(&pointer_push("", container), key);
                let conjunct: Conjunct<'r> = Conjunct {
                    schema: def,
                    scope: self.registry.enter(root_scope, def),
                    pointer: pointer.clone(),
                    skip: 0,
                };
                let id: TypeId = self.request(
                    Request {
                        conjuncts: vec![conjunct],
                        mask: all_types(),
                    },
                    &Context {
                        schema_index: self.schema_index,
                        origin: pointer,
                        key_name: None,
                        is_root: false,
                    },
                )?;
                definitions.push((key.clone(), id));
            }
        }
        while let Some((id, flat, mask, context)) = self.queue.pop() {
            let node: TypeNode = self.build_node(&flat, mask, &context)?;
            self.graph.nodes[id.index()] = node;
        }
        Ok(SchemaRoots { root, definitions })
    }

    /// The id for `request`: an existing node with the same conjunction, or a reserved one queued
    /// for building. Records `context`'s name proposals on it.
    fn request(&mut self, request: Request<'r>, context: &Context) -> CodeGenResult<TypeId> {
        let flat: Flat<'r> = self.flatten(request.conjuncts)?;
        let mut keys: Vec<(usize, u8)> = flat.conjuncts.iter().map(Conjunct::key).collect();
        keys.sort_unstable();
        keys.dedup();
        let key: NodeKey = (keys, request.mask, flat.never);
        let id: TypeId = if let Some(&id) = self.memo.get(&(self.schema_index, key.clone())) {
            id
        } else {
            let id: TypeId = self.graph.push(TypeNode::Pending);
            self.memo.insert((self.schema_index, key), id);
            self.add_intrinsic_proposals(id, &flat);
            self.queue.push((id, flat, request.mask, context.clone()));
            id
        };
        self.add_context_proposals(id, context);
        Ok(id)
    }

    /// Proposals from the conjunction itself: the `$defs` key of its first schema, its title.
    fn add_intrinsic_proposals(&mut self, id: TypeId, flat: &Flat<'r>) {
        let Some(first) = flat.conjuncts.first() else {
            return;
        };
        let origin: (usize, String) = (self.schema_index, first.pointer.clone());
        if let Some(name) = definition_name(&first.pointer) {
            self.graph.proposals[id.index()].push(NameProposal {
                tier: NameTier::Definition,
                origin: origin.clone(),
                text: name,
            });
        }
        let title: Option<String> = flat
            .conjuncts
            .iter()
            .filter(|c| !c.skips(skip::ANNOTATIONS))
            .find_map(|c| non_empty(c.schema.title.as_deref()));
        if let Some(title) = title {
            let tier: NameTier = match self.settings.model_name_source {
                ModelNameSource::TitleFirst => NameTier::Preferred,
                ModelNameSource::PropertyKeyFirst => NameTier::Secondary,
            };
            self.graph.proposals[id.index()].push(NameProposal {
                tier,
                origin,
                text: title,
            });
        }
    }

    fn add_context_proposals(&mut self, id: TypeId, context: &Context) {
        let origin: (usize, String) = (context.schema_index, context.origin.clone());
        if let Some(ref key) = context.key_name {
            let tier: NameTier = match self.settings.model_name_source {
                ModelNameSource::TitleFirst => NameTier::Secondary,
                ModelNameSource::PropertyKeyFirst => NameTier::Preferred,
            };
            self.graph.proposals[id.index()].push(NameProposal {
                tier,
                origin: origin.clone(),
                text: key.clone(),
            });
        }
        if context.is_root {
            self.graph.proposals[id.index()].push(NameProposal {
                tier: NameTier::Root,
                origin,
                text: "Root".to_string(),
            });
        }
    }

    /// Expands `$ref` and `allOf` of the seed conjuncts into the full conjunction.
    fn flatten(&self, seeds: Vec<Conjunct<'r>>) -> CodeGenResult<Flat<'r>> {
        let mut out: Vec<Conjunct<'r>> = Vec::new();
        let mut never: bool = false;
        let mut seen: HashSet<(usize, u8)> = HashSet::new();
        let mut stack: Vec<(Conjunct<'r>, bool)> =
            seeds.into_iter().rev().map(|c| (c, true)).collect();
        while let Some((mut c, is_seed)) = stack.pop() {
            if !seen.insert(c.key()) {
                continue;
            }
            let s: &'r JsonSchema = c.schema;
            let mut children: Vec<Conjunct<'r>> = Vec::new();
            let mut had_ref: bool = false;
            let mut drop_self: bool = false;
            if let Some(ref_str) = s.ref_.as_deref()
                && !c.skips(skip::REF)
            {
                had_ref = true;
                let resolved = self.registry.resolve(c.scope, ref_str).map_err(|e| {
                    CodeGenError::RefResolution {
                        ref_str: ref_str.to_string(),
                        reason: e.to_string(),
                    }
                })?;
                match resolved.schema {
                    SchemaRef::Bool(true) => {}
                    SchemaRef::Bool(false) => never = true,
                    SchemaRef::Object(target) => children.push(Conjunct {
                        schema: target,
                        scope: self.registry.enter(resolved.scope, target),
                        pointer: resolved
                            .pointer
                            .iter()
                            .fold(String::new(), |p, t| pointer_push(&p, t)),
                        skip: c.skip & skip::ANNOTATIONS,
                    }),
                }
                if self.registry.dialect(c.scope).ref_siblings == RefSiblings::Ignored {
                    drop_self = true;
                }
                c.skip |= skip::REF;
            }
            if let Some(ref all_of) = s.all_of
                && !c.skips(skip::ALL_OF)
                && !drop_self
            {
                for (i, branch) in all_of.iter().enumerate() {
                    match branch {
                        Schema::Bool(true) => {}
                        Schema::Bool(false) => never = true,
                        Schema::Object(b) => children.push(Conjunct {
                            schema: b,
                            scope: self.registry.enter(c.scope, b),
                            pointer: pointer_push(
                                &pointer_push(&c.pointer, "allOf"),
                                &i.to_string(),
                            ),
                            skip: c.skip & skip::ANNOTATIONS,
                        }),
                    }
                }
                c.skip |= skip::ALL_OF;
            }
            let keep: bool = !drop_self
                && (contributes(s, c.skip)
                    || (is_seed && !had_ref && !c.skips(skip::ANNOTATIONS) && has_annotations(s)));
            if keep {
                out.push(c);
            }
            for child in children.into_iter().rev() {
                stack.push((child, false));
            }
        }
        drop_redundant_type_conjuncts(&mut out);
        Ok(Flat {
            conjuncts: out,
            never,
        })
    }

    /// Builds the node for a reserved id.
    fn build_node(
        &mut self,
        flat: &Flat<'r>,
        mask: TypeSet,
        context: &Context,
    ) -> CodeGenResult<TypeNode> {
        let annotations: Annotations = annotations_of(flat);
        if flat.never {
            return Ok(uninhabited(annotations));
        }
        // A union keyword: one variant per branch, with the other conjuncts distributed in.
        if let Some((owner, kind, branches)) = first_union(flat) {
            return self.build_union(flat, mask, context, owner, kind, branches, annotations);
        }
        let facts: Facts = Facts::of(flat, mask, self.registry, self.conditionals);
        if let Some(values) = facts.enum_values {
            let (nullable, values): (bool, Vec<Value>) = split_null(values);
            if nullable && !values.is_empty() {
                let inner: TypeId =
                    self.request_masked(flat, mask.without(JsonType::Null), context)?;
                return Ok(TypeNode::Option(inner));
            }
            if nullable {
                return Ok(TypeNode::Null);
            }
            return Ok(TypeNode::Enum(super::EnumNode {
                values: sort_values(values),
                annotations,
            }));
        }
        let types: TypeSet = facts.types;
        if types.is_empty() {
            return Ok(uninhabited(annotations));
        }
        if facts.unconstrained {
            return Ok(TypeNode::Any);
        }
        let non_null: TypeSet = types.without(JsonType::Null);
        if types.contains(JsonType::Null) && !non_null.is_empty() {
            let inner: TypeId = self.request_masked(flat, non_null, context)?;
            return Ok(TypeNode::Option(inner));
        }
        if non_null.len() > 1 {
            let mut variants: Vec<Variant> = Vec::new();
            for ty in non_null.iter() {
                let inner: TypeId = self.request_masked(flat, TypeSet::from(ty), context)?;
                variants.push(Variant {
                    ty: inner,
                    title: None,
                    is_ref: false,
                });
            }
            return Ok(TypeNode::Union(UnionNode {
                kind: UnionKind::TypeArray,
                variants,
                annotations,
            }));
        }
        let Some(ty) = types.single() else {
            return Ok(TypeNode::Any);
        };
        let node: TypeNode = match ty {
            JsonType::Null => TypeNode::Null,
            JsonType::Boolean => TypeNode::Boolean,
            JsonType::Integer => match facts.integer_range() {
                (Some(lo), Some(hi)) if lo > hi => uninhabited(annotations),
                _ => TypeNode::Integer(facts.integer_width()),
            },
            JsonType::Number if facts.number_range_is_empty() => uninhabited(annotations),
            JsonType::Number => TypeNode::Number(facts.float_width()),
            JsonType::String => TypeNode::String {
                format: facts.string_format(),
                constraints: facts.string_constraints.clone(),
            },
            JsonType::Array => {
                let items: TypeId = if facts.items.is_empty() {
                    self.any()
                } else {
                    let conjuncts: Vec<Conjunct<'r>> = facts.items.clone();
                    let pointer: String = conjuncts[0].pointer.clone();
                    self.request(
                        Request {
                            conjuncts,
                            mask: all_types(),
                        },
                        &Context {
                            schema_index: self.schema_index,
                            origin: pointer,
                            key_name: context.key_name.clone(),
                            is_root: false,
                        },
                    )?
                };
                TypeNode::Array {
                    items,
                    unique: facts.unique_items,
                    constraints: facts.array_constraints.clone(),
                }
            }
            JsonType::Object => self.build_object(flat, &facts, context, annotations)?,
        };
        Ok(node)
    }

    /// A node for an object: a struct when it has properties or forbids others, else a map.
    fn build_object(
        &mut self,
        flat: &Flat<'r>,
        facts: &Facts<'r>,
        context: &Context,
        annotations: Annotations,
    ) -> CodeGenResult<TypeNode> {
        let mut fields: Vec<Field> = Vec::new();
        for (key, property) in &facts.properties {
            if property.forbidden {
                continue;
            }
            let ty: TypeId = if property.conjuncts.is_empty() {
                self.any()
            } else {
                let pointer: String = property.conjuncts[0].pointer.clone();
                self.request(
                    Request {
                        conjuncts: property.conjuncts.clone(),
                        mask: all_types(),
                    },
                    &Context {
                        schema_index: self.schema_index,
                        origin: pointer,
                        key_name: Some(key.clone()),
                        is_root: false,
                    },
                )?
            };
            fields.push(Field {
                json_key: key.clone(),
                ty,
                required: property.required,
                default: property.default.clone(),
                description: property.description.clone(),
                deprecated: property.deprecated,
            });
        }
        // Keys the schema does not name: kept in a map field unless `additionalProperties` is
        // `false` (or a setting overrides the schema).
        let override_mode: Option<UnknownProperties> = self.settings.unknown_properties;
        let deny_unknown: bool =
            facts.deny_unknown && override_mode != Some(UnknownProperties::Capture);
        let additional: Option<TypeId> = if override_mode == Some(UnknownProperties::Drop) {
            None
        } else if !facts.additional.is_empty() {
            let pointer: String = facts.additional[0].pointer.clone();
            let base: String = base_name(flat, context);
            Some(self.request(
                Request {
                    conjuncts: facts.additional.clone(),
                    mask: all_types(),
                },
                &Context {
                    schema_index: self.schema_index,
                    origin: pointer,
                    key_name: Some(format!("{base}_Value")),
                    is_root: false,
                },
            )?)
        } else if deny_unknown {
            None
        } else {
            Some(self.any())
        };
        if fields.is_empty() && !deny_unknown {
            let values: TypeId = match additional {
                Some(values) => values,
                None => self.any(),
            };
            return Ok(TypeNode::Map { values });
        }
        Ok(TypeNode::Object(ObjectNode {
            fields,
            deny_unknown,
            additional,
            annotations,
        }))
    }

    #[expect(clippy::too_many_arguments, clippy::too_many_lines)]
    fn build_union(
        &mut self,
        flat: &Flat<'r>,
        mask: TypeSet,
        context: &Context,
        owner: usize,
        kind: UnionKind,
        branches: &'r [Schema],
        annotations: Annotations,
    ) -> CodeGenResult<TypeNode> {
        let consumed: u8 = match kind {
            UnionKind::AnyOf => skip::ANY_OF,
            UnionKind::OneOf | UnionKind::TypeArray => skip::ONE_OF,
        };
        // Every other conjunct (and the owner without this keyword) applies to each variant;
        // annotations stay on the union.
        let carried: Vec<Conjunct<'r>> = flat
            .conjuncts
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let mut c: Conjunct<'r> = c.clone();
                if i == owner {
                    c.skip |= consumed;
                }
                c.skip |= skip::ANNOTATIONS | skip::REF | skip::ALL_OF;
                c
            })
            .filter(|c| contributes(c.schema, c.skip))
            .collect();
        let owner_conjunct: &Conjunct<'r> = &flat.conjuncts[owner];
        let keyword: &str = match kind {
            UnionKind::AnyOf => "anyOf",
            UnionKind::OneOf | UnionKind::TypeArray => "oneOf",
        };
        let base_pointer: String = pointer_push(&owner_conjunct.pointer, keyword);
        let mut branch_conjuncts: Vec<Option<Conjunct<'r>>> = Vec::new();
        for (i, branch) in branches.iter().enumerate() {
            branch_conjuncts.push(match branch {
                Schema::Bool(false) => None,
                Schema::Bool(true) => Some(Conjunct {
                    schema: empty_schema(),
                    scope: owner_conjunct.scope,
                    pointer: pointer_push(&base_pointer, &i.to_string()),
                    skip: 0,
                }),
                Schema::Object(b) => Some(Conjunct {
                    schema: b,
                    scope: self.registry.enter(owner_conjunct.scope, b),
                    pointer: pointer_push(&base_pointer, &i.to_string()),
                    skip: 0,
                }),
            });
        }
        let live: Vec<(usize, Conjunct<'r>)> = branch_conjuncts
            .into_iter()
            .enumerate()
            .filter_map(|(i, c)| c.map(|c| (i, c)))
            .collect();
        // Branches that are sets of strings (`enum` / `const`) become one string enum with all
        // their values: the whole union when every branch is one, else one variant in place of
        // the first. Distinct strings never overlap, so this is exact for `oneOf` and `anyOf`.
        let string_values: Vec<Option<Vec<Value>>> = self.branch_string_values(&carried, &live)?;
        if !live.is_empty() && string_values.iter().all(Option::is_some) {
            return Ok(TypeNode::Enum(super::EnumNode {
                values: sort_values(dedup_values(
                    string_values.into_iter().flatten().flatten().collect(),
                )),
                annotations,
            }));
        }
        let string_branches: usize = string_values.iter().filter(|v| v.is_some()).count();
        let all_strings: Vec<Value> = string_values.iter().flatten().flatten().cloned().collect();
        // `anyOf` / `oneOf` of one type and `null`: an optional value.
        if live.len() == 2 {
            let nulls: Vec<bool> = live
                .iter()
                .map(|(_, c)| self.is_null_only(c))
                .collect::<CodeGenResult<Vec<bool>>>()?;
            if nulls[0] != nulls[1] {
                let (_, value) = &live[usize::from(nulls[0])];
                let mut conjuncts: Vec<Conjunct<'r>> = carried.clone();
                conjuncts.push(value.clone());
                let inner: TypeId = self.request(
                    Request { conjuncts, mask },
                    &Context {
                        schema_index: self.schema_index,
                        origin: value.pointer.clone(),
                        key_name: context.key_name.clone(),
                        is_root: false,
                    },
                )?;
                return Ok(TypeNode::Option(inner));
            }
        }
        let base_name: String = base_name(flat, context);
        let mut variants: Vec<Variant> = Vec::new();
        let mut combined_done: bool = false;
        for ((i, branch), strings) in live.into_iter().zip(string_values) {
            if string_branches >= 2 && strings.is_some() {
                if combined_done {
                    continue;
                }
                combined_done = true;
                let ty: TypeId = self.graph.push(TypeNode::Enum(super::EnumNode {
                    values: sort_values(dedup_values(all_strings.clone())),
                    annotations: Annotations::default(),
                }));
                self.add_context_proposals(
                    ty,
                    &Context {
                        schema_index: self.schema_index,
                        origin: branch.pointer.clone(),
                        key_name: Some(format!("{base_name}_Variant{i}")),
                        is_root: false,
                    },
                );
                variants.push(Variant {
                    ty,
                    title: None,
                    is_ref: false,
                });
                continue;
            }
            let title: Option<String> = if branch.schema.ref_.is_none() {
                non_empty(branch.schema.title.as_deref())
            } else {
                None
            };
            let origin: String = branch.pointer.clone();
            let is_ref: bool = branch.schema.ref_.is_some();
            let mut conjuncts: Vec<Conjunct<'r>> = carried.clone();
            conjuncts.push(branch);
            let ty: TypeId = self.request(
                Request { conjuncts, mask },
                &Context {
                    schema_index: self.schema_index,
                    origin,
                    key_name: Some(format!("{base_name}_Variant{i}")),
                    is_root: false,
                },
            )?;
            variants.push(Variant { ty, title, is_ref });
        }
        Ok(TypeNode::Union(UnionNode {
            kind,
            variants,
            annotations,
        }))
    }

    /// Per branch, its values when the branch (with the carried conjuncts) is a string `enum` /
    /// `const` and nothing else constrains it.
    fn branch_string_values(
        &self,
        carried: &[Conjunct<'r>],
        live: &[(usize, Conjunct<'r>)],
    ) -> CodeGenResult<Vec<Option<Vec<Value>>>> {
        let mut out: Vec<Option<Vec<Value>>> = Vec::with_capacity(live.len());
        for (_, branch) in live {
            let mut conjuncts: Vec<Conjunct<'r>> = carried.to_vec();
            conjuncts.push(branch.clone());
            let flat: Flat<'r> = self.flatten(conjuncts)?;
            if flat.never || first_union(&flat).is_some() {
                out.push(None);
                continue;
            }
            let facts: Facts<'r> = Facts::of(&flat, all_types(), self.registry, self.conditionals);
            out.push(
                facts
                    .enum_values
                    .filter(|v| !v.is_empty() && v.iter().all(Value::is_string)),
            );
        }
        Ok(out)
    }

    /// Whether the conjunction of `c` alone holds only `null`.
    fn is_null_only(&self, c: &Conjunct<'r>) -> CodeGenResult<bool> {
        let flat: Flat<'r> = self.flatten(vec![c.clone()])?;
        if first_union(&flat).is_some() {
            return Ok(false);
        }
        let facts: Facts<'r> = Facts::of(&flat, all_types(), self.registry, self.conditionals);
        if let Some(ref values) = facts.enum_values {
            return Ok(!values.is_empty() && values.iter().all(Value::is_null));
        }
        Ok(!facts.unconstrained && facts.types == TypeSet::from(JsonType::Null))
    }

    /// The same conjunction restricted to `mask`.
    fn request_masked(
        &mut self,
        flat: &Flat<'r>,
        mask: TypeSet,
        context: &Context,
    ) -> CodeGenResult<TypeId> {
        let origin: String = flat
            .conjuncts
            .first()
            .map(|c| c.pointer.clone())
            .unwrap_or_default();
        let id: TypeId = self.request(
            Request {
                conjuncts: flat.conjuncts.clone(),
                mask,
            },
            &Context {
                schema_index: self.schema_index,
                origin,
                key_name: context.key_name.clone(),
                is_root: context.is_root,
            },
        )?;
        Ok(id)
    }

    /// The `Any` node.
    fn any(&mut self) -> TypeId {
        let key: NodeKey = (Vec::new(), all_types(), false);
        if let Some(&id) = self.memo.get(&(usize::MAX, key.clone())) {
            return id;
        }
        let id: TypeId = self.graph.push(TypeNode::Any);
        self.memo.insert((usize::MAX, key), id);
        id
    }
}

/// An empty schema object, for a `true` branch.
fn empty_schema() -> &'static JsonSchema {
    static EMPTY: std::sync::OnceLock<JsonSchema> = std::sync::OnceLock::new();
    EMPTY.get_or_init(JsonSchema::default)
}

/// An enum with no values: no instance is valid.
fn uninhabited(annotations: Annotations) -> TypeNode {
    TypeNode::Enum(super::EnumNode {
        values: Vec::new(),
        annotations,
    })
}

fn all_types() -> TypeSet {
    JsonType::ALL
        .into_iter()
        .fold(TypeSet::empty(), TypeSet::with)
}

/// The `$defs` / `definitions` key `pointer` names, when it is `/$defs/<key>` or
/// `/definitions/<key>` at any depth.
fn definition_name(pointer: &str) -> Option<String> {
    let mut tokens = pointer.rsplitn(3, '/');
    let key: &str = tokens.next()?;
    let container: &str = tokens.next()?;
    tokens.next()?;
    (container == "$defs" || container == "definitions")
        .then(|| key.replace("~1", "/").replace("~0", "~"))
}

/// A pattern matching where both `a` and `b` match (`pattern` is a search, not anchored, so
/// each is a lookahead over the whole string).
fn both_patterns(a: &str, b: &str) -> String {
    format!("^(?=[\\s\\S]*?(?:{a}))(?=[\\s\\S]*?(?:{b}))")
}

/// The text a node's derived names (union variants, map values) start from: the key it is
/// reached under, else its `$defs` key, else its title, else `Root`.
fn base_name(flat: &Flat<'_>, context: &Context) -> String {
    context
        .key_name
        .clone()
        .or_else(|| {
            flat.conjuncts
                .first()
                .and_then(|c| definition_name(&c.pointer))
        })
        .or_else(|| {
            flat.conjuncts
                .iter()
                .filter(|c| !c.skips(skip::ANNOTATIONS))
                .find_map(|c| non_empty(c.schema.title.as_deref()))
        })
        .unwrap_or_else(|| "Root".to_string())
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Whether `s` has a keyword that shapes the type, ignoring the `skip`ped ones.
fn contributes(s: &JsonSchema, skip_bits: u8) -> bool {
    s.type_.is_some() || contributes_besides_type(s, skip_bits)
}

/// [`contributes`], not counting `type`.
fn contributes_besides_type(s: &JsonSchema, skip_bits: u8) -> bool {
    let skipped = |bit: u8| skip_bits & bit != 0;
    !s.properties.is_empty()
        || s.required.as_ref().is_some_and(|r| !r.is_empty())
        || s.additional_properties.is_some()
        || s.items.is_some()
        || s.enum_values.is_some()
        || s.const_value.is_some()
        || s.minimum.is_some()
        || s.maximum.is_some()
        || s.exclusive_minimum.is_some()
        || s.exclusive_maximum.is_some()
        || s.min_length.is_some()
        || s.max_length.is_some()
        || s.pattern.is_some()
        || s.format.is_some()
        || s.unique_items.is_some()
        || s.min_items.is_some()
        || s.max_items.is_some()
        || (s.any_of.is_some() && !skipped(skip::ANY_OF))
        || (s.one_of.is_some() && !skipped(skip::ONE_OF))
        || (s.ref_.is_some() && !skipped(skip::REF))
        || (s.all_of.is_some() && !skipped(skip::ALL_OF))
        || s.extensions.contains_key("then")
        || s.extensions.contains_key("else")
}

fn has_annotations(s: &JsonSchema) -> bool {
    s.title.is_some()
        || s.description.is_some()
        || s.comment.is_some()
        || s.examples.is_some()
        || s.deprecated.is_some()
        || s.id.is_some()
}

/// Whether `s` contributes only `type` (given `skip_bits`).
fn contributes_only_type(s: &JsonSchema, skip_bits: u8) -> bool {
    s.type_.is_some() && !contributes_besides_type(s, skip_bits)
}

/// Drops carried conjuncts (annotations skipped) that only restate `type` already implied by
/// the others, so `{"type": "object", "oneOf": [{"$ref": "#/$defs/A"}]}`'s variant is `A` itself.
fn drop_redundant_type_conjuncts(conjuncts: &mut Vec<Conjunct<'_>>) {
    let candidates: Vec<usize> = conjuncts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.skips(skip::ANNOTATIONS) && contributes_only_type(c.schema, c.skip))
        .map(|(i, _)| i)
        .collect();
    let mut removed: BTreeSet<usize> = BTreeSet::new();
    for i in candidates {
        let Some(own) = conjuncts[i].schema.type_ else {
            continue;
        };
        let others: Option<TypeSet> = conjuncts
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i && !removed.contains(j))
            .filter_map(|(_, c)| c.schema.type_)
            .reduce(intersect_types);
        if let Some(others) = others
            && others.iter().all(|t| covers(own, t))
        {
            removed.insert(i);
        }
    }
    let mut index: usize = 0;
    conjuncts.retain(|_| {
        let keep: bool = !removed.contains(&index);
        index += 1;
        keep
    });
}

/// Whether the type set `set` admits every instance of type `t` (`number` covers `integer`).
fn covers(set: TypeSet, t: JsonType) -> bool {
    set.contains(t) || (t == JsonType::Integer && set.contains(JsonType::Number))
}

/// The types both sets admit: `number` ∩ `integer` is `integer`.
fn intersect_types(a: TypeSet, b: TypeSet) -> TypeSet {
    JsonType::ALL
        .into_iter()
        .filter(|&t| covers(a, t) && covers(b, t))
        .filter(|&t| {
            // `integer` survives only when one side names it (not when both only say `number`).
            t != JsonType::Integer || a.contains(JsonType::Integer) || b.contains(JsonType::Integer)
        })
        .fold(TypeSet::empty(), TypeSet::with)
}

/// `types` restricted to `mask`: each masked type the set admits (`number` admits
/// `integer`). The full mask leaves `types` as written.
fn apply_mask(types: TypeSet, mask: TypeSet) -> TypeSet {
    if mask == all_types() {
        return types;
    }
    mask.iter()
        .filter(|&t| covers(types, t))
        .fold(TypeSet::empty(), TypeSet::with)
}

/// The first `anyOf` / `oneOf` keyword in the conjunction: (conjunct index, kind, branches).
fn first_union<'r>(flat: &Flat<'r>) -> Option<(usize, UnionKind, &'r [Schema])> {
    flat.conjuncts.iter().enumerate().find_map(|(i, c)| {
        let s: &'r JsonSchema = c.schema;
        if let Some(ref any_of) = s.any_of
            && !c.skips(skip::ANY_OF)
        {
            return Some((i, UnionKind::AnyOf, any_of.as_slice()));
        }
        if let Some(ref one_of) = s.one_of
            && !c.skips(skip::ONE_OF)
        {
            return Some((i, UnionKind::OneOf, one_of.as_slice()));
        }
        None
    })
}

/// The first non-empty value of each annotation, in conjunction order.
fn annotations_of(flat: &Flat<'_>) -> Annotations {
    let mut a: Annotations = Annotations::default();
    for c in flat
        .conjuncts
        .iter()
        .filter(|c| !c.skips(skip::ANNOTATIONS))
    {
        let s: &JsonSchema = c.schema;
        if a.title.is_none() {
            a.title = non_empty(s.title.as_deref());
        }
        if a.description.is_none() {
            a.description = non_empty(s.description.as_deref());
        }
        if a.comment.is_none() {
            a.comment.clone_from(&s.comment);
        }
        if a.examples.is_none() {
            a.examples.clone_from(&s.examples);
        }
        if a.deprecated.is_none() {
            a.deprecated = s.deprecated;
        }
        if a.id.is_none() {
            a.id.clone_from(&s.id);
        }
    }
    a
}

/// One property of an object node, merged across the conjunction.
#[derive(Debug, Clone, Default)]
struct PropertyFacts<'r> {
    conjuncts: Vec<Conjunct<'r>>,
    /// A `false` property schema: the property may not appear.
    forbidden: bool,
    required: bool,
    default: Option<Value>,
    description: Option<String>,
    deprecated: Option<bool>,
}

/// What a conjunction says, keyword by keyword.
#[derive(Debug, Clone)]
struct Facts<'r> {
    types: TypeSet,
    /// No keyword constrains the instance at all: any value.
    unconstrained: bool,
    enum_values: Option<Vec<Value>>,
    lower: Option<serde_json::Number>,
    exclusive_lower: Option<serde_json::Number>,
    upper: Option<serde_json::Number>,
    exclusive_upper: Option<serde_json::Number>,
    format: Option<String>,
    string_constraints: StringConstraints,
    items: Vec<Conjunct<'r>>,
    unique_items: bool,
    array_constraints: ArrayConstraints,
    properties: BTreeMap<String, PropertyFacts<'r>>,
    additional: Vec<Conjunct<'r>>,
    deny_unknown: bool,
}

impl<'r> Facts<'r> {
    #[expect(clippy::too_many_lines)]
    fn of(
        flat: &Flat<'r>,
        mask: TypeSet,
        registry: &Registry<'_>,
        conditionals: &'r Conditionals,
    ) -> Facts<'r> {
        let mut explicit: Option<TypeSet> = None;
        let mut enum_values: Option<Vec<Value>> = None;
        let mut facts: Facts<'r> = Facts {
            types: all_types(),
            unconstrained: false,
            enum_values: None,
            lower: None,
            exclusive_lower: None,
            upper: None,
            exclusive_upper: None,
            format: None,
            string_constraints: StringConstraints::default(),
            items: Vec::new(),
            unique_items: false,
            array_constraints: ArrayConstraints::default(),
            properties: BTreeMap::new(),
            additional: Vec::new(),
            deny_unknown: false,
        };
        let mut inferred: TypeSet = TypeSet::empty();
        for c in &flat.conjuncts {
            let s: &'r JsonSchema = c.schema;
            if let Some(t) = s.type_ {
                explicit = Some(explicit.map_or(t, |e| intersect_types(e, t)));
            }
            for values in [
                s.enum_values.clone(),
                s.const_value.clone().map(|v| vec![v]),
            ] {
                let Some(values) = values else { continue };
                enum_values = Some(match enum_values {
                    None => dedup_values(values),
                    Some(existing) => existing
                        .into_iter()
                        .filter(|v| values.iter().any(|w| number::json_equal(v, w)))
                        .collect(),
                });
            }
            facts.lower = tighter(facts.lower.take(), s.minimum.clone(), Ordering::Greater);
            facts.exclusive_lower = tighter(
                facts.exclusive_lower.take(),
                s.exclusive_minimum.clone(),
                Ordering::Greater,
            );
            facts.upper = tighter(facts.upper.take(), s.maximum.clone(), Ordering::Less);
            facts.exclusive_upper = tighter(
                facts.exclusive_upper.take(),
                s.exclusive_maximum.clone(),
                Ordering::Less,
            );
            if s.minimum.is_some()
                || s.maximum.is_some()
                || s.exclusive_minimum.is_some()
                || s.exclusive_maximum.is_some()
                || s.multiple_of.is_some()
            {
                inferred.insert(JsonType::Number);
            }
            if facts.format.is_none() {
                facts.format.clone_from(&s.format);
            }
            let sc: &mut StringConstraints = &mut facts.string_constraints;
            sc.min_length = max_opt(sc.min_length, s.min_length);
            sc.max_length = min_opt(sc.max_length, s.max_length);
            sc.pattern = match (sc.pattern.take(), &s.pattern) {
                (None, p) => p.clone(),
                (Some(a), Some(b)) if a != *b => Some(both_patterns(&a, b)),
                (a, _) => a,
            };
            if s.min_length.is_some() || s.max_length.is_some() || s.pattern.is_some() {
                inferred.insert(JsonType::String);
            }
            if let Some(Schema::Object(items)) = &s.items {
                facts.items.push(Conjunct {
                    schema: items,
                    scope: c.scope,
                    pointer: pointer_push(&c.pointer, "items"),
                    skip: 0,
                });
            }
            facts.unique_items |= s.unique_items == Some(true);
            let ac: &mut ArrayConstraints = &mut facts.array_constraints;
            ac.min_items = max_opt(ac.min_items, s.min_items);
            ac.max_items = min_opt(ac.max_items, s.max_items);
            if s.items.is_some()
                || s.min_items.is_some()
                || s.max_items.is_some()
                || s.unique_items.is_some()
            {
                inferred.insert(JsonType::Array);
            }
            let required: &[String] = s.required.as_deref().unwrap_or(&[]);
            for (key, property) in &s.properties {
                let entry: &mut PropertyFacts<'r> =
                    facts.properties.entry(key.clone()).or_default();
                match property {
                    Schema::Bool(true) => {}
                    Schema::Bool(false) => entry.forbidden = true,
                    Schema::Object(p) => {
                        if entry.default.is_none() {
                            entry.default.clone_from(&p.default_value);
                        }
                        if entry.description.is_none() {
                            entry.description = non_empty(p.description.as_deref());
                        }
                        if entry.deprecated.is_none() {
                            entry.deprecated = p.deprecated;
                        }
                        entry.conjuncts.push(Conjunct {
                            schema: p,
                            scope: c.scope,
                            pointer: pointer_push(&pointer_push(&c.pointer, "properties"), key),
                            skip: 0,
                        });
                    }
                }
            }
            for key in required {
                if let Some(entry) = facts.properties.get_mut(key) {
                    entry.required = true;
                }
            }
            match &s.additional_properties {
                None | Some(Schema::Bool(true)) => {}
                Some(Schema::Bool(false)) => facts.deny_unknown = true,
                Some(Schema::Object(a)) => facts.additional.push(Conjunct {
                    schema: a,
                    scope: c.scope,
                    pointer: pointer_push(&c.pointer, "additionalProperties"),
                    skip: 0,
                }),
            }
            if !s.properties.is_empty() || s.required.is_some() || s.additional_properties.is_some()
            {
                inferred.insert(JsonType::Object);
            }
        }
        // `required` names a property declared only in another conjunct: mark it there too.
        for c in &flat.conjuncts {
            for key in c.schema.required.as_deref().unwrap_or(&[]) {
                if let Some(entry) = facts.properties.get_mut(key) {
                    entry.required = true;
                }
            }
        }
        // `then` / `else` hold conditionally, so the properties they add are optional fields: the
        // type accepts every valid instance and keeps every field. A key both define takes the
        // union of the two; a key the base defines keeps the base's type.
        for c in &flat.conjuncts {
            if registry.dialect(c.scope).keyword("then").is_none() {
                continue;
            }
            let s: &'r JsonSchema = c.schema;
            let branches: Vec<(&str, &'r JsonSchema)> = ["then", "else"]
                .into_iter()
                .filter_map(|k| conditionals.get(s, k).map(|b| (k, b)))
                .collect();
            for &(keyword, branch) in &branches {
                for (key, property) in &branch.properties {
                    // Defined by the base, or already added from `then`.
                    if facts.properties.contains_key(key) {
                        continue;
                    }
                    let Schema::Object(p) = property else {
                        continue;
                    };
                    let in_both: bool = branches.len() == 2
                        && branches.iter().all(|(_, b)| b.properties.contains_key(key));
                    let (schema, pointer): (&'r JsonSchema, String) = if in_both {
                        let Some(merged) = conditionals.merged(s, key) else {
                            continue;
                        };
                        (
                            merged,
                            pointer_push(&pointer_push(&c.pointer, "then|else"), key),
                        )
                    } else {
                        (
                            p,
                            pointer_push(
                                &pointer_push(&pointer_push(&c.pointer, keyword), "properties"),
                                key,
                            ),
                        )
                    };
                    let entry: &mut PropertyFacts<'r> =
                        facts.properties.entry(key.clone()).or_default();
                    entry.description = non_empty(p.description.as_deref());
                    entry.deprecated = p.deprecated;
                    entry.conjuncts.push(Conjunct {
                        schema,
                        scope: c.scope,
                        pointer,
                        skip: 0,
                    });
                }
            }
        }
        let has_format: bool = facts.format.is_some();
        let base: TypeSet = match explicit {
            Some(t) => t,
            None if !inferred.is_empty() => inferred,
            None if has_format => TypeSet::from(JsonType::String),
            None => all_types(),
        };
        facts.types = apply_mask(base, mask);
        facts.unconstrained = explicit.is_none()
            && inferred.is_empty()
            && !has_format
            && enum_values.is_none()
            && mask == all_types();
        if let Some(values) = enum_values {
            let types: TypeSet = facts.types;
            facts.enum_values = Some(
                values
                    .into_iter()
                    .filter(|v| value_matches(types, v))
                    .collect(),
            );
        }
        facts
    }

    /// The integer range the bounds allow: (lowest, highest), each `None` when unbounded.
    fn integer_range(&self) -> (Option<i128>, Option<i128>) {
        let lower: Option<i128> = tighter_i128(
            self.lower
                .as_ref()
                .and_then(|n| number::integer_lower_bound(n, false)),
            self.exclusive_lower
                .as_ref()
                .and_then(|n| number::integer_lower_bound(n, true)),
            Ordering::Greater,
        );
        let upper: Option<i128> = tighter_i128(
            self.upper
                .as_ref()
                .and_then(|n| number::integer_upper_bound(n, false)),
            self.exclusive_upper
                .as_ref()
                .and_then(|n| number::integer_upper_bound(n, true)),
            Ordering::Less,
        );
        (lower, upper)
    }

    /// Whether no number satisfies the bounds (lower above upper, or equal with either
    /// exclusive).
    fn number_range_is_empty(&self) -> bool {
        let inclusive: [(&Option<serde_json::Number>, bool); 2] =
            [(&self.lower, false), (&self.exclusive_lower, true)];
        let upper: [(&Option<serde_json::Number>, bool); 2] =
            [(&self.upper, false), (&self.exclusive_upper, true)];
        inclusive.iter().any(|(lo, lo_ex)| {
            upper.iter().any(|(hi, hi_ex)| match (lo, hi) {
                (Some(lo), Some(hi)) => match number::cmp_num(lo, hi) {
                    Ordering::Greater => true,
                    Ordering::Equal => *lo_ex || *hi_ex,
                    Ordering::Less => false,
                },
                _ => false,
            })
        })
    }

    fn integer_width(&self) -> IntWidth {
        match self.integer_range() {
            (Some(lo), Some(hi)) => integer_width_for_range(lo, hi).unwrap_or(IntWidth::I64),
            _ => IntWidth::I64,
        }
    }

    fn float_width(&self) -> FloatWidth {
        let lower: Option<&serde_json::Number> = [&self.lower, &self.exclusive_lower]
            .into_iter()
            .flatten()
            .max_by(|a, b| number::cmp_num(a, b));
        let upper: Option<&serde_json::Number> = [&self.upper, &self.exclusive_upper]
            .into_iter()
            .flatten()
            .min_by(|a, b| number::cmp_num(a, b));
        if let (Some(lo), Some(hi)) = (lower, upper)
            && let (Some(f32_min), Some(f32_max)) = (
                serde_json::Number::from_f64(f64::from(f32::MIN)),
                serde_json::Number::from_f64(f64::from(f32::MAX)),
            )
            && number::cmp_num(lo, hi) != Ordering::Greater
            && number::cmp_num(lo, &f32_min) != Ordering::Less
            && number::cmp_num(hi, &f32_max) != Ordering::Greater
        {
            return FloatWidth::F32;
        }
        FloatWidth::F64
    }

    fn string_format(&self) -> StringFormat {
        match self.format.as_deref() {
            Some("uuid") => StringFormat::Uuid,
            _ => StringFormat::Plain,
        }
    }
}

/// Whether the JSON value `v` has a type in `types` (an integer-valued number matches
/// `integer`).
fn value_matches(types: TypeSet, v: &Value) -> bool {
    let t: JsonType = match v {
        Value::Null => JsonType::Null,
        Value::Bool(_) => JsonType::Boolean,
        Value::Number(n) => {
            if types.contains(JsonType::Number) {
                return true;
            }
            return types.contains(JsonType::Integer)
                && number::is_integer(n, crate::json_schema::IntegerSemantics::Numeric);
        }
        Value::String(_) => JsonType::String,
        Value::Array(_) => JsonType::Array,
        Value::Object(_) => JsonType::Object,
    };
    types.contains(t)
}

/// Splits `null` out of an enum's values: (had null, the rest).
fn split_null(values: Vec<Value>) -> (bool, Vec<Value>) {
    let nullable: bool = values.iter().any(Value::is_null);
    (
        nullable,
        values.into_iter().filter(|v| !v.is_null()).collect(),
    )
}

/// Removes values equal (by JSON Schema equality) to an earlier one.
fn dedup_values(values: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(values.len());
    for v in values {
        if !out.iter().any(|w| number::json_equal(w, &v)) {
            out.push(v);
        }
    }
    out
}

/// Enum values in their fixed order: strings alphabetically when all are strings, otherwise by
/// JSON text.
fn sort_values(mut values: Vec<Value>) -> Vec<Value> {
    if values.iter().all(Value::is_string) {
        values.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    } else {
        values.sort_by_cached_key(Value::to_string);
    }
    values
}

fn tighter(
    a: Option<serde_json::Number>,
    b: Option<serde_json::Number>,
    keep: Ordering,
) -> Option<serde_json::Number> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if number::cmp_num(&a, &b) == keep {
            a
        } else {
            b
        }),
        (a, None) | (None, a) => a,
    }
}

fn tighter_i128(a: Option<i128>, b: Option<i128>, keep: Ordering) -> Option<i128> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if a.cmp(&b) == keep { a } else { b }),
        (a, None) | (None, a) => a,
    }
}

fn max_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, None) | (None, a) => a,
    }
}

fn min_opt(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) | (None, a) => a,
    }
}

/// The narrowest integer width holding every integer in `lo..=hi`, or `None` when the range is
/// empty or no 64-bit type holds it.
pub(crate) fn integer_width_for_range(lo: i128, hi: i128) -> Option<IntWidth> {
    if lo > hi {
        return None;
    }
    let candidates: [(IntWidth, i128, i128); 8] = [
        (IntWidth::U8, 0, i128::from(u8::MAX)),
        (IntWidth::U16, 0, i128::from(u16::MAX)),
        (IntWidth::U32, 0, i128::from(u32::MAX)),
        (IntWidth::U64, 0, i128::from(u64::MAX)),
        (IntWidth::I8, i128::from(i8::MIN), i128::from(i8::MAX)),
        (IntWidth::I16, i128::from(i16::MIN), i128::from(i16::MAX)),
        (IntWidth::I32, i128::from(i32::MIN), i128::from(i32::MAX)),
        (IntWidth::I64, i128::from(i64::MIN), i128::from(i64::MAX)),
    ];
    candidates
        .into_iter()
        .find(|&(_, min, max)| lo >= min && hi <= max)
        .map(|(width, _, _)| width)
}
