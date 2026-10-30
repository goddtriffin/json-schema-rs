//! Reverse codegen: Rust types to JSON Schema.
//!
//! Types implement [`ToJsonSchema`] to produce a [`JsonSchema`] value that can be
//! serialized via [`TryFrom`] to String or `Vec<u8>`.

mod error;
mod resource_uri;

pub use error::{ReverseCodeGenError, ReverseCodeGenResult};

use crate::json_schema::{JsonSchema, JsonType, Schema};
use resource_uri::Placed;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Trait for types that can produce a JSON Schema.
///
/// Implemented for primitive/standard types (e.g. `String`, `Option<T>`) and for
/// structs via `#[derive(ToJsonSchema)]` with optional container/field attributes.
pub trait ToJsonSchema {
    /// Returns the JSON Schema for this type as a standalone root schema.
    ///
    /// # Errors
    ///
    /// Every problem that keeps the types from forming a valid document, one per instance (see
    /// [`SchemaGenerator::into_root_schema`]).
    fn json_schema() -> ReverseCodeGenResult;

    /// Returns the schema for this type as a subschema of the document `generator` is
    /// building.
    ///
    /// Named types (derived structs and enums) return a `$ref` and register their body in the
    /// generator's `$defs`; containers pass the generator through to their element types. The
    /// default inlines [`ToJsonSchema::json_schema`], hoisting any `$defs` it carries into the
    /// generator, so a hand-written impl only needs `json_schema`; its errors are reported to
    /// the generator.
    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        match Self::json_schema() {
            Ok(schema) => generator.inline_subschema(schema),
            Err(errors) => {
                generator.report_errors(errors);
                JsonSchema::default()
            }
        }
    }

    /// Identity of this type in a [`SchemaGenerator`]: two types with the same `schema_id`
    /// share one `$defs` entry.
    ///
    /// The derive returns the type's full path (`module_path!()` plus the type name), so
    /// same-name types from different modules stay distinct.
    #[doc(hidden)]
    #[must_use]
    fn schema_id() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Prefix of the provisional `$ref` the generator returns for a definition until
/// [`SchemaGenerator::into_root_schema`] assigns final `$defs` names. A second `#` is not a
/// valid URI character, so no `$ref` written by hand can start with it.
const PROVISIONAL_REF_PREFIX: &str = "#/$defs/#";

/// Prefix of the provisional `$ref` to a schema resource (a derived type with `$id`), followed
/// by the resource's index. [`SchemaGenerator::into_root_schema`] replaces it with the reference
/// that resolves to the resource from the base URI of the schema it appears in.
const RESOURCE_REF_PREFIX: &str = "#/$id/#";

/// Identity of one `$defs` entry. Entries that share a name are suffixed in this order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum DefinitionIdentity {
    /// A named type, by [`ToJsonSchema::schema_id`].
    Type(String),
    /// A `$defs` entry hoisted from a standalone schema, in the order it was first seen.
    Hoisted(usize),
}

/// One `$defs` entry of the document being built.
#[derive(Debug, Clone)]
struct Definition {
    identity: DefinitionIdentity,
    /// Preferred `$defs` key: the bare type name, or the hoisted key.
    name: String,
    /// `None` while the body is being built, and for a root that nothing refers back to. A
    /// hoisted entry may be a boolean schema.
    body: Option<Schema>,
}

/// A standalone schema already passed to [`SchemaGenerator::inline_subschema`] and the
/// subschema it produced, so inlining the same schema again reuses its definitions.
#[derive(Debug, Clone)]
struct InlinedSchema {
    standalone: JsonSchema,
    inlined: JsonSchema,
}

/// The `$defs` of one schema resource: the document root, or a derived type with `$id` whose
/// body is being built. A `#/$defs/...` `$ref` inside a resource resolves against that
/// resource (2020-12 core §9.2.1), so each resource carries the definitions its refs name.
#[derive(Debug, Default, Clone)]
struct Scope {
    definitions: Vec<Definition>,
    type_definitions: BTreeMap<String, usize>,
    pending: BTreeSet<usize>,
    referenced_while_pending: BTreeSet<usize>,
    inlined: Vec<InlinedSchema>,
    /// [`ToJsonSchema::schema_id`] of the `$id` type this scope is the root of, if any.
    resource: Option<String>,
}

/// A derived type with `$id`.
#[derive(Debug)]
struct Resource {
    schema_id: String,
    /// Preferred `$defs` key.
    name: String,
    id: String,
    /// The body and its scope as built, with refs to other resources still provisional. `None`
    /// while the body is being built, and for the document root (whose scope is the document's).
    built: Option<(JsonSchema, Scope)>,
}

/// One embedded copy of a resource in the finished document, or the document root itself.
#[derive(Debug)]
struct Node {
    /// Index into [`SchemaGenerator::resources`]; `None` for a document root without `$id`.
    resource: Option<usize>,
    /// The node whose `$defs` hold this copy; `None` for the document root.
    parent: Option<usize>,
    /// Its URI under each placeholder document base; `None` when its `$id` is not a URI
    /// reference, so refs to and from it are written as is.
    uri: Option<Placed>,
}

/// How one node refers to another.
#[derive(Debug, Clone)]
enum NodeRef {
    /// From the document root (or a `$defs` entry sharing its base) to a resource embedded in
    /// the root `$defs`: `#/$defs/<Name>`.
    RootDefinition(usize),
    /// A URI reference that resolves to the target from the referrer's base.
    Uri(String),
}

/// Generator context for one root schema: owns the `$defs` entries and the set of named types
/// whose body is being built.
///
/// Entries are keyed by identity ([`ToJsonSchema::schema_id`] for named types), not by name.
/// A named type found in the pending set returns its `$ref` immediately, so a cycle of any
/// length between derived types terminates. Descent depth is bounded by the number of
/// distinct named types (each is built at most once per scope) times the number of distinct
/// `$id` types (each is built at most once per generator), plus the nesting of container types
/// written in the source.
///
/// A derived type with `$id` is its own schema resource ([`SchemaGenerator::resource_subschema`]):
/// its body is built in a child scope whose `$defs` it carries, and it is embedded once, in the
/// document root `$defs`. Inside a resource, a reference to the resource itself is `#` and a
/// reference to another resource is that resource's `$id`.
///
/// `$ref`s returned while building are provisional: [`SchemaGenerator::into_root_schema`]
/// assigns the final `$defs` keys (the bare name when unique, otherwise a numeric suffix on
/// every entry sharing it, ordered by identity) and rewrites them.
#[derive(Debug, Default)]
pub struct SchemaGenerator {
    document: Scope,
    /// Scopes of the `$id` types being built, innermost last.
    resource_scopes: Vec<Scope>,
    /// Every `$id` type seen so far, in the order first seen.
    resources: Vec<Resource>,
    /// Index into `resources` by [`ToJsonSchema::schema_id`].
    resource_indices: BTreeMap<String, usize>,
    /// The resource the document root is, if it has `$id`.
    root_resource: Option<usize>,
    /// Problems reported while building, returned by [`SchemaGenerator::into_root_schema`].
    errors: Vec<ReverseCodeGenError>,
}

impl SchemaGenerator {
    /// Creates a generator with no definitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a `$ref` to the named type `schema_id` (preferred `$defs` key `name`), building
    /// its body with `build` the first time `schema_id` is requested in the current resource.
    ///
    /// When `schema_id` is already being built (a recursive reference), the `$ref` is returned
    /// without calling `build`.
    pub fn named_subschema(
        &mut self,
        schema_id: &str,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        let scope: &mut Scope = self.current_scope();
        let index: usize = if let Some(&index) = scope.type_definitions.get(schema_id) {
            if scope.pending.contains(&index) {
                scope.referenced_while_pending.insert(index);
            }
            index
        } else {
            let index: usize = scope.push_type_definition(schema_id, name);
            scope.pending.insert(index);
            let body: JsonSchema = build(self);
            let scope: &mut Scope = self.current_scope();
            scope.pending.remove(&index);
            scope.definitions[index].body = Some(body.into());
            index
        };
        ref_schema(provisional_ref(index))
    }

    /// Returns a `$ref` to the named type `schema_id` with `$id` `id` (preferred `$defs` key
    /// `name`), building its body with `build` in its own scope the first time `schema_id` is
    /// requested.
    ///
    /// The body carries the `$defs` of the named types it refers to. From inside the resource
    /// itself the `$ref` is `#`; otherwise it is provisional until
    /// [`SchemaGenerator::into_root_schema`] places the resource and writes the reference that
    /// resolves to it from the referrer's base URI.
    pub fn resource_subschema(
        &mut self,
        schema_id: &str,
        name: &str,
        id: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        if self.current_scope().resource.as_deref() == Some(schema_id) {
            return ref_schema("#".to_string());
        }
        if let Some(&index) = self.resource_indices.get(schema_id) {
            return ref_schema(resource_ref(index));
        }

        let index: usize = self.push_resource(schema_id, name, id);
        self.resource_scopes.push(Scope {
            resource: Some(schema_id.to_string()),
            ..Scope::default()
        });
        let body: JsonSchema = build(self);
        let Some(scope) = self.resource_scopes.pop() else {
            unreachable!("`build` returns with the scope stack it was given");
        };
        self.resources[index].built = Some((body, scope));
        ref_schema(resource_ref(index))
    }

    /// Builds the root schema for the named type `schema_id` (preferred `$defs` key `name`)
    /// from `build`.
    ///
    /// The root body is returned inline. It is also placed in `$defs` when a nested type
    /// refers back to it, so that `$ref` resolves.
    ///
    /// # Errors
    ///
    /// See [`SchemaGenerator::into_root_schema`].
    pub fn into_named_root_schema(
        mut self,
        schema_id: &str,
        name: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> ReverseCodeGenResult {
        let index: usize = match self.document.type_definitions.get(schema_id) {
            Some(&index) => index,
            None => self.document.push_type_definition(schema_id, name),
        };
        self.document.pending.insert(index);
        let body: JsonSchema = build(&mut self);
        self.document.pending.remove(&index);
        if self.document.referenced_while_pending.contains(&index) {
            self.document.definitions[index].body = Some(body.clone().into());
        }
        self.into_root_schema(body)
    }

    /// Builds the root schema for the named type `schema_id` with `$id` `id` (preferred `$defs`
    /// key `name`, used if another resource needs its own copy of it) from `build`.
    ///
    /// The document root is the resource, so a nested reference back to it is `#` and it never
    /// appears in its own `$defs`.
    ///
    /// # Errors
    ///
    /// See [`SchemaGenerator::into_root_schema`].
    pub fn into_resource_root_schema(
        mut self,
        schema_id: &str,
        name: &str,
        id: &str,
        build: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> ReverseCodeGenResult {
        self.document.resource = Some(schema_id.to_string());
        self.root_resource = Some(self.push_resource(schema_id, name, id));
        let body: JsonSchema = build(&mut self);
        self.into_root_schema(body)
    }

    /// Records problems found while building the document (for example by a hand-written
    /// [`ToJsonSchema::json_schema_with`] whose element type failed);
    /// [`SchemaGenerator::into_root_schema`] returns them.
    pub fn report_errors(&mut self, errors: impl IntoIterator<Item = ReverseCodeGenError>) {
        self.errors.extend(errors);
    }

    /// Assigns the final `$defs` keys, rewrites every provisional `$ref` in `body` and the
    /// definitions, attaches the `$defs` (if any) to `body` and returns it as the root schema.
    ///
    /// Each name used by one entry stays bare. When several entries share a name, every one
    /// of them gets a numeric suffix (`Name0`, `Name1`, ...) in identity order, skipping
    /// suffixed names that are already taken. A hoisted entry whose name and body equal
    /// another entry's is merged into it first.
    ///
    /// Each resource (derived type with `$id`) is embedded in the document root `$defs`, and
    /// every `$ref` to it is written so that it resolves to it from the referring schema's base
    /// URI (RFC 3986 §5.2): its `$id` as written when that works, else the relative or absolute
    /// reference that does. A resource whose URI depends on the document's own base cannot be
    /// reached from a resource with an absolute `$id` (the document base is unknown), so that
    /// resource carries its own copy in its `$defs`, where the copy resolves against it. Two
    /// types whose `$id`s resolve to the same URI with identical schemas are embedded once.
    ///
    /// # Errors
    ///
    /// Every problem found, one per instance: each error reported with
    /// [`SchemaGenerator::report_errors`], a [`ReverseCodeGenError::InvalidResourceId`] per type
    /// whose `$id` is not a URI reference, and a [`ReverseCodeGenError::DuplicateResourceId`]
    /// per type whose `$id` resolves to the URI of an earlier one with a different schema.
    pub fn into_root_schema(mut self, body: JsonSchema) -> ReverseCodeGenResult {
        let mut errors: Vec<ReverseCodeGenError> = std::mem::take(&mut self.errors);
        let schema: JsonSchema = if self.resources.is_empty() {
            std::mem::take(&mut self.document).into_root_schema(body)
        } else {
            self.into_root_schema_with_resources(body, &mut errors)
        };
        if errors.is_empty() {
            Ok(schema)
        } else {
            Err(errors)
        }
    }

    /// [`SchemaGenerator::into_root_schema`] for a document that embeds resources.
    fn into_root_schema_with_resources(
        mut self,
        body: JsonSchema,
        errors: &mut Vec<ReverseCodeGenError>,
    ) -> JsonSchema {
        let mut body: JsonSchema = body;
        let mut built: Vec<Option<(JsonSchema, Scope)>> = self
            .resources
            .iter_mut()
            .map(|resource| resource.built.take())
            .collect();
        if let Some(root) = self.root_resource {
            built[root] = Some((body.clone(), self.document.clone()));
        }

        let depth: usize = resource_uri::placeholder_depth(
            self.resources.iter().map(|resource| resource.id.as_str()),
            self.resources.len() * 5,
        );
        let bases: Placed = resource_uri::placeholder_bases(depth);
        let root_uri: Option<Placed> = match self.root_resource {
            Some(root) => resource_uri::place(&bases, &self.resources[root].id).ok(),
            None => Some(bases.clone()),
        };
        let mut document: Scope = std::mem::take(&mut self.document);
        self.merge_identical_resources(&mut built, &mut body, &mut document, root_uri.as_ref());

        let mut targets: Vec<BTreeSet<usize>> = built
            .iter_mut()
            .map(|entry| match entry {
                Some((body, scope)) => resource_refs(body, scope),
                None => BTreeSet::new(),
            })
            .collect();
        let document_targets: BTreeSet<usize> = resource_refs(&mut body, &mut document);
        if let Some(root) = self.root_resource {
            targets[root].clone_from(&document_targets);
        }

        let (nodes, references): (Vec<Node>, BTreeMap<(usize, usize), NodeRef>) =
            self.layout(&targets, &document_targets, root_uri);
        errors.extend(self.resource_errors(&nodes, &bases));

        let mut finished: Vec<Option<JsonSchema>> = vec![None; nodes.len()];
        for node_index in (1..nodes.len()).rev() {
            let Some(resource) = nodes[node_index].resource else {
                unreachable!("only the document root has no resource");
            };
            let is_last_copy: bool = !nodes[1..node_index]
                .iter()
                .any(|node| node.resource == Some(resource));
            let (mut node_body, mut scope): (JsonSchema, Scope) = if is_last_copy {
                built[resource].take()
            } else {
                built[resource].clone()
            }
            .unwrap_or_else(|| unreachable!("every embedded resource was built"));
            let resolve = |reference: &str| -> Option<String> {
                let target: usize = parse_resource_ref(reference)?;
                match references.get(&(node_index, target))? {
                    NodeRef::Uri(uri) => Some(uri.clone()),
                    NodeRef::RootDefinition(_) => {
                        unreachable!("only the document root refers by `$defs` entry")
                    }
                }
            };
            scope.rewrite_refs(&mut node_body, &resolve);
            for (child_index, child) in nodes.iter().enumerate() {
                if child.parent == Some(node_index) {
                    let finished_child: Option<JsonSchema> = finished[child_index].take();
                    self.push_resource_definition(&mut scope, child, finished_child);
                }
            }
            finished[node_index] = Some(scope.into_root_schema(node_body));
        }

        let mut root_definitions: BTreeMap<usize, usize> = BTreeMap::new();
        for (child_index, child) in nodes.iter().enumerate() {
            if child.parent == Some(0) {
                root_definitions.insert(child_index, document.definitions.len());
                let finished_child: Option<JsonSchema> = finished[child_index].take();
                self.push_resource_definition(&mut document, child, finished_child);
            }
        }
        let resolve = |reference: &str| -> Option<String> {
            let target: usize = parse_resource_ref(reference)?;
            match references.get(&(0, target))? {
                NodeRef::Uri(uri) => Some(uri.clone()),
                NodeRef::RootDefinition(node) => Some(provisional_ref(root_definitions[node])),
            }
        };
        document.rewrite_refs(&mut body, &resolve);
        document.into_root_schema(body)
    }

    /// Merges each resource into an earlier one (the document root first) whose `$id` resolves
    /// to the same URI and whose schema is identical, rewriting the refs to it. Repeats until
    /// nothing merges, since a merge can make two referring resources identical.
    fn merge_identical_resources(
        &self,
        built: &mut [Option<(JsonSchema, Scope)>],
        body: &mut JsonSchema,
        document: &mut Scope,
        root_uri: Option<&Placed>,
    ) {
        let Some(root_uri) = root_uri else {
            return;
        };
        let uris: Vec<Option<Placed>> = self
            .resources
            .iter()
            .map(|resource| resource_uri::place(root_uri, &resource.id).ok())
            .collect();
        let identical = |a: &(JsonSchema, Scope), b: &(JsonSchema, Scope)| -> bool {
            let entries = |scope: &Scope| -> Vec<(String, Option<Schema>)> {
                scope
                    .definitions
                    .iter()
                    .map(|definition| (definition.name.clone(), definition.body.clone()))
                    .collect()
            };
            a.0 == b.0 && entries(&a.1) == entries(&b.1)
        };
        loop {
            let mut order: Vec<usize> = (0..self.resources.len()).collect();
            if let Some(root) = self.root_resource {
                order.retain(|&index| index != root);
                order.insert(0, root);
            }
            let merge: Option<(usize, usize)> = order.iter().enumerate().find_map(|(i, &kept)| {
                order[i + 1..].iter().find_map(|&duplicate| {
                    let same_uri: bool = uris[kept].is_some() && uris[kept] == uris[duplicate];
                    let is_identical: bool = match (&built[kept], &built[duplicate]) {
                        (Some(a), Some(b)) => identical(a, b),
                        _ => false,
                    };
                    (same_uri && is_identical).then_some((duplicate, kept))
                })
            });
            let Some((duplicate, kept)) = merge else {
                return;
            };
            built[duplicate] = None;
            let resolve = |reference: &str| -> Option<String> {
                (parse_resource_ref(reference)? == duplicate).then(|| resource_ref(kept))
            };
            for (entry_body, scope) in built.iter_mut().flatten() {
                scope.rewrite_refs(entry_body, &resolve);
            }
            document.rewrite_refs(body, &resolve);
        }
    }

    /// An [`ReverseCodeGenError::InvalidResourceId`] per resource whose `$id` is not a URI
    /// reference, then a [`ReverseCodeGenError::DuplicateResourceId`] per resource with a copy
    /// at the URI of an earlier resource's copy (identical ones were merged).
    fn resource_errors(&self, nodes: &[Node], bases: &Placed) -> Vec<ReverseCodeGenError> {
        let mut errors: Vec<ReverseCodeGenError> = Vec::new();
        for resource in &self.resources {
            if let Err(reason) = resource_uri::place(bases, &resource.id) {
                errors.push(ReverseCodeGenError::InvalidResourceId {
                    schema_id: resource.schema_id.clone(),
                    id: resource.id.clone(),
                    reason,
                });
            }
        }
        let mut reported: BTreeSet<(usize, usize)> = BTreeSet::new();
        for (index, node) in nodes.iter().enumerate() {
            let (Some(resource), Some(uri)) = (node.resource, node.uri.as_ref()) else {
                continue;
            };
            let first: Option<usize> = nodes[..index].iter().find_map(|earlier| {
                let earlier_resource: usize = earlier.resource?;
                (earlier_resource != resource && earlier.uri.as_ref() == Some(uri))
                    .then_some(earlier_resource)
            });
            if let Some(first) = first
                && reported.insert((first, resource))
            {
                errors.push(ReverseCodeGenError::DuplicateResourceId {
                    first: self.resources[first].schema_id.clone(),
                    first_id: self.resources[first].id.clone(),
                    second: self.resources[resource].schema_id.clone(),
                    second_id: self.resources[resource].id.clone(),
                });
            }
        }
        errors
    }

    /// Places every resource the document reaches, and decides each `$ref` from a node to a
    /// resource. Node 0 is the document root.
    ///
    /// Nodes are visited breadth-first from the root. A `$ref` from node `n` to resource `t`
    /// goes to the first existing copy of `t` that some reference reaches from `n`'s base;
    /// else to a new copy in the root `$defs` if one is reachable from `n`; else to a new copy
    /// in `n`'s own `$defs` (its `$id` as written then resolves against `n`).
    ///
    /// This terminates: a new copy is made only when every existing copy of `t` depends on more
    /// of the document base than `n` does, and the new copy depends on no more than `n`, so each
    /// resource gets at most one copy per way of depending on the base (none, scheme, scheme
    /// and authority, directory, file).
    fn layout(
        &self,
        targets: &[BTreeSet<usize>],
        document_targets: &BTreeSet<usize>,
        root_uri: Option<Placed>,
    ) -> (Vec<Node>, BTreeMap<(usize, usize), NodeRef>) {
        let mut nodes: Vec<Node> = vec![Node {
            resource: self.root_resource,
            parent: None,
            uri: root_uri,
        }];
        let mut references: BTreeMap<(usize, usize), NodeRef> = BTreeMap::new();
        let mut queue: VecDeque<usize> = VecDeque::from([0]);
        while let Some(from) = queue.pop_front() {
            let from_targets: &BTreeSet<usize> = match nodes[from].resource {
                Some(resource) if from != 0 => &targets[resource],
                _ => document_targets,
            };
            for &target in from_targets {
                if nodes[from].resource == Some(target) {
                    continue;
                }
                let existing: Option<NodeRef> = nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, node)| node.resource == Some(target))
                    .find_map(|(to, _)| self.node_ref(&nodes, from, to));
                let node_ref: NodeRef = if let Some(node_ref) = existing {
                    node_ref
                } else {
                    let has_root_copy: bool = nodes
                        .iter()
                        .any(|node| node.resource == Some(target) && node.parent == Some(0));
                    let mut node_ref: Option<NodeRef> = None;
                    for parent in [0, from] {
                        if parent == 0 && has_root_copy {
                            continue;
                        }
                        nodes.push(Node {
                            resource: Some(target),
                            parent: Some(parent),
                            uri: nodes[parent].uri.as_ref().and_then(|uri| {
                                resource_uri::place(uri, &self.resources[target].id).ok()
                            }),
                        });
                        let to: usize = nodes.len() - 1;
                        node_ref = self.node_ref(&nodes, from, to);
                        if node_ref.is_some() {
                            queue.push_back(to);
                            break;
                        }
                        nodes.pop();
                    }
                    node_ref.unwrap_or_else(|| {
                        unreachable!("a copy in the referrer's own `$defs` resolves from it")
                    })
                };
                references.insert((from, target), node_ref);
            }
        }
        (nodes, references)
    }

    /// The reference from node `from` to node `to`, if one resolves from `from`'s base.
    fn node_ref(&self, nodes: &[Node], from: usize, to: usize) -> Option<NodeRef> {
        if from == 0 && nodes[to].parent == Some(0) {
            return Some(NodeRef::RootDefinition(to));
        }
        let id: &str = match nodes[to].resource {
            Some(resource) => &self.resources[resource].id,
            None => {
                unreachable!("only the document root has no resource, and nothing refers to it")
            }
        };
        match (&nodes[from].uri, &nodes[to].uri) {
            (Some(from_uri), Some(to_uri)) => {
                resource_uri::reference(from_uri, to_uri, id).map(NodeRef::Uri)
            }
            _ => Some(NodeRef::Uri(id.to_string())),
        }
    }

    /// Adds the finished copy `body` of `node`'s resource to `scope`'s `$defs`.
    fn push_resource_definition(&self, scope: &mut Scope, node: &Node, body: Option<JsonSchema>) {
        let Some(resource) = node.resource.map(|index| &self.resources[index]) else {
            unreachable!("only the document root has no resource");
        };
        let index: usize = scope.push_type_definition(&resource.schema_id, &resource.name);
        scope.definitions[index].body = body.map(Schema::from);
    }

    fn push_resource(&mut self, schema_id: &str, name: &str, id: &str) -> usize {
        let index: usize = self.resources.len();
        self.resources.push(Resource {
            schema_id: schema_id.to_string(),
            name: name.to_string(),
            id: id.to_string(),
            built: None,
        });
        self.resource_indices.insert(schema_id.to_string(), index);
        index
    }

    /// Embeds a standalone schema as a subschema: its `$defs` (at any depth) become entries of
    /// the current resource's `$defs`, its `$ref`s to them are rewritten, and its `$schema` is
    /// dropped (`$schema` must not appear in a subschema that is not a resource root).
    ///
    /// A `$ref` is matched to a hoisted entry by its JSON Pointer (`#/$defs/Outer/$defs/Inner`).
    /// A `#/$defs/Inner` with no top-level `Inner` resolves to the one nested `Inner` when
    /// exactly one exists. Inlining an equal standalone schema again reuses its entries.
    ///
    /// A schema with `$id` is its own resource and is embedded whole, `$schema` and `$defs`
    /// included. So is a `$defs` entry with `$id`, and no `$ref` inside a resource is rewritten,
    /// since it resolves against that resource.
    pub fn inline_subschema(&mut self, schema: JsonSchema) -> JsonSchema {
        self.current_scope().inline_subschema(schema)
    }

    fn current_scope(&mut self) -> &mut Scope {
        self.resource_scopes
            .last_mut()
            .unwrap_or(&mut self.document)
    }
}

impl Scope {
    /// Rewrites the provisional resource refs in `body` and in this scope's definitions.
    fn rewrite_refs(&mut self, body: &mut JsonSchema, resolve: &impl Fn(&str) -> Option<String>) {
        rewrite_refs(body, resolve);
        for definition in &mut self.definitions {
            if let Some(object) = definition.body.as_mut().and_then(Schema::as_object_mut) {
                rewrite_refs(object, resolve);
            }
        }
    }

    fn push_type_definition(&mut self, schema_id: &str, name: &str) -> usize {
        let index: usize = self.definitions.len();
        self.definitions.push(Definition {
            identity: DefinitionIdentity::Type(schema_id.to_string()),
            name: name.to_string(),
            body: None,
        });
        self.type_definitions.insert(schema_id.to_string(), index);
        index
    }

    /// See [`SchemaGenerator::into_root_schema`].
    fn into_root_schema(self, mut body: JsonSchema) -> JsonSchema {
        let mut definitions: Vec<Definition> = self.definitions;
        let mut canonical: Vec<usize> = (0..definitions.len()).collect();
        merge_equal_hoisted_definitions(&mut definitions, &mut canonical, &mut body);

        let final_names: Vec<Option<String>> = assign_final_names(&definitions, &canonical);
        let resolve = |reference: &str| -> Option<String> {
            let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
            let name: &str = final_names[index].as_deref()?;
            Some(format!("#/$defs/{}{rest}", escape_pointer_token(name)))
        };

        rewrite_refs(&mut body, &resolve);
        let mut defs: BTreeMap<String, Schema> = BTreeMap::new();
        for (index, definition) in definitions.into_iter().enumerate() {
            if let (Some(name), Some(mut definition_body)) =
                (final_names[index].clone(), definition.body)
            {
                if let Some(object) = definition_body.as_object_mut() {
                    rewrite_refs(object, &resolve);
                }
                defs.insert(name, definition_body);
            }
        }
        if !defs.is_empty() {
            body.defs.get_or_insert_with(BTreeMap::new).extend(defs);
        }
        body
    }

    /// See [`SchemaGenerator::inline_subschema`].
    fn inline_subschema(&mut self, mut schema: JsonSchema) -> JsonSchema {
        if schema.id.is_some() {
            return schema;
        }
        let has_defs: bool = schema.defs.as_ref().is_some_and(|defs| !defs.is_empty());
        if !has_defs {
            schema.schema = None;
            return schema;
        }
        if let Some(previous) = self
            .inlined
            .iter()
            .find(|previous| previous.standalone == schema)
        {
            return previous.inlined.clone();
        }

        let standalone: JsonSchema = schema.clone();
        let (mut flat, hoisted): (JsonSchema, Vec<HoistedDefinition>) = flatten_defs(schema);
        let mut by_pointer: BTreeMap<String, String> = BTreeMap::new();
        let mut nested_by_key: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut indices: Vec<usize> = Vec::with_capacity(hoisted.len());
        for definition in &hoisted {
            let index: usize = self.definitions.len();
            self.definitions.push(Definition {
                identity: DefinitionIdentity::Hoisted(index),
                name: definition.key.clone(),
                body: None,
            });
            indices.push(index);
            by_pointer.insert(definition.pointer.clone(), provisional_ref(index));
            nested_by_key
                .entry(definition.key.clone())
                .or_default()
                .push(definition.pointer.clone());
        }
        for (key, pointers) in nested_by_key {
            let top_level: String = format!("#/$defs/{}", escape_pointer_token(&key));
            if let [pointer] = pointers.as_slice()
                && !by_pointer.contains_key(&top_level)
            {
                let target: String = by_pointer[pointer].clone();
                by_pointer.insert(top_level, target);
            }
        }

        let resolve = |reference: &str| -> Option<String> {
            by_pointer
                .iter()
                .filter(|(pointer, _)| {
                    reference
                        .strip_prefix(pointer.as_str())
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
                })
                .max_by_key(|(pointer, _)| pointer.len())
                .map(|(pointer, target)| format!("{target}{}", &reference[pointer.len()..]))
        };
        rewrite_refs_outside_resources(&mut flat, &resolve);
        for (definition, index) in hoisted.into_iter().zip(indices) {
            let mut body: Schema = definition.body;
            if let Some(object) = body.as_object_mut() {
                rewrite_refs_outside_resources(object, &resolve);
            }
            self.definitions[index].body = Some(body);
        }

        flat.schema = None;
        self.inlined.push(InlinedSchema {
            standalone,
            inlined: flat.clone(),
        });
        flat
    }
}

/// Returns `T`'s schema as a standalone root schema built with a fresh [`SchemaGenerator`]:
/// the subschema from [`ToJsonSchema::json_schema_with`] plus any `$defs` it registered.
///
/// # Errors
///
/// See [`SchemaGenerator::into_root_schema`].
pub fn root_schema_for<T: ToJsonSchema + ?Sized>() -> ReverseCodeGenResult {
    let mut generator: SchemaGenerator = SchemaGenerator::new();
    let body: JsonSchema = T::json_schema_with(&mut generator);
    generator.into_root_schema(body)
}

fn resource_ref(index: usize) -> String {
    format!("{RESOURCE_REF_PREFIX}{index}")
}

fn parse_resource_ref(reference: &str) -> Option<usize> {
    reference.strip_prefix(RESOURCE_REF_PREFIX)?.parse().ok()
}

/// The resources that the provisional refs in `body` and `scope`'s definitions name.
fn resource_refs(body: &mut JsonSchema, scope: &mut Scope) -> BTreeSet<usize> {
    let mut found: BTreeSet<usize> = BTreeSet::new();
    let mut stack: Vec<&mut JsonSchema> = vec![body];
    stack.extend(
        scope
            .definitions
            .iter_mut()
            .filter_map(|definition| definition.body.as_mut().and_then(Schema::as_object_mut)),
    );
    while let Some(current) = stack.pop() {
        if let Some(index) = current.ref_.as_deref().and_then(parse_resource_ref) {
            found.insert(index);
        }
        stack.extend(current.subschemas_mut());
    }
    found
}

fn provisional_ref(index: usize) -> String {
    format!("{PROVISIONAL_REF_PREFIX}{index}")
}

/// Splits a provisional `$ref` into its definition index and the pointer after it.
fn parse_provisional_ref(reference: &str) -> Option<(usize, &str)> {
    let rest: &str = reference.strip_prefix(PROVISIONAL_REF_PREFIX)?;
    let digits: usize = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let index: usize = rest[..digits].parse().ok()?;
    Some((index, &rest[digits..]))
}

/// Escapes a `$defs` key as a JSON Pointer reference token (RFC 6901 §3).
fn escape_pointer_token(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn ref_schema(reference: String) -> JsonSchema {
    JsonSchema {
        ref_: Some(reference),
        ..Default::default()
    }
}

/// Replaces every `$ref` in `schema` (at any depth) for which `resolve` returns a new value.
fn rewrite_refs(schema: &mut JsonSchema, resolve: &impl Fn(&str) -> Option<String>) {
    let mut stack: Vec<&mut JsonSchema> = vec![schema];
    while let Some(current) = stack.pop() {
        if let Some(reference) = current.ref_.as_deref()
            && let Some(resolved) = resolve(reference)
        {
            current.ref_ = Some(resolved);
        }
        stack.extend(current.subschemas_mut());
    }
}

/// Like [`rewrite_refs`], but leaves every subschema with `$id` (and everything inside it)
/// untouched: its `$ref`s resolve against that resource, not the schema being rewritten.
fn rewrite_refs_outside_resources(
    schema: &mut JsonSchema,
    resolve: &impl Fn(&str) -> Option<String>,
) {
    let mut stack: Vec<&mut JsonSchema> = vec![schema];
    while let Some(current) = stack.pop() {
        if current.id.is_some() {
            continue;
        }
        if let Some(reference) = current.ref_.as_deref()
            && let Some(resolved) = resolve(reference)
        {
            current.ref_ = Some(resolved);
        }
        stack.extend(current.subschemas_mut());
    }
}

/// A `$defs` entry taken out of a standalone schema by [`flatten_defs`].
#[derive(Debug)]
struct HoistedDefinition {
    /// JSON Pointer of the entry in the standalone schema (`#/$defs/Outer/$defs/Inner`).
    pointer: String,
    key: String,
    body: Schema,
}

/// Removes the `$defs` of `schema` and, transitively, of every `$defs` entry that is not a
/// resource (has no `$id`; a resource keeps its own `$defs`). Returns the schema without them
/// and the entries ordered by pointer.
fn flatten_defs(schema: JsonSchema) -> (JsonSchema, Vec<HoistedDefinition>) {
    let mut root: JsonSchema = schema;
    let mut hoisted: Vec<HoistedDefinition> = Vec::new();
    let mut stack: Vec<(String, BTreeMap<String, Schema>)> = Vec::new();
    if let Some(defs) = root.defs.take() {
        stack.push(("#".to_string(), defs));
    }
    while let Some((parent, defs)) = stack.pop() {
        for (key, mut body) in defs {
            let pointer: String = format!("{parent}/$defs/{}", escape_pointer_token(&key));
            if let Some(object) = body.as_object_mut()
                && object.id.is_none()
                && let Some(nested) = object.defs.take()
            {
                stack.push((pointer.clone(), nested));
            }
            hoisted.push(HoistedDefinition { pointer, key, body });
        }
    }
    hoisted.sort_by(|a, b| a.pointer.cmp(&b.pointer));
    (root, hoisted)
}

/// Merges each hoisted definition into another live definition with the same name and an
/// equal body (preferring a named type, then the earliest entry), rewriting its `$ref`s.
/// Repeats until nothing merges, since a merge can make two referring bodies equal.
fn merge_equal_hoisted_definitions(
    definitions: &mut [Definition],
    canonical: &mut [usize],
    body: &mut JsonSchema,
) {
    let has_hoisted: bool = definitions
        .iter()
        .any(|definition| matches!(definition.identity, DefinitionIdentity::Hoisted(_)));
    if !has_hoisted {
        return;
    }
    loop {
        let mut merge: Option<(usize, usize)> = None;
        'search: for duplicate in 0..definitions.len() {
            let is_hoisted: bool = matches!(
                definitions[duplicate].identity,
                DefinitionIdentity::Hoisted(_)
            );
            if !is_hoisted || canonical[duplicate] != duplicate {
                continue;
            }
            let Some(duplicate_body) = definitions[duplicate].body.as_ref() else {
                continue;
            };
            for target in 0..definitions.len() {
                let is_preferred: bool = match definitions[target].identity {
                    DefinitionIdentity::Type(_) => true,
                    DefinitionIdentity::Hoisted(_) => target < duplicate,
                };
                if target != duplicate
                    && is_preferred
                    && canonical[target] == target
                    && definitions[target].name == definitions[duplicate].name
                    && definitions[target].body.as_ref() == Some(duplicate_body)
                {
                    merge = Some((duplicate, target));
                    break 'search;
                }
            }
        }
        let Some((duplicate, target)) = merge else {
            return;
        };
        canonical[duplicate] = target;
        definitions[duplicate].body = None;
        let resolve = |reference: &str| -> Option<String> {
            let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
            (index == duplicate).then(|| format!("{}{rest}", provisional_ref(target)))
        };
        rewrite_refs(body, &resolve);
        for definition in definitions.iter_mut() {
            if let Some(definition_body) = definition.body.as_mut().and_then(Schema::as_object_mut)
            {
                rewrite_refs(definition_body, &resolve);
            }
        }
    }
}

/// Final `$defs` key of each live definition (one with a body that was not merged away).
fn assign_final_names(definitions: &[Definition], canonical: &[usize]) -> Vec<Option<String>> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, definition) in definitions.iter().enumerate() {
        if canonical[index] == index && definition.body.is_some() {
            groups.entry(&definition.name).or_default().push(index);
        }
    }
    let mut final_names: Vec<Option<String>> = vec![None; definitions.len()];
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (name, members) in &groups {
        if let [index] = members.as_slice() {
            taken.insert((*name).to_string());
            final_names[*index] = Some((*name).to_string());
        }
    }
    for (name, members) in &mut groups {
        if members.len() < 2 {
            continue;
        }
        members.sort_by(|a, b| definitions[*a].identity.cmp(&definitions[*b].identity));
        let mut suffix: usize = 0;
        for index in members.iter() {
            let mut candidate: String = format!("{name}{suffix}");
            while taken.contains(&candidate) {
                suffix += 1;
                candidate = format!("{name}{suffix}");
            }
            suffix += 1;
            taken.insert(candidate.clone());
            final_names[*index] = Some(candidate);
        }
    }
    final_names
}

impl ToJsonSchema for String {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        })
    }
}

/// Any JSON value: the empty schema `{}`, which accepts every instance. Codegen emits
/// `serde_json::Value` for a schema with no single Rust type (e.g. `"items": true`).
impl ToJsonSchema for serde_json::Value {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(JsonSchema::default())
    }
}

impl ToJsonSchema for bool {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        })
    }
}

/// An integer schema bounded by the Rust type's range, written exactly (`u64::MAX` is
/// `18446744073709551615`, not a rounded float).
fn integer_schema_with_bounds(
    min: impl Into<serde_json::Number>,
    max: impl Into<serde_json::Number>,
) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Integer.into()),
        minimum: Some(min.into()),
        maximum: Some(max.into()),
        ..Default::default()
    }
}

impl ToJsonSchema for i8 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(i8::MIN, i8::MAX))
    }
}

impl ToJsonSchema for u8 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(u8::MIN, u8::MAX))
    }
}

impl ToJsonSchema for i16 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(i16::MIN, i16::MAX))
    }
}

impl ToJsonSchema for u16 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(u16::MIN, u16::MAX))
    }
}

impl ToJsonSchema for i32 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(i32::MIN, i32::MAX))
    }
}

impl ToJsonSchema for u32 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(u32::MIN, u32::MAX))
    }
}

impl ToJsonSchema for i64 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(i64::MIN, i64::MAX))
    }
}

impl ToJsonSchema for u64 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(integer_schema_with_bounds(u64::MIN, u64::MAX))
    }
}

/// A number schema bounded by the Rust type's finite range.
fn number_schema_with_bounds(min: f64, max: f64) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Number.into()),
        minimum: serde_json::Number::from_f64(min),
        maximum: serde_json::Number::from_f64(max),
        ..Default::default()
    }
}

impl ToJsonSchema for f32 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(number_schema_with_bounds(
            f64::from(f32::MIN),
            f64::from(f32::MAX),
        ))
    }
}

impl ToJsonSchema for f64 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(number_schema_with_bounds(f64::MIN, f64::MAX))
    }
}

/// `null`: the JSON form of `()`, which forward codegen emits for a `null` branch of a union.
impl ToJsonSchema for () {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(null_schema())
    }
}

/// The schema `{"type": "null"}`.
fn null_schema() -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Null.into()),
        ..Default::default()
    }
}

/// `schema` widened to also accept `null`.
///
/// A schema that only constrains by `type` (no `$ref`, `enum`, `const`, `allOf`, `anyOf` or
/// `oneOf`, any of which would still reject `null`) gets `null` added to its type set. The empty
/// schema already accepts `null` and is returned unchanged. Any other schema becomes
/// `{"anyOf": [schema, {"type": "null"}]}`.
fn nullable(schema: JsonSchema) -> JsonSchema {
    if schema == JsonSchema::default() {
        return schema;
    }
    let only_typed: bool = schema.ref_.is_none()
        && schema.enum_values.is_none()
        && schema.const_value.is_none()
        && schema.all_of.is_none()
        && schema.any_of.is_none()
        && schema.one_of.is_none();
    match schema.type_ {
        Some(types) if only_typed => JsonSchema {
            type_: Some(types.with(JsonType::Null)),
            ..schema
        },
        _ => JsonSchema {
            any_of: Some(vec![schema.into(), null_schema().into()]),
            ..Default::default()
        },
    }
}

/// A nullable `T`: `T`'s schema widened to accept `null` (see [`nullable`]).
///
/// The derive strips one `Option` from a struct field to mean "not required", so this impl is
/// reached where `Option` means `null` is allowed: nested (`Vec<Option<T>>`, an
/// `Option<Option<T>>` field) or on a field marked `#[json_schema(required)]`.
impl<T: ToJsonSchema> ToJsonSchema for Option<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        nullable(T::json_schema_with(generator))
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Vec<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(T::json_schema_with(generator).into()),
            ..Default::default()
        }
    }
}

#[expect(clippy::implicit_hasher)]
impl<T: ToJsonSchema + std::hash::Hash + Eq> ToJsonSchema for std::collections::HashSet<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(T::json_schema_with(generator).into()),
            unique_items: Some(true),
            ..Default::default()
        }
    }
}

impl<V: ToJsonSchema> ToJsonSchema for BTreeMap<String, V> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            additional_properties: Some(V::json_schema_with(generator).into()),
            ..Default::default()
        }
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Box<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>()
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        T::json_schema_with(generator)
    }
}

/// Minimal hand-written struct implementing [`ToJsonSchema`] (used to validate trait shape).
#[derive(Debug, Clone)]
pub struct HandWrittenExample;

impl ToJsonSchema for HandWrittenExample {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(JsonSchema {
            type_: Some(JsonType::Object.into()),
            title: Some("HandWrittenExample".to_string()),
            ..Default::default()
        })
    }
}

#[cfg(feature = "uuid")]
impl ToJsonSchema for uuid::Uuid {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(JsonSchema {
            type_: Some(JsonType::String.into()),
            format: Some("uuid".to_string()),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ReverseCodeGenError, ReverseCodeGenResult, SchemaGenerator, ToJsonSchema, root_schema_for,
    };
    use crate::json_schema::{JsonSchema, JsonType, Schema, TypeSet};
    use std::collections::BTreeMap;

    /// `result` with the schema serialized to JSON text.
    fn serialized(result: ReverseCodeGenResult) -> Result<String, Vec<ReverseCodeGenError>> {
        result.map(|schema: JsonSchema| serde_json::to_string(&schema).expect("serialize"))
    }

    #[test]
    fn string_json_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = String::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn bool_json_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = bool::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn serde_json_value_json_schema_is_empty_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema::default());
        let actual: ReverseCodeGenResult = serde_json::Value::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_of_serde_json_value_serializes_items_as_empty_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"array","items":{}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Vec::<serde_json::Value>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_string_json_schema_adds_null_to_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":["null","string"]}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<String>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_option_string_json_schema_adds_null_once() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":["null","string"]}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<Option<String>>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_serde_json_value_json_schema_stays_empty_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema::default());
        let actual: ReverseCodeGenResult = Option::<serde_json::Value>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unit_json_schema_is_null_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"null"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(<()>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_option_string_json_schema_has_nullable_items() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"array","items":{"type":["null","string"]}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Vec::<Option<String>>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_of_ref_json_schema_is_any_of_with_null() {
        let expected: JsonSchema = JsonSchema {
            any_of: Some(vec![ref_to("Named").into(), null_type_schema().into()]),
            ..Default::default()
        };
        let actual: JsonSchema = super::nullable(ref_to("Named"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_of_enum_json_schema_is_any_of_with_null() {
        let enum_schema: JsonSchema = JsonSchema {
            type_: Some(JsonType::String.into()),
            enum_values: Some(vec![serde_json::json!("a"), serde_json::json!("b")]),
            ..Default::default()
        };
        let expected: JsonSchema = JsonSchema {
            any_of: Some(vec![enum_schema.clone().into(), null_type_schema().into()]),
            ..Default::default()
        };
        let actual: JsonSchema = super::nullable(enum_schema);
        assert_eq!(expected, actual);
    }

    fn null_type_schema() -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Null.into()),
            ..Default::default()
        }
    }

    #[test]
    fn i64_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(
            r#"{"type":"integer","minimum":-9223372036854775808,"maximum":9223372036854775807}"#
                .to_string(),
        );
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(i64::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_i64_json_schema_adds_null_to_type() {
        let expected: ReverseCodeGenResult =
            i64::json_schema().map(|schema: JsonSchema| JsonSchema {
                type_: Some(TypeSet::from(JsonType::Integer).with(JsonType::Null)),
                ..schema
            });
        let actual: ReverseCodeGenResult = Option::<i64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn i32_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":-2147483648,"maximum":2147483647}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(i32::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn u32_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":0,"maximum":4294967295}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(u32::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn u64_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":0,"maximum":18446744073709551615}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(u64::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn i8_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":-128,"maximum":127}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(i8::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn u8_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":0,"maximum":255}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(u8::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn i16_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":-32768,"maximum":32767}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(i16::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn u16_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":0,"maximum":65535}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(u16::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn f64_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(
            r#"{"type":"number","minimum":-1.7976931348623157e+308,"maximum":1.7976931348623157e+308}"#
                .to_string(),
        );
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(f64::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_f64_json_schema_adds_null_to_type() {
        let expected: ReverseCodeGenResult =
            f64::json_schema().map(|schema: JsonSchema| JsonSchema {
                type_: Some(TypeSet::from(JsonType::Number).with(JsonType::Null)),
                ..schema
            });
        let actual: ReverseCodeGenResult = Option::<f64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn f32_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(
            r#"{"type":"number","minimum":-3.4028234663852886e+38,"maximum":3.4028234663852886e+38}"#
                .to_string(),
        );
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(f32::json_schema());
        assert_eq!(expected, actual);
    }

    fn hand_written_example_schema() -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            title: Some("HandWrittenExample".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn hand_written_example_json_schema() {
        let expected: ReverseCodeGenResult = Ok(hand_written_example_schema());
        let actual: ReverseCodeGenResult = super::HandWrittenExample::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_string_json_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(string_schema().into()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = Vec::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_i64_json_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(super::integer_schema_with_bounds(i64::MIN, i64::MAX).into()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = Vec::<i64>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_vec_string_json_schema_adds_null_to_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":["array","null"],"items":{"type":"string"}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<Vec<String>>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn hash_set_string_json_schema_has_unique_items_true() {
        use std::collections::HashSet;
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(string_schema().into()),
            unique_items: Some(true),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = HashSet::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn btree_map_json_schema_serializes_value_schema_as_additional_properties() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"object","additionalProperties":{"type":"string"}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(BTreeMap::<String, String>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_string_json_schema_has_unique_items_none() {
        let expected: Result<Option<bool>, Vec<ReverseCodeGenError>> = Ok(None);
        let actual: Result<Option<bool>, Vec<ReverseCodeGenError>> =
            Vec::<String>::json_schema().map(|schema: JsonSchema| schema.unique_items);
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn uuid_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"uuid"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(uuid::Uuid::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn option_uuid_json_schema_adds_null_to_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":["null","string"],"format":"uuid"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<uuid::Uuid>::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn vec_uuid_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"array","items":{"type":"string","format":"uuid"}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Vec::<uuid::Uuid>::json_schema());
        assert_eq!(expected, actual);
    }

    fn ref_to(name: &str) -> JsonSchema {
        JsonSchema {
            ref_: Some(format!("#/$defs/{name}")),
            ..Default::default()
        }
    }

    fn string_schema() -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::String.into()),
            ..Default::default()
        }
    }

    fn integer_schema() -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Integer.into()),
            ..Default::default()
        }
    }

    fn object_schema(properties: &[(&str, JsonSchema)]) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Object.into()),
            properties: properties
                .iter()
                .map(|(key, schema)| ((*key).to_string(), schema.clone().into()))
                .collect(),
            ..Default::default()
        }
    }

    fn defs(entries: &[(&str, JsonSchema)]) -> BTreeMap<String, Schema> {
        entries
            .iter()
            .map(|(key, schema)| ((*key).to_string(), schema.clone().into()))
            .collect()
    }

    /// A standalone schema `{"$ref": "#/$defs/<name>", "$defs": {"<name>": body}}`, as a
    /// hand-written `json_schema()` might return.
    fn standalone_with_def(name: &str, body: JsonSchema) -> JsonSchema {
        JsonSchema {
            defs: Some(defs(&[(name, body)])),
            ..ref_to(name)
        }
    }

    #[test]
    fn named_subschema_returns_ref_and_registers_body() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema =
            generator.named_subschema("a::Name", "Name", |_| string_schema());
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Name", string_schema())])),
            ..ref_to("Name")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_each_schema_id_once() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let mut builds: u32 = 0;
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let expected: u32 = 1;
        let actual: u32 = builds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_same_name_with_different_schema_ids_separately() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let mut builds: u32 = 0;
        let _: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
            builds += 1;
            string_schema()
        });
        let _: JsonSchema = generator.named_subschema("b::Name", "Name", |_| {
            builds += 1;
            integer_schema()
        });
        let expected: u32 = 2;
        let actual: u32 = builds;
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_returns_ref_for_pending_schema_id_without_building() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema =
            generator.named_subschema("a::Node", "Node", |generator: &mut SchemaGenerator| {
                JsonSchema {
                    items: Some(
                        generator
                            .named_subschema("a::Node", "Node", |_| {
                                panic!("a pending schema id must not be rebuilt")
                            })
                            .into(),
                    ),
                    ..Default::default()
                }
            });
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[(
                "Node",
                JsonSchema {
                    items: Some(ref_to("Node").into()),
                    ..Default::default()
                },
            )])),
            ..ref_to("Node")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn same_name_different_schema_ids_are_all_suffixed_in_schema_id_order() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("Name0", string_schema()),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Name0")), ("second", ref_to("Name1"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn suffixed_names_skip_a_name_already_taken() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let taken: JsonSchema = generator.named_subschema("c::Name0", "Name0", |_| JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        });
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let body: JsonSchema =
            object_schema(&[("taken", taken), ("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Name0",
                    JsonSchema {
                        type_: Some(JsonType::Boolean.into()),
                        ..Default::default()
                    },
                ),
                ("Name1", string_schema()),
                ("Name2", integer_schema()),
            ])),
            ..object_schema(&[
                ("taken", ref_to("Name0")),
                ("first", ref_to("Name1")),
                ("second", ref_to("Name2")),
            ])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_without_back_reference_has_no_defs() {
        let expected: ReverseCodeGenResult = Ok(string_schema());
        let actual: ReverseCodeGenResult =
            SchemaGenerator::new().into_named_root_schema("a::Name", "Name", |_| string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_with_back_reference_also_defines_root() {
        let body: JsonSchema = JsonSchema {
            items: Some(ref_to("Node").into()),
            ..Default::default()
        };
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Node", body.clone())])),
            ..body
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_named_root_schema(
            "a::Node",
            "Node",
            |generator: &mut SchemaGenerator| JsonSchema {
                items: Some(
                    generator
                        .named_subschema("a::Node", "Node", |_| {
                            panic!("a pending schema id must not be rebuilt")
                        })
                        .into(),
                ),
                ..Default::default()
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_root_name_shared_with_nested_type_keeps_nested_unsuffixed() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Name", integer_schema())])),
            ..object_schema(&[("nested", ref_to("Name"))])
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_named_root_schema(
            "a::Name",
            "Name",
            |generator: &mut SchemaGenerator| {
                object_schema(&[(
                    "nested",
                    generator.named_subschema("b::Name", "Name", |_| integer_schema()),
                )])
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_back_referenced_root_shares_name_with_nested_type() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Name0",
                    object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))]),
                ),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))])
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_named_root_schema(
            "a::Name",
            "Name",
            |generator: &mut SchemaGenerator| {
                let nested: JsonSchema =
                    generator.named_subschema("b::Name", "Name", |_| integer_schema());
                let this: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
                    panic!("a pending schema id must not be rebuilt")
                });
                object_schema(&[("nested", nested), ("this", this)])
            },
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_defs_and_drops_schema_keyword() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            ..standalone_with_def("Inner", string_schema())
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(standalone_with_def("Inner", string_schema()));
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_boolean_def() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([("Never".to_string(), Schema::Bool(false))])),
            ..ref_to("Never")
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r##"{"$ref":"#/$defs/Never","$defs":{"Never":false}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(inlined));
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_keeps_schema_keyword_on_resource_root() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            id: Some("http://example.com/inner".to_string()),
            ..string_schema()
        };
        let expected: JsonSchema = standalone.clone();
        let actual: JsonSchema = SchemaGenerator::new().inline_subschema(standalone);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_embeds_a_resource_with_defs_whole() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            id: Some("https://example.com/hand".to_string()),
            defs: Some(defs(&[("Str", string_schema())])),
            ..object_schema(&[("x", ref_to("Str"))])
        };
        let expected: ReverseCodeGenResult = Ok(standalone.clone());
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_leaves_a_resource_inside_the_standalone_schema_untouched() {
        let resource: JsonSchema = JsonSchema {
            id: Some("https://example.com/resource".to_string()),
            defs: Some(defs(&[("S", string_schema())])),
            ..ref_to("S")
        };
        let resource_def: JsonSchema = JsonSchema {
            id: Some("https://example.com/resource-def".to_string()),
            defs: Some(defs(&[("S", string_schema())])),
            ..ref_to("S")
        };
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("S", integer_schema()),
                ("R", resource_def.clone()),
            ])),
            ..object_schema(&[
                ("n", ref_to("S")),
                ("def", ref_to("R")),
                ("resource", resource.clone()),
            ])
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let named: JsonSchema = generator.named_subschema("a::S", "S", |_| string_schema());
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let actual: ReverseCodeGenResult =
            generator.into_root_schema(object_schema(&[("named", named), ("inlined", inlined)]));
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("S0", string_schema()),
                ("S1", integer_schema()),
                ("R", resource_def),
            ])),
            ..object_schema(&[
                ("named", ref_to("S0")),
                (
                    "inlined",
                    object_schema(&[
                        ("n", ref_to("S1")),
                        ("def", ref_to("R")),
                        ("resource", resource),
                    ]),
                ),
            ])
        });
        assert_eq!(expected, actual);
    }

    fn resource_ref(id: &str) -> JsonSchema {
        JsonSchema {
            ref_: Some(id.to_string()),
            ..Default::default()
        }
    }

    fn array_of(items: JsonSchema) -> JsonSchema {
        JsonSchema {
            items: Some(items.into()),
            ..Default::default()
        }
    }

    #[test]
    fn resource_subschema_refers_to_itself_as_hash() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.resource_subschema(
            "a::Node",
            "Node",
            "https://example.com/node",
            |generator: &mut SchemaGenerator| {
                array_of(generator.resource_subschema(
                    "a::Node",
                    "Node",
                    "https://example.com/node",
                    |_| panic!("a pending resource must not be rebuilt"),
                ))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Node", array_of(resource_ref("#")))])),
            ..ref_to("Node")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_keeps_named_dependencies_in_its_own_defs() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.resource_subschema(
            "a::Resource",
            "Resource",
            "https://example.com/resource",
            |generator: &mut SchemaGenerator| {
                array_of(generator.named_subschema("a::Leaf", "Leaf", |_| string_schema()))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[(
                "Resource",
                JsonSchema {
                    defs: Some(defs(&[("Leaf", string_schema())])),
                    ..array_of(ref_to("Leaf"))
                },
            )])),
            ..ref_to("Resource")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_inside_a_resource_is_embedded_at_the_root_and_referred_to_by_id() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.resource_subschema(
            "a::A",
            "A",
            "https://example.com/a",
            |generator: &mut SchemaGenerator| {
                array_of(
                    generator.resource_subschema("a::B", "B", "https://example.com/b", |_| {
                        string_schema()
                    }),
                )
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", string_schema()),
            ])),
            ..ref_to("A")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_refers_to_a_pending_outer_resource_by_its_id() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let reference: JsonSchema = generator.resource_subschema(
            "a::A",
            "A",
            "https://example.com/a",
            |generator: &mut SchemaGenerator| {
                array_of(generator.resource_subschema(
                    "a::B",
                    "B",
                    "https://example.com/b",
                    |generator: &mut SchemaGenerator| {
                        array_of(generator.resource_subschema(
                            "a::A",
                            "A",
                            "https://example.com/a",
                            |_| panic!("a pending resource must not be rebuilt"),
                        ))
                    },
                ))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", array_of(resource_ref("https://example.com/a"))),
            ])),
            ..ref_to("A")
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_built_inside_a_resource_is_referred_to_from_the_document_by_its_defs_entry()
     {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let via: JsonSchema = generator.resource_subschema(
            "a::A",
            "A",
            "https://example.com/a",
            |generator: &mut SchemaGenerator| {
                array_of(
                    generator.resource_subschema("a::B", "B", "https://example.com/b", |_| {
                        string_schema()
                    }),
                )
            },
        );
        let direct: JsonSchema =
            generator.resource_subschema("a::B", "B", "https://example.com/b", |_| {
                panic!("a built resource must not be rebuilt")
            });
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", string_schema()),
            ])),
            ..object_schema(&[("via", ref_to("A")), ("direct", ref_to("B"))])
        });
        let actual: ReverseCodeGenResult =
            generator.into_root_schema(object_schema(&[("via", via), ("direct", direct)]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_resource_root_schema_refers_to_itself_as_hash_and_stays_out_of_defs() {
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_resource_root_schema(
            "a::Root",
            "Root",
            "https://example.com/root",
            |generator: &mut SchemaGenerator| {
                array_of(generator.named_subschema(
                    "a::Child",
                    "Child",
                    |generator: &mut SchemaGenerator| {
                        array_of(generator.resource_subschema(
                            "a::Root",
                            "Root",
                            "https://example.com/root",
                            |_| panic!("the pending root must not be rebuilt"),
                        ))
                    },
                ))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Child", array_of(resource_ref("#")))])),
            ..array_of(ref_to("Child"))
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_flattens_nested_defs_and_resolves_bare_ref_to_unique_nested_def() {
        let outer: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[("b", ref_to("Inner"))])
        };
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Outer", outer)])),
            ..object_schema(&[("a", ref_to("Outer"))])
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("Inner", string_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_resolves_nested_pointer_ref_when_names_conflict() {
        let outer: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", integer_schema())])),
            ..object_schema(&[(
                "b",
                JsonSchema {
                    ref_: Some("#/$defs/Outer/$defs/Inner".to_string()),
                    ..Default::default()
                },
            )])
        };
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", string_schema()), ("Outer", outer)])),
            ..object_schema(&[("a", ref_to("Outer")), ("c", ref_to("Inner"))])
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner1"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer")), ("c", ref_to("Inner0"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_rewrites_ref_into_a_def_by_pointer_prefix() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", object_schema(&[("x", string_schema())]))])),
            ref_: Some("#/$defs/Inner/properties/x".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let _: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| integer_schema());
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("Inner0", integer_schema()),
                ("Inner1", object_schema(&[("x", string_schema())])),
            ])),
            ref_: Some("#/$defs/Inner1/properties/x".to_string()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_escapes_pointer_tokens_in_def_keys() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(inlined);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_conflicting_bodies_are_all_suffixed() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let second: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", integer_schema()));
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Inner0")), ("second", ref_to("Inner1"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_equal_bodies_from_different_schemas_merge() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let second: JsonSchema = generator.inline_subschema(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(ref_to("Inner").into()),
            defs: Some(defs(&[("Inner", string_schema())])),
            ..Default::default()
        });
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[
                ("first", ref_to("Inner")),
                (
                    "second",
                    JsonSchema {
                        type_: Some(JsonType::Array.into()),
                        items: Some(ref_to("Inner").into()),
                        ..Default::default()
                    },
                ),
            ])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_def_equal_to_named_type_merges_into_it() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let named: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| string_schema());
        let hoisted: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let body: JsonSchema = object_schema(&[("hoisted", hoisted), ("named", named)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[("hoisted", ref_to("Inner")), ("named", ref_to("Inner"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_merge_transitively_once_their_references_merge() {
        let outer: JsonSchema = object_schema(&[("inner", ref_to("Inner"))]);
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                ("Inner", string_schema()),
                ("Outer", outer.clone()),
            ])),
            ..ref_to("Outer")
        };
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(JsonSchema {
            title: Some("Second".to_string()),
            ..standalone
        });
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Inner", string_schema()), ("Outer", outer)])),
            ..object_schema(&[
                ("first", ref_to("Outer")),
                (
                    "second",
                    JsonSchema {
                        title: Some("Second".to_string()),
                        ..ref_to("Outer")
                    },
                ),
            ])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inlining_an_equal_recursive_schema_twice_reuses_its_definitions() {
        let standalone: JsonSchema =
            standalone_with_def("List", object_schema(&[("next", ref_to("List"))]));
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(standalone);
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[(
                "List",
                object_schema(&[("next", ref_to("List"))]),
            )])),
            ..object_schema(&[("first", ref_to("List")), ("second", ref_to("List"))])
        });
        let actual: ReverseCodeGenResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_root_schema_without_defs_leaves_defs_none() {
        let expected: ReverseCodeGenResult = Ok(string_schema());
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_root_schema(string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_schema_id_is_the_type_name() {
        let expected: &str = std::any::type_name::<String>();
        let actual: &str = String::schema_id();
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_of_named_type_root_schema_has_defs() {
        struct Named;
        impl ToJsonSchema for Named {
            fn json_schema() -> ReverseCodeGenResult {
                root_schema_for::<Self>()
            }

            fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
                generator.named_subschema("a::Named", "Named", |_| string_schema())
            }
        }
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(ref_to("Named").into()),
            defs: Some(defs(&[("Named", string_schema())])),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = Vec::<Named>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn hand_written_impl_inlines_through_default_json_schema_with() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(hand_written_example_schema().into()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = Vec::<super::HandWrittenExample>::json_schema();
        assert_eq!(expected, actual);
    }

    /// The body of a derived type with `$id`: `schema` carrying that `$id`.
    fn with_id(id: &str, schema: JsonSchema) -> JsonSchema {
        JsonSchema {
            id: Some(id.to_string()),
            ..schema
        }
    }

    /// One derived type with `$id` in a cycle: `(schema_id, name, id, property)`.
    type CycleMember<'a> = (&'a str, &'a str, &'a str, &'a str);

    /// Returns the ref to `members[index]`, building it (once) as an object whose `property`
    /// refers to the next member, so the last member refers back to the first.
    fn cycle_member(
        generator: &mut SchemaGenerator,
        members: &[CycleMember<'_>],
        index: usize,
    ) -> JsonSchema {
        let (schema_id, name, id, property): CycleMember<'_> = members[index];
        generator.resource_subschema(schema_id, name, id, |generator: &mut SchemaGenerator| {
            let next: JsonSchema = cycle_member(generator, members, (index + 1) % members.len());
            with_id(id, object_schema(&[(property, next)]))
        })
    }

    /// The root schema of a document without `$id` whose property `property` refers to the
    /// first of `members`.
    fn cycle_document(property: &str, members: &[CycleMember<'_>]) -> ReverseCodeGenResult {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = cycle_member(&mut generator, members, 0);
        generator.into_root_schema(object_schema(&[(property, first)]))
    }

    /// Why `id` is not a URI reference, as the generator reports it.
    fn invalid_id_reason(id: &str) -> String {
        super::resource_uri::place(&super::resource_uri::placeholder_bases(1), id)
            .expect_err("not a URI reference")
    }

    #[test]
    fn ref_between_absolute_ids_is_the_id_as_written() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Ping",
                    with_id(
                        "https://example.com/schemas/ping.json",
                        object_schema(&[(
                            "pong",
                            resource_ref("https://example.com/schemas/pong.json"),
                        )]),
                    ),
                ),
                (
                    "Pong",
                    with_id(
                        "https://example.com/schemas/pong.json",
                        object_schema(&[(
                            "ping",
                            resource_ref("https://example.com/schemas/ping.json"),
                        )]),
                    ),
                ),
            ])),
            ..object_schema(&[("ping", ref_to("Ping"))])
        });
        let actual: ReverseCodeGenResult = cycle_document(
            "ping",
            &[
                (
                    "a::Ping",
                    "Ping",
                    "https://example.com/schemas/ping.json",
                    "pong",
                ),
                (
                    "a::Pong",
                    "Pong",
                    "https://example.com/schemas/pong.json",
                    "ping",
                ),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_from_a_relative_id_to_an_absolute_id_is_the_id_as_written() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Ping",
                    with_id(
                        "a/ping.json",
                        object_schema(&[("pong", resource_ref("https://example.com/pong"))]),
                    ),
                ),
                ("Pong", with_id("https://example.com/pong", string_schema())),
            ])),
            ..object_schema(&[("ping", ref_to("Ping"))])
        });
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let ping: JsonSchema = generator.resource_subschema(
            "a::Ping",
            "Ping",
            "a/ping.json",
            |generator: &mut SchemaGenerator| {
                let pong: JsonSchema = generator.resource_subschema(
                    "a::Pong",
                    "Pong",
                    "https://example.com/pong",
                    |_| with_id("https://example.com/pong", string_schema()),
                );
                with_id("a/ping.json", object_schema(&[("pong", pong)]))
            },
        );
        let actual: ReverseCodeGenResult =
            generator.into_root_schema(object_schema(&[("ping", ping)]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_between_relative_ids_in_the_same_directory_is_the_id_as_written() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "A",
                    with_id("a.json", object_schema(&[("b", resource_ref("b.json"))])),
                ),
                (
                    "B",
                    with_id("b.json", object_schema(&[("a", resource_ref("a.json"))])),
                ),
            ])),
            ..object_schema(&[("a", ref_to("A"))])
        });
        let actual: ReverseCodeGenResult = cycle_document(
            "a",
            &[("a::A", "A", "a.json", "b"), ("a::B", "B", "b.json", "a")],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_between_relative_ids_in_different_directories_is_the_relative_path() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Ping",
                    with_id(
                        "a/ping.json",
                        object_schema(&[("pong", resource_ref("../pong.json"))]),
                    ),
                ),
                (
                    "Pong",
                    with_id(
                        "pong.json",
                        object_schema(&[("ping", resource_ref("a/ping.json"))]),
                    ),
                ),
            ])),
            ..object_schema(&[("ping", ref_to("Ping"))])
        });
        let actual: ReverseCodeGenResult = cycle_document(
            "ping",
            &[
                ("a::Ping", "Ping", "a/ping.json", "pong"),
                ("a::Pong", "Pong", "pong.json", "ping"),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_id_referred_to_from_an_absolute_id_is_copied_into_the_referrers_defs() {
        let pong: JsonSchema = with_id(
            "pong.json",
            object_schema(&[("ping", resource_ref("https://example.com/ping"))]),
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[(
                "Ping",
                JsonSchema {
                    defs: Some(defs(&[("Pong", pong)])),
                    ..with_id(
                        "https://example.com/ping",
                        object_schema(&[("pong", resource_ref("pong.json"))]),
                    )
                },
            )])),
            ..object_schema(&[("ping", ref_to("Ping"))])
        });
        let actual: ReverseCodeGenResult = cycle_document(
            "ping",
            &[
                ("a::Ping", "Ping", "https://example.com/ping", "pong"),
                ("a::Pong", "Pong", "pong.json", "ping"),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_id_referred_to_from_the_root_and_an_absolute_id_gets_a_copy_in_each() {
        let pong_copy: JsonSchema = with_id(
            "pong.json",
            object_schema(&[("ping", resource_ref("https://example.com/ping"))]),
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Ping",
                    JsonSchema {
                        defs: Some(defs(&[("Pong", pong_copy.clone())])),
                        ..with_id(
                            "https://example.com/ping",
                            object_schema(&[("pong", resource_ref("pong.json"))]),
                        )
                    },
                ),
                ("Pong", pong_copy),
            ])),
            ..object_schema(&[("ping", ref_to("Ping")), ("pong", ref_to("Pong"))])
        });
        let members: [CycleMember<'_>; 2] = [
            ("a::Ping", "Ping", "https://example.com/ping", "pong"),
            ("a::Pong", "Pong", "pong.json", "ping"),
        ];
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let first: JsonSchema = cycle_member(&mut generator, &members, 0);
        let second: JsonSchema = cycle_member(&mut generator, &members, 1);
        let actual: ReverseCodeGenResult =
            generator.into_root_schema(object_schema(&[("ping", first), ("pong", second)]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn absolute_id_root_embeds_relative_ids_once_and_they_refer_back_to_it() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[
                (
                    "Ping",
                    with_id(
                        "a/ping.json",
                        object_schema(&[("pong", resource_ref("../pong.json"))]),
                    ),
                ),
                (
                    "Pong",
                    with_id(
                        "pong.json",
                        object_schema(&[
                            ("ping", resource_ref("a/ping.json")),
                            (
                                "root",
                                resource_ref("https://example.com/schemas/root.json"),
                            ),
                        ]),
                    ),
                ),
            ])),
            ..with_id(
                "https://example.com/schemas/root.json",
                object_schema(&[("ping", ref_to("Ping"))]),
            )
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_resource_root_schema(
            "a::Root",
            "Root",
            "https://example.com/schemas/root.json",
            |generator: &mut SchemaGenerator| {
                let ping: JsonSchema = generator.resource_subschema(
                    "a::Ping",
                    "Ping",
                    "a/ping.json",
                    |generator: &mut SchemaGenerator| {
                        let pong: JsonSchema = generator.resource_subschema(
                            "a::Pong",
                            "Pong",
                            "pong.json",
                            |generator: &mut SchemaGenerator| {
                                let ping: JsonSchema = generator.resource_subschema(
                                    "a::Ping",
                                    "Ping",
                                    "a/ping.json",
                                    |_| panic!("a pending resource must not be rebuilt"),
                                );
                                let root: JsonSchema = generator.resource_subschema(
                                    "a::Root",
                                    "Root",
                                    "https://example.com/schemas/root.json",
                                    |_| panic!("the pending root must not be rebuilt"),
                                );
                                with_id(
                                    "pong.json",
                                    object_schema(&[("ping", ping), ("root", root)]),
                                )
                            },
                        );
                        with_id("a/ping.json", object_schema(&[("pong", pong)]))
                    },
                );
                with_id(
                    "https://example.com/schemas/root.json",
                    object_schema(&[("ping", ping)]),
                )
            },
        );
        assert_eq!(expected, actual);
    }

    /// The root schema of a document whose properties refer to the derived types `nodes`
    /// (`(schema_id, id, body)`, all named `Node`), by schema id.
    fn nodes_document(nodes: &[(&str, &str, JsonSchema)]) -> ReverseCodeGenResult {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        let properties: Vec<(&str, JsonSchema)> = nodes
            .iter()
            .map(|(schema_id, id, body)| {
                let reference: JsonSchema =
                    generator.resource_subschema(schema_id, "Node", id, |_| body.clone());
                (*schema_id, reference)
            })
            .collect();
        generator.into_root_schema(object_schema(&properties))
    }

    #[test]
    fn identical_resources_whose_ids_resolve_to_one_uri_merge() {
        let node: JsonSchema = with_id(
            "node.json",
            JsonSchema {
                type_: Some(JsonType::Array.into()),
                ..array_of(resource_ref("#"))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            defs: Some(defs(&[("Node", node.clone())])),
            ..object_schema(&[("a::Node", ref_to("Node")), ("b::Node", ref_to("Node"))])
        });
        let actual: ReverseCodeGenResult = nodes_document(&[
            ("a::Node", "node.json", node.clone()),
            ("b::Node", "node.json", node),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn differing_resources_whose_ids_resolve_to_one_uri_are_each_an_error() {
        let expected: ReverseCodeGenResult = Err(vec![
            ReverseCodeGenError::DuplicateResourceId {
                first: "a::Node".to_string(),
                first_id: "node.json".to_string(),
                second: "b::Node".to_string(),
                second_id: "./node.json".to_string(),
            },
            ReverseCodeGenError::DuplicateResourceId {
                first: "a::Node".to_string(),
                first_id: "node.json".to_string(),
                second: "c::Node".to_string(),
                second_id: "x/../node.json".to_string(),
            },
        ]);
        let actual: ReverseCodeGenResult = nodes_document(&[
            (
                "a::Node",
                "node.json",
                with_id("node.json", string_schema()),
            ),
            (
                "b::Node",
                "./node.json",
                with_id("./node.json", integer_schema()),
            ),
            (
                "c::Node",
                "x/../node.json",
                with_id("x/../node.json", array_of(string_schema())),
            ),
        ]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_id_that_is_not_a_uri_reference_is_an_error() {
        let expected: ReverseCodeGenResult = Err(vec![ReverseCodeGenError::InvalidResourceId {
            schema_id: "a::Node".to_string(),
            id: "not a uri".to_string(),
            reason: invalid_id_reason("not a uri"),
        }]);
        let actual: ReverseCodeGenResult = nodes_document(&[(
            "a::Node",
            "not a uri",
            with_id("not a uri", string_schema()),
        )]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_id_that_is_not_a_uri_reference_is_an_error() {
        let expected: ReverseCodeGenResult = Err(vec![ReverseCodeGenError::InvalidResourceId {
            schema_id: "a::Root".to_string(),
            id: "not a uri".to_string(),
            reason: invalid_id_reason("not a uri"),
        }]);
        let actual: ReverseCodeGenResult = SchemaGenerator::new().into_resource_root_schema(
            "a::Root",
            "Root",
            "not a uri",
            |_| with_id("not a uri", string_schema()),
        );
        assert_eq!(expected, actual);
    }

    fn reported_error() -> ReverseCodeGenError {
        ReverseCodeGenError::InvalidResourceId {
            schema_id: "a::Reported".to_string(),
            id: "reported".to_string(),
            reason: "reported by hand".to_string(),
        }
    }

    #[test]
    fn into_root_schema_returns_reported_errors() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        generator.report_errors(vec![reported_error()]);
        let expected: ReverseCodeGenResult = Err(vec![reported_error()]);
        let actual: ReverseCodeGenResult = generator.into_root_schema(string_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_root_schema_returns_reported_errors_before_resource_errors() {
        let mut generator: SchemaGenerator = SchemaGenerator::new();
        generator.report_errors(vec![reported_error()]);
        let reference: JsonSchema =
            generator.resource_subschema("a::Node", "Node", "not a uri", |_| {
                with_id("not a uri", string_schema())
            });
        let expected: ReverseCodeGenResult = Err(vec![
            reported_error(),
            ReverseCodeGenError::InvalidResourceId {
                schema_id: "a::Node".to_string(),
                id: "not a uri".to_string(),
                reason: invalid_id_reason("not a uri"),
            },
        ]);
        let actual: ReverseCodeGenResult = generator.into_root_schema(reference);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hand_written_impl_errors_are_returned_through_a_container() {
        struct Failing;
        impl ToJsonSchema for Failing {
            fn json_schema() -> ReverseCodeGenResult {
                Err(vec![reported_error()])
            }
        }
        let expected: ReverseCodeGenResult = Err(vec![reported_error()]);
        let actual: ReverseCodeGenResult = Vec::<Failing>::json_schema();
        assert_eq!(expected, actual);
    }
}
