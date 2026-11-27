//! Reverse codegen: Rust types to JSON Schema.
//!
//! Types implement [`ToJsonSchema`] to produce a [`JsonSchema`] value that can be
//! serialized via [`TryFrom`] to String or `Vec<u8>`.

#[doc(hidden)]
pub mod derive_support;
mod dialect;
mod error;
mod generated;
mod resource_uri;
mod settings;

pub use error::{ReverseCodeGenError, ReverseCodeGenResult};
pub use generated::{GeneratedSchema, GeneratedSchemaResult};
pub use settings::{
    MetaSchema, NumberFormats, ReverseCodeGenSettings, ReverseCodeGenSettingsBuilder, TargetDialect,
};

use crate::json_schema::registry::parse_uri;
use crate::json_schema::{Dialect, JsonSchema, JsonType, Schema, SpecVersion, SubschemaPosition};
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

    /// Builds this type's schema as the root of a document written by `generator`: the
    /// generator's [`ReverseCodeGenSettings`] decide the dialect and the layout.
    ///
    /// The default builds [`ToJsonSchema::json_schema_with`] into the generator and finishes
    /// it; the derive overrides it to return a named type's body inline as the root.
    ///
    /// # Errors
    ///
    /// See [`SchemaGenerator::into_root_schema`].
    #[doc(hidden)]
    fn json_schema_root(mut generator: SchemaGenerator) -> GeneratedSchemaResult {
        let body: JsonSchema = Self::json_schema_with(&mut generator);
        generator.into_root_schema(body)
    }

    /// The schema of this type where serde reads it from its parent's own map, under
    /// `#[serde(flatten)]`: the type's body itself rather than a `$ref` to it, so its
    /// properties can merge into the parent's.
    ///
    /// The derive returns a struct's or enum's body (an externally tagged enum's unit variants
    /// as `{"Name": null}` objects, the shape serde reads them in there); the default is
    /// [`ToJsonSchema::json_schema_with`].
    #[doc(hidden)]
    fn json_schema_flattened(generator: &mut SchemaGenerator) -> JsonSchema {
        Self::json_schema_with(generator)
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
    /// The body of each named type written where it is used
    /// ([`ReverseCodeGenSettings::inline_subschemas`]), by [`ToJsonSchema::schema_id`].
    inline_bodies: BTreeMap<String, JsonSchema>,
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
    settings: ReverseCodeGenSettings,
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
    /// The named types whose bodies are being inlined ([`SchemaGenerator::inline_named`]), so a
    /// type that flattens itself falls back to a `$ref` instead of recursing forever.
    inlining: BTreeSet<String>,
}

impl SchemaGenerator {
    /// Creates a generator with no definitions that writes its document as `settings` say.
    /// `SchemaGenerator::default()` writes the spec-exact 2020-12 document.
    #[must_use]
    pub fn new(settings: ReverseCodeGenSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    /// The settings this generator writes its document with, for a hand-written
    /// [`ToJsonSchema::json_schema_with`] whose schema depends on them.
    #[must_use]
    pub fn settings(&self) -> &ReverseCodeGenSettings {
        &self.settings
    }

    /// Builds the root schema of `T` as a document written as this generator's settings say.
    ///
    /// # Errors
    ///
    /// See [`SchemaGenerator::into_root_schema`].
    pub fn into_root_schema_for<T: ToJsonSchema + ?Sized>(self) -> GeneratedSchemaResult {
        T::json_schema_root(self)
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
        let inline: bool = self.settings.inline_subschemas;
        let scope: &mut Scope = self.current_scope();
        if inline && let Some(body) = scope.inline_bodies.get(schema_id) {
            return body.clone();
        }
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
            if inline && !scope.referenced_while_pending.contains(&index) {
                // Nothing refers back to it: it is written where it is used, and the
                // definition is dropped.
                scope
                    .inline_bodies
                    .insert(schema_id.to_string(), body.clone());
                return body;
            }
            scope.definitions[index].body = Some(body.into());
            index
        };
        ref_schema(provisional_ref(index))
    }

    /// Returns the body of the named type `schema_id` itself, built by `inline`, for a position
    /// that needs the schema rather than a `$ref` to it (a `#[serde(flatten)]` field merges the
    /// body's properties into its parent).
    ///
    /// When the type is already being built or inlined (a type that flattens itself), this is
    /// `fallback` (the type's `$ref`) instead, so building always ends.
    pub fn inline_named(
        &mut self,
        schema_id: &str,
        inline: impl FnOnce(&mut Self) -> JsonSchema,
        fallback: impl FnOnce(&mut Self) -> JsonSchema,
    ) -> JsonSchema {
        let scope: &Scope = self.resource_scopes.last().unwrap_or(&self.document);
        let pending_definition: bool = scope
            .type_definitions
            .get(schema_id)
            .is_some_and(|index| scope.pending.contains(index));
        let pending_resource: bool = self
            .resource_indices
            .get(schema_id)
            .is_some_and(|&index| self.resources[index].built.is_none());
        if pending_definition || pending_resource || self.inlining.contains(schema_id) {
            return fallback(self);
        }
        self.inlining.insert(schema_id.to_string());
        let body: JsonSchema = inline(self);
        self.inlining.remove(schema_id);
        body
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
    ) -> GeneratedSchemaResult {
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
    ) -> GeneratedSchemaResult {
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
    /// definitions, attaches the `$defs` (if any) to `body` and returns it as the root schema:
    /// the 2020-12 model, plus its form in the target dialect when that differs (see
    /// [`GeneratedSchema`]).
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
    pub fn into_root_schema(mut self, body: JsonSchema) -> GeneratedSchemaResult {
        let mut errors: Vec<ReverseCodeGenError> = std::mem::take(&mut self.errors);
        let settings: ReverseCodeGenSettings = std::mem::take(&mut self.settings);
        let target: TargetDialect = settings.target();
        let path: String = match definitions_path(&settings) {
            Ok(path) => path,
            Err(error) => {
                errors.push(error);
                target.default_definitions_path().to_string()
            }
        };
        if path != target.default_definitions_path() {
            errors.extend(self.resources.iter().map(|resource| {
                ReverseCodeGenError::ResourceOutsideDefinitions {
                    path: path.clone(),
                    schema_id: resource.schema_id.clone(),
                }
            }));
        }
        let model_path: &str = model_definitions_path(target, &path);
        let aliases: ModelAliases = if model_path != path {
            ModelAliases::HandWrittenAndGenerated
        } else if is_draft_4_to_7(target) {
            ModelAliases::HandWritten
        } else {
            ModelAliases::None
        };
        let (schema, definitions): (JsonSchema, BTreeMap<String, Schema>) =
            if self.resources.is_empty() {
                let existing: ExistingDefinitions = existing_definitions(&settings, &path, &body);
                std::mem::take(&mut self.document).finish(
                    body,
                    model_path,
                    aliases,
                    &existing,
                    &mut errors,
                )
            } else {
                let existing: ExistingDefinitions = existing_definitions(&settings, &path, &body);
                self.into_root_schema_with_resources(
                    body,
                    model_path,
                    aliases,
                    &existing,
                    target,
                    &mut errors,
                )
            };
        let generated: GeneratedSchema = finish_document(
            &settings,
            &path,
            model_path,
            schema,
            definitions,
            &mut errors,
        );
        if errors.is_empty() {
            Ok(generated)
        } else {
            Err(errors)
        }
    }

    /// [`SchemaGenerator::into_root_schema`] for a document that embeds resources.
    fn into_root_schema_with_resources(
        mut self,
        body: JsonSchema,
        path: &str,
        aliases: ModelAliases,
        existing: &ExistingDefinitions,
        target: TargetDialect,
        errors: &mut Vec<ReverseCodeGenError>,
    ) -> (JsonSchema, BTreeMap<String, Schema>) {
        let converts: bool = target.needs_conversion();
        let draft_4_to_7: bool = is_draft_4_to_7(target);
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
            finished[node_index] =
                Some(scope.into_root_schema(node_body, converts, draft_4_to_7, errors));
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
        document.finish(body, path, aliases, existing, errors)
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

    /// A resource scope's body with its definitions in its own `$defs`. For a target that needs
    /// conversion a hand-written `definitions` entry is reserved too, since conversion moves
    /// the `$defs` into `definitions`.
    fn into_root_schema(
        self,
        body: JsonSchema,
        converts: bool,
        draft_4_to_7: bool,
        errors: &mut Vec<ReverseCodeGenError>,
    ) -> JsonSchema {
        let mut existing: ExistingDefinitions = BTreeMap::new();
        collect_existing(&mut existing, body.defs.as_ref(), true, DEFS_PATH);
        if converts {
            collect_existing(
                &mut existing,
                body.definitions.as_ref(),
                false,
                "/definitions",
            );
        }
        let (mut body, definitions): (JsonSchema, BTreeMap<String, Schema>) = self.finish(
            body,
            DEFS_PATH,
            if draft_4_to_7 {
                ModelAliases::HandWrittenAndGenerated
            } else {
                ModelAliases::None
            },
            &existing,
            errors,
        );
        if !definitions.is_empty() {
            let defs: &mut BTreeMap<String, Schema> = body.defs.get_or_insert_with(BTreeMap::new);
            debug_assert!(
                definitions.keys().all(|name| !defs.contains_key(name)),
                "a generated definition must not replace a hand-written one"
            );
            defs.extend(definitions);
        }
        body
    }

    /// See [`SchemaGenerator::into_root_schema`]. Returns `body` and the definitions, whose
    /// `$ref`s are written to resolve under the JSON Pointer `path`.
    ///
    /// `existing` holds the entries the hand-written `body` already puts where the definitions
    /// go. A definition whose name is taken by a mergeable entry and whose body equals it is
    /// that entry (it is not returned, and its refs resolve to the entry, wherever it lives);
    /// any other definition of a taken name gets a numeric suffix that no existing entry uses.
    /// Existing entries are never renamed. Which definitions merge is the greatest fixpoint:
    /// every candidate is assumed merged, the bodies are compared under that assumption, and the
    /// candidates that differ are dropped until none changes, so recursive and mutually
    /// recursive definitions merge too. A definition that is not merged and whose `$id` is the
    /// `$id` of a hand-written entry is a [`ReverseCodeGenError::DuplicateResourceId`].
    ///
    /// On a draft 4-7 target, whose conversion moves `$defs` into `definitions`, `aliases`
    /// rewrites the refs that the document resolves in the joined container so that the model
    /// resolves them too (see [`ModelAliases`]): in the body (outside nested resources) and in
    /// every emitted generated body (outside resources, so a body that is itself a resource
    /// keeps its own refs), and both sides of the merge comparison are read in that form. A
    /// final name never equals a hand-written entry's name in either container, so a ref that
    /// names a generated definition never targets a hand-written entry.
    ///
    /// A hand-written entry that holds one of the generator's provisional `$ref`s (it was built
    /// from [`SchemaGenerator::inline_subschema`] output) never equals a resolved body, so it
    /// never merges: the definition is suffixed.
    fn finish(
        self,
        mut body: JsonSchema,
        path: &str,
        aliases: ModelAliases,
        existing: &ExistingDefinitions,
        errors: &mut Vec<ReverseCodeGenError>,
    ) -> (JsonSchema, BTreeMap<String, Schema>) {
        let hand_written: HandWrittenContainers = HandWrittenContainers::of(&body, aliases);
        let mut definitions: Vec<Definition> = self.definitions;
        let mut canonical: Vec<usize> = (0..definitions.len()).collect();
        merge_equal_hoisted_definitions(&mut definitions, &mut canonical, &mut body);

        let mut merged: BTreeSet<usize> = (0..definitions.len())
            .filter(|&index| {
                canonical[index] == index
                    && definitions[index].body.is_some()
                    && existing
                        .get(&definitions[index].name)
                        .is_some_and(|entry| entry.mergeable)
            })
            .collect();
        let mut final_names: Vec<Option<String>>;
        let mut targets: Vec<Option<String>>;
        loop {
            final_names = assign_final_names(&definitions, &canonical, existing, &merged);
            targets = ref_targets(&definitions, &final_names, &merged, existing, path);
            let resolve = |reference: &str| -> Option<String> {
                let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
                Some(format!("{}{rest}", targets[index].as_deref()?))
            };
            let alias = alias_resolver(aliases, &final_names, &merged, &hand_written);
            let dropped: Vec<usize> = merged
                .iter()
                .copied()
                .filter(|&index| {
                    let Some(entry) = existing.get(&definitions[index].name) else {
                        return true;
                    };
                    let Some(mut resolved) = definitions[index].body.clone() else {
                        return true;
                    };
                    if let Some(object) = resolved.as_object_mut() {
                        rewrite_refs(object, &resolve);
                        if aliases != ModelAliases::None {
                            rewrite_refs_outside_resources(object, &alias);
                        }
                    }
                    let entry_value: std::borrow::Cow<'_, serde_json::Value> =
                        if aliases == ModelAliases::None {
                            std::borrow::Cow::Borrowed(&entry.value)
                        } else {
                            entry.normalized_value(&alias)
                        };
                    serde_json::to_value(&resolved).ok().as_ref() != Some(entry_value.as_ref())
                })
                .collect();
            if dropped.is_empty() {
                break;
            }
            for index in dropped {
                merged.remove(&index);
            }
        }
        report_duplicate_ids(&definitions, &canonical, &merged, existing, errors);
        let alias = alias_resolver(aliases, &final_names, &merged, &hand_written);
        let resolve = |reference: &str| -> Option<String> {
            let (index, rest): (usize, &str) = parse_provisional_ref(reference)?;
            Some(format!("{}{rest}", targets[index].as_deref()?))
        };

        rewrite_refs(&mut body, &resolve);
        if aliases != ModelAliases::None {
            rewrite_refs_outside_nested_resources(&mut body, &alias);
        }
        let mut defs: BTreeMap<String, Schema> = BTreeMap::new();
        for (index, definition) in definitions.into_iter().enumerate() {
            if merged.contains(&index) {
                continue;
            }
            if let (Some(name), Some(mut definition_body)) =
                (final_names[index].clone(), definition.body)
            {
                if let Some(object) = definition_body.as_object_mut() {
                    rewrite_refs(object, &resolve);
                    // A body that is itself a resource keeps its refs: they resolve against it.
                    if aliases != ModelAliases::None {
                        rewrite_refs_outside_resources(object, &alias);
                    }
                }
                defs.insert(name, definition_body);
            }
        }
        (body, defs)
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
pub fn root_schema_for<T: ToJsonSchema + ?Sized>() -> GeneratedSchemaResult {
    SchemaGenerator::default().into_root_schema_for::<T>()
}

/// One entry a hand-written root already holds where the definitions of the target go.
#[derive(Debug, Clone)]
struct ExistingEntry {
    /// The entry as JSON.
    value: serde_json::Value,
    /// Whether a definition with an equal body may merge into it. Not when the target's
    /// conversion never walks the entry (a root extension under a custom path: refs to it
    /// would point at JSON the dialect converter leaves as written), or when the target's
    /// container takes the name from elsewhere.
    mergeable: bool,
    /// The JSON Pointer of the container the entry lives in, which refs that merge into it use
    /// (`/$defs` for an entry that conversion later moves into `definitions`).
    path: String,
    /// The entry as the model holds it, when it is a typed field (not a root extension).
    schema: Option<Schema>,
}

impl ExistingEntry {
    /// The entry's JSON with its `#/definitions/G` refs rewritten by `alias`.
    fn normalized_value(
        &self,
        alias: &impl Fn(&str) -> Option<String>,
    ) -> std::borrow::Cow<'_, serde_json::Value> {
        let Some(Schema::Object(object)) = &self.schema else {
            return std::borrow::Cow::Borrowed(&self.value);
        };
        let mut rewritten: JsonSchema = (**object).clone();
        rewrite_refs_outside_resources(&mut rewritten, alias);
        serde_json::to_value(&rewritten).map_or(
            std::borrow::Cow::Borrowed(&self.value),
            std::borrow::Cow::Owned,
        )
    }
}

/// Which refs a scope's 2020-12 model rewrites so that they resolve where the converted
/// document resolves them (see [`Scope::finish`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelAliases {
    /// The target keeps `$defs` (or is the model itself): nothing is rewritten.
    None,
    /// A draft 4-7 target, whose conversion moves `$defs` into `definitions` and rewrites
    /// `#/$defs/..` refs: a ref to a hand-written entry names it in the container where the
    /// model holds it (`#/definitions/H` to a `$defs`-only `H` is `#/$defs/H`, and
    /// `#/$defs/D` to a `definitions`-only `D` is `#/definitions/D`).
    HandWritten,
    /// Also the generated definitions, which the model keeps in `$defs` while the document
    /// places them in `definitions`: `#/definitions/G` is `#/$defs/G`.
    HandWrittenAndGenerated,
}

/// The hand-written entry names of a scope's root, in the two containers a draft 4-7
/// conversion joins.
#[derive(Debug, Default)]
struct HandWrittenContainers {
    /// Names under `$defs` that `definitions` does not hold.
    defs_only: BTreeSet<String>,
    /// Names under `definitions` that `$defs` does not hold.
    definitions_only: BTreeSet<String>,
}

impl HandWrittenContainers {
    fn of(body: &JsonSchema, aliases: ModelAliases) -> Self {
        if aliases == ModelAliases::None {
            return HandWrittenContainers::default();
        }
        let keys = |map: Option<&BTreeMap<String, Schema>>| -> BTreeSet<String> {
            map.into_iter().flatten().map(|(k, _)| k.clone()).collect()
        };
        let defs: BTreeSet<String> = keys(body.defs.as_ref());
        let definitions: BTreeSet<String> = keys(body.definitions.as_ref());
        HandWrittenContainers {
            defs_only: defs.difference(&definitions).cloned().collect(),
            definitions_only: definitions.difference(&defs).cloned().collect(),
        }
    }
}

/// The ref rewrite of a draft 4-7 target's model (see [`Scope::finish`] and [`ModelAliases`]):
/// `#/definitions/N` becomes `#/$defs/N` for every definition `N` that stays generated (its
/// final name is not that of a hand-written entry, which is merged into instead) and every
/// hand-written `$defs`-only entry, and `#/$defs/N` becomes `#/definitions/N` for every
/// hand-written `definitions`-only entry. The token is matched decoded (URI percent-encoding,
/// then JSON Pointer `~1` / `~0`) and kept as written, with any tail after it.
fn alias_resolver<'a>(
    aliases: ModelAliases,
    final_names: &[Option<String>],
    merged: &BTreeSet<usize>,
    hand_written: &'a HandWrittenContainers,
) -> impl Fn(&str) -> Option<String> + use<'a> {
    let generated: BTreeSet<String> = if aliases == ModelAliases::HandWrittenAndGenerated {
        final_names
            .iter()
            .enumerate()
            .filter(|(index, _)| !merged.contains(index))
            .filter_map(|(_, name)| name.clone())
            .collect()
    } else {
        BTreeSet::new()
    };
    move |reference: &str| -> Option<String> {
        if parse_provisional_ref(reference).is_some() {
            return None;
        }
        let (to_defs, rest): (bool, &str) = match reference.strip_prefix("#/definitions/") {
            Some(rest) => (true, rest),
            None => (false, reference.strip_prefix("#/$defs/")?),
        };
        let (token, tail): (&str, &str) = rest.find('/').map_or((rest, ""), |at| rest.split_at(at));
        let name: String = decode_fragment_token(token)?;
        let container: &str = if to_defs {
            (generated.contains(&name) || hand_written.defs_only.contains(&name))
                .then_some(DEFS_PATH)?
        } else {
            hand_written
                .definitions_only
                .contains(&name)
                .then_some("/definitions")?
        };
        Some(format!("#{container}/{token}{tail}"))
    }
}

/// Decodes one JSON Pointer reference token of a URI fragment: URI percent-encoding first
/// (RFC 6901 §6), then `~1` and `~0`. `None` when an escape is malformed or the bytes are not
/// UTF-8.
fn decode_fragment_token(token: &str) -> Option<String> {
    let unescaped: std::borrow::Cow<'_, str> = if token.contains('%') {
        let bytes: &[u8] = token.as_bytes();
        let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
        let mut at: usize = 0;
        while at < bytes.len() {
            if bytes[at] == b'%' {
                let hex: &str = token
                    .get(at + 1..at + 3)
                    .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))?;
                decoded.push(u8::from_str_radix(hex, 16).ok()?);
                at += 3;
            } else {
                decoded.push(bytes[at]);
                at += 1;
            }
        }
        std::borrow::Cow::Owned(String::from_utf8(decoded).ok()?)
    } else {
        std::borrow::Cow::Borrowed(token)
    };
    if unescaped.contains('~') {
        crate::json_pointer::decode_token(&unescaped).ok()
    } else {
        Some(unescaped.into_owned())
    }
}

/// Whether `target` is draft 4, 6 or 7: the targets whose definitions go to `definitions`.
fn is_draft_4_to_7(target: TargetDialect) -> bool {
    matches!(
        target,
        TargetDialect::Draft04 | TargetDialect::Draft06 | TargetDialect::Draft07
    )
}

/// The entries a hand-written root body already holds where the definitions of the target go,
/// by name.
type ExistingDefinitions = BTreeMap<String, ExistingEntry>;

fn collect_existing(
    existing: &mut ExistingDefinitions,
    entries: Option<&BTreeMap<String, Schema>>,
    mergeable: bool,
    container: &str,
) {
    for (name, schema) in entries.into_iter().flatten() {
        existing
            .entry(name.clone())
            .or_insert_with(|| match serde_json::to_value(schema) {
                Ok(value) => ExistingEntry {
                    value,
                    mergeable,
                    path: container.to_string(),
                    schema: Some(schema.clone()),
                },
                Err(_) => ExistingEntry {
                    value: serde_json::Value::Null,
                    mergeable: false,
                    path: container.to_string(),
                    schema: None,
                },
            });
    }
}

/// The names `body` already uses in the container of the target at the JSON Pointer `path`.
fn existing_definitions(
    settings: &ReverseCodeGenSettings,
    path: &str,
    body: &JsonSchema,
) -> ExistingDefinitions {
    let converts: bool = settings.target().needs_conversion();
    let mut existing: ExistingDefinitions = BTreeMap::new();
    if path == DEFS_PATH {
        collect_existing(&mut existing, body.defs.as_ref(), true, DEFS_PATH);
    } else if path == "/definitions" {
        collect_existing(
            &mut existing,
            body.definitions.as_ref(),
            true,
            "/definitions",
        );
        if converts {
            // Conversion moves `$defs` into `definitions`, where a ref still reads `#/$defs/..`
            // until the converter rewrites it, so an equal entry is merged into where it is.
            collect_existing(&mut existing, body.defs.as_ref(), true, DEFS_PATH);
        }
    } else {
        let tokens: Vec<String> = path
            .split('/')
            .skip(1)
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect();
        // A path through the `$defs` / `definitions` fields lives in the model's maps.
        let owned: Option<serde_json::Value> = match tokens[0].as_str() {
            "$defs" => body
                .defs
                .as_ref()
                .and_then(|d| serde_json::to_value(d).ok()),
            "definitions" => body
                .definitions
                .as_ref()
                .and_then(|d| serde_json::to_value(d).ok()),
            _ => None,
        };
        let is_field: bool = matches!(tokens[0].as_str(), "$defs" | "definitions");
        let mut current: Option<&serde_json::Value> = if is_field {
            owned.as_ref()
        } else {
            body.extensions.get(&tokens[0])
        };
        for token in &tokens[1..] {
            current = current.and_then(|value| value.get(token));
        }
        if let Some(serde_json::Value::Object(entries)) = current {
            for (name, value) in entries {
                existing.insert(
                    name.clone(),
                    ExistingEntry {
                        value: value.clone(),
                        // Conversion walks the model's fields, not a root extension.
                        mergeable: !converts,
                        path: path.to_string(),
                        schema: None,
                    },
                );
            }
        }
    }
    existing
}

/// The ref prefix (`#<container>/<escaped name>`) of each live definition: a merged one points
/// at the hand-written entry's own container, any other at `path`.
fn ref_targets(
    definitions: &[Definition],
    final_names: &[Option<String>],
    merged: &BTreeSet<usize>,
    existing: &ExistingDefinitions,
    path: &str,
) -> Vec<Option<String>> {
    final_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let name: &str = name.as_deref()?;
            let container: &str = if merged.contains(&index) {
                existing
                    .get(&definitions[index].name)
                    .map_or(path, |entry| entry.path.as_str())
            } else {
                path
            };
            Some(format!("#{container}/{}", escape_pointer_token(name)))
        })
        .collect()
}

/// Reports a [`ReverseCodeGenError::DuplicateResourceId`] for each definition that is not
/// merged into a hand-written entry yet carries the same resolved `$id` as one, since the
/// document would hold two resources with one URI (2020-12 core §8.2.1).
fn report_duplicate_ids(
    definitions: &[Definition],
    canonical: &[usize],
    merged: &BTreeSet<usize>,
    existing: &ExistingDefinitions,
    errors: &mut Vec<ReverseCodeGenError>,
) {
    let hand_written: Vec<(&String, &str)> = existing
        .iter()
        .filter_map(|(name, entry)| Some((name, entry.value.get("$id")?.as_str()?)))
        .collect();
    if hand_written.is_empty() {
        return;
    }
    let generated: Vec<(usize, &str)> = definitions
        .iter()
        .enumerate()
        .filter(|(index, _)| canonical[*index] == *index && !merged.contains(index))
        .filter_map(|(index, definition)| {
            let object: &JsonSchema = definition.body.as_ref()?.as_object()?;
            Some((index, object.id.as_deref()?))
        })
        .collect();
    let depth: usize = resource_uri::placeholder_depth(
        hand_written
            .iter()
            .map(|(_, id)| *id)
            .chain(generated.iter().map(|(_, id)| *id)),
        1,
    );
    let bases: Placed = resource_uri::placeholder_bases(depth);
    for (index, id) in generated {
        let Ok(placed) = resource_uri::place(&bases, id) else {
            continue;
        };
        for (name, existing_id) in &hand_written {
            if resource_uri::place(&bases, existing_id).is_ok_and(|other| other == placed) {
                let second: String = match &definitions[index].identity {
                    DefinitionIdentity::Type(schema_id) => schema_id.clone(),
                    DefinitionIdentity::Hoisted(_) => {
                        format!("a hoisted definition `{}`", definitions[index].name)
                    }
                };
                errors.push(ReverseCodeGenError::DuplicateResourceId {
                    first: format!("#{}/{}", existing[*name].path, escape_pointer_token(name)),
                    first_id: (*existing_id).to_string(),
                    second,
                    second_id: id.to_string(),
                });
            }
        }
    }
}

/// The JSON Pointer of a resource scope's definitions, and of the document's in the default
/// (2020-12) layout.
const DEFS_PATH: &str = "/$defs";

/// The JSON Pointer the 2020-12 model places the definitions under, and writes their refs
/// against. Conversion to draft 4-7 moves `$defs` into `definitions` and rewrites `#/$defs/..`
/// refs, so for `/definitions` the model keeps them in `$defs`: its refs resolve, and read like
/// the hand-written `#/$defs/..` refs the root may hold to the same definitions. Any other path
/// is the same in the model and the target document.
fn model_definitions_path(target: TargetDialect, path: &str) -> &str {
    if path == "/definitions" && is_draft_4_to_7(target) {
        DEFS_PATH
    } else {
        path
    }
}

/// The normalized JSON Pointer the document root's definitions go under: the setting with one
/// leading `#` and one trailing `/` removed, or the dialect's own container.
fn definitions_path(settings: &ReverseCodeGenSettings) -> Result<String, ReverseCodeGenError> {
    let target: TargetDialect = settings.target();
    let Some(raw) = settings.definitions_path.as_deref() else {
        return Ok(target.default_definitions_path().to_string());
    };
    let invalid = |reason: String| ReverseCodeGenError::InvalidDefinitionsPath {
        path: raw.to_string(),
        reason,
    };
    let path: &str = raw.strip_prefix('#').unwrap_or(raw);
    let path: &str = path.strip_suffix('/').unwrap_or(path);
    if path.is_empty() {
        return Err(invalid("it is empty".to_string()));
    }
    let Some(tokens) = path.strip_prefix('/') else {
        return Err(invalid("a JSON Pointer starts with `/`".to_string()));
    };
    let mut first: String = String::new();
    for (index, token) in tokens.split('/').enumerate() {
        if token.is_empty() {
            return Err(invalid("it has an empty token".to_string()));
        }
        let mut unescaped: String = String::with_capacity(token.len());
        let mut chars = token.chars();
        while let Some(c) = chars.next() {
            if c != '~' {
                unescaped.push(c);
                continue;
            }
            match chars.next() {
                Some('0') => unescaped.push('~'),
                Some('1') => unescaped.push('/'),
                _ => return Err(invalid("`~` must be followed by `0` or `1`".to_string())),
            }
        }
        if index == 0 {
            first = unescaped;
        }
    }
    let is_container: bool = first == "$defs" || first == "definitions";
    if is_container && !dialect::defines(target, &first) {
        return Err(invalid(format!("`{first}` is not a keyword of {target}")));
    }
    if !is_container
        && (dialect::defines(target, &first) || Dialect::default().keyword(&first).is_some())
    {
        return Err(invalid(format!(
            "`{first}` is a keyword of {target}, not a place for definitions"
        )));
    }
    Ok(path.to_string())
}

/// Sets the finished document's `$schema` and places the definitions under `path`. For a target
/// that is not 2020-12 the document is also converted to it, once, here: the 2020-12 model
/// stays exact (its `$schema` is 2020-12's, so it validates as the types define) and the
/// converted JSON is kept beside it.
fn finish_document(
    settings: &ReverseCodeGenSettings,
    path: &str,
    model_path: &str,
    mut body: JsonSchema,
    definitions: BTreeMap<String, Schema>,
    errors: &mut Vec<ReverseCodeGenError>,
) -> GeneratedSchema {
    let target: TargetDialect = settings.target();
    let schema_uri: Option<String> = match &settings.meta_schema {
        MetaSchema::FollowDialect => target.schema_uri().map(str::to_string),
        MetaSchema::Omit => None,
        MetaSchema::Uri(uri) => match parse_uri(uri) {
            Ok(_) => Some(uri.clone()),
            Err(reason) => {
                errors.push(ReverseCodeGenError::InvalidMetaSchemaUri {
                    uri: uri.clone(),
                    reason,
                });
                None
            }
        },
    };
    if !target.needs_conversion() {
        body.schema = schema_uri;
        return GeneratedSchema::native(place_definitions(body, model_path, definitions, errors));
    }
    let root_siblings: bool = schema_uri.is_some() || !definitions.is_empty();
    let mut pass: dialect::Converter<'_> =
        dialect::Converter::new(target, path, errors).with_root_siblings(root_siblings);
    let mut target_json: serde_json::Value = pass.convert(&body, "", true);
    let target_definitions: serde_json::Map<String, serde_json::Value> = definitions
        .iter()
        .map(|(name, schema)| {
            let pointer: String = format!("{path}/{}", escape_pointer_token(name));
            (name.clone(), pass.convert(schema, &pointer, false))
        })
        .collect();
    if let (serde_json::Value::Object(root), Some(uri)) = (&mut target_json, schema_uri) {
        root.insert("$schema".to_string(), serde_json::Value::String(uri));
    }
    if let serde_json::Value::Object(root) = &mut target_json {
        place_in_value(root, path, target_definitions, errors);
    }
    body.schema = match settings.meta_schema {
        MetaSchema::Omit => None,
        _ => Some(SpecVersion::Draft202012.schema_uri().to_string()),
    };
    GeneratedSchema::converted(
        place_definitions(body, model_path, definitions, errors),
        target_json,
    )
}

/// Writes `definitions` into `body` under the JSON Pointer `path`.
fn place_definitions(
    mut body: JsonSchema,
    path: &str,
    definitions: BTreeMap<String, Schema>,
    errors: &mut Vec<ReverseCodeGenError>,
) -> JsonSchema {
    if definitions.is_empty() {
        return body;
    }
    if path == DEFS_PATH {
        let defs: &mut BTreeMap<String, Schema> = body.defs.get_or_insert_with(BTreeMap::new);
        debug_assert!(
            definitions.keys().all(|name| !defs.contains_key(name)),
            "a generated definition must not replace a hand-written one"
        );
        defs.extend(definitions);
        return body;
    }
    if path == "/definitions" {
        let map: &mut BTreeMap<String, Schema> = body.definitions.get_or_insert_with(BTreeMap::new);
        debug_assert!(
            definitions.keys().all(|name| !map.contains_key(name)),
            "a generated definition must not replace a hand-written one"
        );
        map.extend(definitions);
        return body;
    }
    let Ok(serde_json::Value::Object(entries)) = serde_json::to_value(&definitions) else {
        unreachable!("a schema map serializes to a JSON object");
    };
    let first: String = path
        .split('/')
        .nth(1)
        .unwrap_or_default()
        .replace("~1", "/")
        .replace("~0", "~");
    if first == "$defs" || first == "definitions" {
        // A path through a field the model holds: place into its JSON and read it back.
        let field: &mut Option<BTreeMap<String, Schema>> = if first == "$defs" {
            &mut body.defs
        } else {
            &mut body.definitions
        };
        let mut root: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        if let Some(map) = field
            .as_ref()
            .and_then(|map| serde_json::to_value(map).ok())
        {
            root.insert(first.clone(), map);
        }
        let before: usize = errors.len();
        place_in_value(&mut root, path, entries, errors);
        if errors.len() == before {
            let placed: Option<BTreeMap<String, Schema>> = root
                .remove(&first)
                .and_then(|value| serde_json::from_value(value).ok());
            if placed.is_some() {
                *field = placed;
            } else {
                // A definition named like a keyword the model types differently.
                errors.push(ReverseCodeGenError::DefinitionsPathOccupied {
                    path: path.to_string(),
                });
            }
        }
        return body;
    }
    // The first token is no keyword the model holds as a field, so it is an extension; the rest
    // of the path is JSON.
    let mut extensions: serde_json::Map<String, serde_json::Value> =
        std::mem::take(&mut body.extensions).into_iter().collect();
    place_in_value(&mut extensions, path, entries, errors);
    body.extensions = extensions.into_iter().collect();
    body
}

/// Writes `entries` into `root` under the JSON Pointer `path`, creating the objects on the way.
/// A non-object already at `path` is occupied ([`ReverseCodeGenError::DefinitionsPathOccupied`],
/// once per path). An object there takes the entries beside the ones it holds; the generator
/// renames its definitions around the names the hand-written root holds, so a name that is still
/// taken is occupied too.
fn place_in_value(
    root: &mut serde_json::Map<String, serde_json::Value>,
    path: &str,
    entries: serde_json::Map<String, serde_json::Value>,
    errors: &mut Vec<ReverseCodeGenError>,
) {
    if entries.is_empty() {
        return;
    }
    let mut occupied = || {
        let error = ReverseCodeGenError::DefinitionsPathOccupied {
            path: path.to_string(),
        };
        if !errors.contains(&error) {
            errors.push(error);
        }
    };
    let tokens: Vec<String> = path
        .split('/')
        .skip(1)
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect();
    let Some((last, parents)) = tokens.split_last() else {
        unreachable!("a definitions path has a token");
    };
    let mut current: &mut serde_json::Map<String, serde_json::Value> = root;
    for token in parents {
        let next: &mut serde_json::Value = current
            .entry(token.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let serde_json::Value::Object(map) = next else {
            occupied();
            return;
        };
        current = map;
    }
    match current.get_mut(last) {
        None => {
            current.insert(last.clone(), serde_json::Value::Object(entries));
        }
        Some(serde_json::Value::Object(existing)) => {
            for (name, entry) in entries {
                if existing.contains_key(&name) {
                    occupied();
                } else {
                    existing.insert(name, entry);
                }
            }
        }
        Some(_) => occupied(),
    }
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
    let mut roots: Vec<&mut JsonSchema> = vec![body];
    roots.extend(
        scope
            .definitions
            .iter_mut()
            .filter_map(|definition| definition.body.as_mut().and_then(Schema::as_object_mut)),
    );
    for root in roots {
        for_each_ref(root, false, &mut |reference: &mut String| {
            if let Some(index) = parse_resource_ref(reference) {
                found.insert(index);
            }
        });
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
    for_each_ref(schema, false, &mut |reference: &mut String| {
        if let Some(resolved) = resolve(reference) {
            *reference = resolved;
        }
    });
}

/// Like [`rewrite_refs`], but leaves every subschema with `$id` (and everything inside it)
/// untouched: its `$ref`s resolve against that resource, not the schema being rewritten.
fn rewrite_refs_outside_resources(
    schema: &mut JsonSchema,
    resolve: &impl Fn(&str) -> Option<String>,
) {
    for_each_ref(schema, true, &mut |reference: &mut String| {
        if let Some(resolved) = resolve(reference) {
            *reference = resolved;
        }
    });
}

/// Like [`rewrite_refs_outside_resources`] for a schema that is itself the root of its resource:
/// its own `$id` does not hide it, only the `$id` of a subschema does.
fn rewrite_refs_outside_nested_resources(
    schema: &mut JsonSchema,
    resolve: &impl Fn(&str) -> Option<String>,
) {
    let id: Option<String> = schema.id.take();
    rewrite_refs_outside_resources(schema, resolve);
    schema.id = id;
}

/// Pushes the subschemas a raw `keyword: value` holds, per `dialect`'s keyword table.
fn push_raw_subschemas<'a>(
    dialect: Dialect,
    keyword: &str,
    value: &'a mut serde_json::Value,
    stack: &mut Vec<RefNode<'a>>,
) {
    let Some(position) = dialect.keyword(keyword).and_then(|k| k.subschemas) else {
        return;
    };
    match (position, value) {
        (
            SubschemaPosition::Array | SubschemaPosition::SingleOrArray,
            serde_json::Value::Array(items),
        ) => {
            stack.extend(items.iter_mut().map(RefNode::Raw));
        }
        (SubschemaPosition::Map, serde_json::Value::Object(map)) => {
            stack.extend(map.values_mut().map(RefNode::Raw));
        }
        (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, value) => {
            stack.push(RefNode::Raw(value));
        }
        _ => {}
    }
}

/// A schema [`for_each_ref`] still has to visit: a typed one, or one held as raw JSON in an
/// `extensions` keyword (`prefixItems`, `unevaluatedProperties`).
enum RefNode<'a> {
    Typed(&'a mut JsonSchema),
    Raw(&'a mut serde_json::Value),
}

/// Calls `visit` on every `$ref` in `schema` at any depth, through typed subschemas and through
/// the subschema keywords (per the 2020-12 keyword table) of its raw `extensions` values. With
/// `skip_resources`, a subschema with `$id` and everything inside it is not visited.
fn for_each_ref(
    schema: &mut JsonSchema,
    skip_resources: bool,
    visit: &mut impl FnMut(&mut String),
) {
    let dialect: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
    let mut stack: Vec<RefNode<'_>> = vec![RefNode::Typed(schema)];
    while let Some(node) = stack.pop() {
        match node {
            RefNode::Typed(current) => {
                if skip_resources && current.id.is_some() {
                    continue;
                }
                if let Some(reference) = current.ref_.as_mut() {
                    visit(reference);
                }
                let (subschemas, extensions) = current.subschemas_mut();
                for (keyword, value) in extensions {
                    push_raw_subschemas(dialect, keyword, value, &mut stack);
                }
                stack.extend(subschemas.into_iter().map(RefNode::Typed));
            }
            RefNode::Raw(value) => {
                let serde_json::Value::Object(object) = value else {
                    continue;
                };
                if skip_resources && object.contains_key("$id") {
                    continue;
                }
                for (keyword, value) in object.iter_mut() {
                    if keyword == "$ref" {
                        if let serde_json::Value::String(reference) = value {
                            visit(reference);
                        }
                    } else {
                        push_raw_subschemas(dialect, keyword, value, &mut stack);
                    }
                }
            }
        }
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
///
/// Names in `existing` are taken by the hand-written body: a definition in `is_existing` is
/// that entry and keeps the name, and any other definition of such a name is suffixed.
fn assign_final_names(
    definitions: &[Definition],
    canonical: &[usize],
    existing: &ExistingDefinitions,
    is_existing: &BTreeSet<usize>,
) -> Vec<Option<String>> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut final_names: Vec<Option<String>> = vec![None; definitions.len()];
    for (index, definition) in definitions.iter().enumerate() {
        if canonical[index] == index && definition.body.is_some() {
            if is_existing.contains(&index) {
                final_names[index] = Some(definition.name.clone());
            } else {
                groups.entry(&definition.name).or_default().push(index);
            }
        }
    }
    let mut taken: BTreeSet<String> = existing.keys().cloned().collect();
    for (name, members) in &groups {
        if let [index] = members.as_slice()
            && !existing.contains_key(*name)
        {
            taken.insert((*name).to_string());
            final_names[*index] = Some((*name).to_string());
        }
    }
    for (name, members) in &mut groups {
        if members.len() < 2 && !existing.contains_key(*name) {
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

/// `schema` with the `format` the generator's [`NumberFormats`] give the number type: `openapi`
/// (none when the vocabulary has none for it) or `schemars`.
fn with_number_format(
    mut schema: JsonSchema,
    generator: &SchemaGenerator,
    openapi: Option<&str>,
    schemars: &str,
) -> JsonSchema {
    schema.format = match generator.settings.number_formats {
        None => None,
        Some(NumberFormats::OpenApi) => openapi.map(str::to_string),
        Some(NumberFormats::Schemars) => Some(schemars.to_string()),
    };
    schema
}

/// [`ToJsonSchema`] for an integer type: its exact range, and its `format` under
/// [`ReverseCodeGenSettings::number_formats`].
macro_rules! integer_schema {
    ($type:ty, $openapi:expr, $schemars:literal) => {
        impl ToJsonSchema for $type {
            fn json_schema() -> ReverseCodeGenResult {
                Ok(integer_schema_with_bounds(<$type>::MIN, <$type>::MAX))
            }

            fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
                with_number_format(
                    integer_schema_with_bounds(<$type>::MIN, <$type>::MAX),
                    generator,
                    $openapi,
                    $schemars,
                )
            }
        }
    };
}

integer_schema!(i8, Some("int32"), "int8");
integer_schema!(u8, Some("int32"), "uint8");
integer_schema!(i16, Some("int32"), "int16");
integer_schema!(u16, Some("int32"), "uint16");
integer_schema!(i32, Some("int32"), "int32");
integer_schema!(u32, Some("int64"), "uint32");
integer_schema!(i64, Some("int64"), "int64");
integer_schema!(u64, None, "uint64");

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

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        with_number_format(
            number_schema_with_bounds(f64::from(f32::MIN), f64::from(f32::MAX)),
            generator,
            Some("float"),
            "float",
        )
    }
}

impl ToJsonSchema for f64 {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(number_schema_with_bounds(f64::MIN, f64::MAX))
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        with_number_format(
            number_schema_with_bounds(f64::MIN, f64::MAX),
            generator,
            Some("double"),
            "double",
        )
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
/// `Option<Option<T>>` field) or on a field marked `#[json_schema(required)]`. Generated code
/// uses [`Nullable`](crate::Nullable) for optional nullable fields instead.
impl<T: ToJsonSchema> ToJsonSchema for Option<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        nullable(T::json_schema_with(generator))
    }
}

impl<T: ToJsonSchema> ToJsonSchema for crate::Nullable<T> {
    /// `T`'s schema with `null` allowed, as for `Option<T>`; a `Nullable` field is optional.
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        nullable(T::json_schema_with(generator))
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Vec<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Array.into()),
            items: Some(T::json_schema_with(generator).into()),
            ..Default::default()
        }
    }
}

/// The schema of a set of `T`: an array of `T` with `uniqueItems`.
fn set_schema<T: ToJsonSchema>(generator: &mut SchemaGenerator) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Array.into()),
        items: Some(T::json_schema_with(generator).into()),
        unique_items: Some(true),
        ..Default::default()
    }
}

impl<T: ToJsonSchema, S> ToJsonSchema for std::collections::HashSet<T, S> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        set_schema::<T>(generator)
    }
}

impl<T: ToJsonSchema> ToJsonSchema for BTreeSet<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        set_schema::<T>(generator)
    }
}

#[cfg(feature = "indexmap")]
impl<T: ToJsonSchema, S> ToJsonSchema for indexmap::IndexSet<T, S> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        set_schema::<T>(generator)
    }
}

/// The schema of a map from strings to `V`: an object whose every value is a `V`.
fn map_schema<V: ToJsonSchema>(generator: &mut SchemaGenerator) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::Object.into()),
        additional_properties: Some(V::json_schema_with(generator).into()),
        ..Default::default()
    }
}

impl<V: ToJsonSchema> ToJsonSchema for BTreeMap<String, V> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        map_schema::<V>(generator)
    }
}

impl<V: ToJsonSchema, S> ToJsonSchema for std::collections::HashMap<String, V, S> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        map_schema::<V>(generator)
    }
}

#[cfg(feature = "indexmap")]
impl<V: ToJsonSchema, S> ToJsonSchema for indexmap::IndexMap<String, V, S> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        map_schema::<V>(generator)
    }
}

impl<T: ToJsonSchema> ToJsonSchema for Box<T> {
    fn json_schema() -> ReverseCodeGenResult {
        root_schema_for::<Self>().map(GeneratedSchema::into_schema)
    }

    fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
        T::json_schema_with(generator)
    }

    fn json_schema_flattened(generator: &mut SchemaGenerator) -> JsonSchema {
        T::json_schema_flattened(generator)
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

/// The schema object `json` holds, for a type whose schema is written out as JSON (see
/// [`schema_marker!`](crate::schema_marker)).
///
/// # Errors
///
/// [`ReverseCodeGenError::InvalidSchema`] when `json` is not a JSON Schema object.
pub fn schema_from_json(json: &str) -> ReverseCodeGenResult {
    serde_json::from_str::<JsonSchema>(json).map_err(|error: serde_json::Error| {
        vec![ReverseCodeGenError::InvalidSchema {
            json: json.to_string(),
            reason: error.to_string(),
        }]
    })
}

/// A unit type whose [`ToJsonSchema`] is the schema written out as JSON: generated code names
/// it in `#[json_schema(with = "...")]` for a field of a type converted from that schema
/// (`CodeGenSettings::convert`), which has no impl of the crate's trait of its own.
///
/// ```
/// use json_schema_rs::ToJsonSchema;
///
/// json_schema_rs::schema_marker!(Decimal, r#"{"type":"string","format":"decimal"}"#);
///
/// let expected: serde_json::Value = serde_json::json!({"type": "string", "format": "decimal"});
/// let actual: serde_json::Value =
///     serde_json::to_value(Decimal::json_schema().unwrap()).unwrap();
/// assert_eq!(expected, actual);
/// ```
#[macro_export]
macro_rules! schema_marker {
    ($name:ident, $json:literal) => {
        /// The JSON Schema of a converted type, for `ToJsonSchema`.
        pub struct $name;

        impl $crate::ToJsonSchema for $name {
            fn json_schema() -> $crate::ReverseCodeGenResult {
                $crate::reverse_code_gen::schema_from_json($json)
            }
        }
    };
}

/// A string with `format`.
fn formatted_string(format: &str) -> JsonSchema {
    JsonSchema {
        type_: Some(JsonType::String.into()),
        format: Some(format.to_string()),
        ..Default::default()
    }
}

#[cfg(feature = "uuid")]
impl ToJsonSchema for uuid::Uuid {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(formatted_string("uuid"))
    }
}

/// Any time zone: `chrono` serializes every `DateTime` as RFC 3339 with its offset.
#[cfg(feature = "chrono")]
impl<Tz: chrono::TimeZone> ToJsonSchema for chrono::DateTime<Tz> {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(formatted_string("date-time"))
    }
}

#[cfg(feature = "chrono")]
impl ToJsonSchema for chrono::NaiveDate {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(formatted_string("date"))
    }
}

impl ToJsonSchema for std::net::Ipv4Addr {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(formatted_string("ipv4"))
    }
}

impl ToJsonSchema for std::net::Ipv6Addr {
    fn json_schema() -> ReverseCodeGenResult {
        Ok(formatted_string("ipv6"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedSchema, GeneratedSchemaResult, HandWrittenContainers, MetaSchema, ModelAliases,
        NumberFormats, ReverseCodeGenError, ReverseCodeGenResult, ReverseCodeGenSettings,
        SchemaGenerator, TargetDialect, ToJsonSchema, alias_resolver, root_schema_for,
    };
    use crate::json_schema::{JsonSchema, JsonType, Schema, TypeSet};
    use std::collections::{BTreeMap, BTreeSet};

    /// The `$schema` of a root document written with the default settings.
    const SCHEMA_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

    /// `result` with the schema serialized to JSON text.
    fn serialized<S: serde::Serialize>(
        result: Result<S, Vec<ReverseCodeGenError>>,
    ) -> Result<String, Vec<ReverseCodeGenError>> {
        result.map(|schema: S| serde_json::to_string(&schema).expect("serialize"))
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
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"array","items":{}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Vec::<serde_json::Value>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_string_json_schema_adds_null_to_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":["null","string"]}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<String>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_option_string_json_schema_adds_null_once() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":["null","string"]}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<Option<String>>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_serde_json_value_json_schema_stays_empty_schema() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            ..JsonSchema::default()
        });
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
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"array","items":{"type":["null","string"]}}"#.to_string());
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
                schema: Some(SCHEMA_2020_12.to_string()),
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
                schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":["array","null"],"items":{"type":"string"}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<Vec<String>>::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn hash_set_string_json_schema_has_unique_items_true() {
        use std::collections::HashSet;
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            type_: Some(JsonType::Array.into()),
            items: Some(string_schema().into()),
            unique_items: Some(true),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = HashSet::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn btree_set_string_json_schema_has_unique_items_true() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            type_: Some(JsonType::Array.into()),
            items: Some(string_schema().into()),
            unique_items: Some(true),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = BTreeSet::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "indexmap")]
    #[test]
    fn index_set_string_json_schema_has_unique_items_true() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            type_: Some(JsonType::Array.into()),
            items: Some(string_schema().into()),
            unique_items: Some(true),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = indexmap::IndexSet::<String>::json_schema();
        assert_eq!(expected, actual);
    }

    #[test]
    fn btree_map_json_schema_serializes_value_schema_as_additional_properties() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":{"type":"string"}}"#.to_string());
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

    #[cfg(feature = "chrono")]
    #[test]
    fn fixed_offset_date_time_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"date-time"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(chrono::DateTime::<chrono::FixedOffset>::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn utc_date_time_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"date-time"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(chrono::DateTime::<chrono::Utc>::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "chrono")]
    #[test]
    fn naive_date_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"date"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(chrono::NaiveDate::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv4_addr_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"ipv4"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(std::net::Ipv4Addr::json_schema());
        assert_eq!(expected, actual);
    }

    #[test]
    fn ipv6_addr_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string","format":"ipv6"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(std::net::Ipv6Addr::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn option_uuid_json_schema_adds_null_to_type() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":["null","string"],"format":"uuid"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(Option::<uuid::Uuid>::json_schema());
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "uuid")]
    #[test]
    fn vec_uuid_json_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"array","items":{"type":"string","format":"uuid"}}"#.to_string());
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema =
            generator.named_subschema("a::Name", "Name", |_| string_schema());
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("Name", string_schema())])),
            ..ref_to("Name")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn named_subschema_builds_each_schema_id_once() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[(
                "Node",
                JsonSchema {
                    items: Some(ref_to("Node").into()),
                    ..Default::default()
                },
            )])),
            ..ref_to("Node")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn same_name_different_schema_ids_are_all_suffixed_in_schema_id_order() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("Name0", string_schema()),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Name0")), ("second", ref_to("Name1"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn suffixed_names_skip_a_name_already_taken() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let taken: JsonSchema = generator.named_subschema("c::Name0", "Name0", |_| JsonSchema {
            type_: Some(JsonType::Boolean.into()),
            ..Default::default()
        });
        let first: JsonSchema = generator.named_subschema("a::Name", "Name", |_| string_schema());
        let second: JsonSchema = generator.named_subschema("b::Name", "Name", |_| integer_schema());
        let body: JsonSchema =
            object_schema(&[("taken", taken), ("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_without_back_reference_has_no_defs() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            ..string_schema()
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_named_root_schema("a::Name", "Name", |_| string_schema())
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_with_back_reference_also_defines_root() {
        let body: JsonSchema = JsonSchema {
            items: Some(ref_to("Node").into()),
            ..Default::default()
        };
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("Node", body.clone())])),
            ..body
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_named_root_schema("a::Node", "Node", |generator: &mut SchemaGenerator| {
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
            })
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_root_name_shared_with_nested_type_keeps_nested_unsuffixed() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("Name", integer_schema())])),
            ..object_schema(&[("nested", ref_to("Name"))])
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_named_root_schema("a::Name", "Name", |generator: &mut SchemaGenerator| {
                object_schema(&[(
                    "nested",
                    generator.named_subschema("b::Name", "Name", |_| integer_schema()),
                )])
            })
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_named_root_schema_back_referenced_root_shares_name_with_nested_type() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                (
                    "Name0",
                    object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))]),
                ),
                ("Name1", integer_schema()),
            ])),
            ..object_schema(&[("nested", ref_to("Name1")), ("this", ref_to("Name0"))])
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_named_root_schema("a::Name", "Name", |generator: &mut SchemaGenerator| {
                let nested: JsonSchema =
                    generator.named_subschema("b::Name", "Name", |_| integer_schema());
                let this: JsonSchema = generator.named_subschema("a::Name", "Name", |_| {
                    panic!("a pending schema id must not be rebuilt")
                });
                object_schema(&[("nested", nested), ("this", this)])
            })
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_defs_and_drops_schema_keyword() {
        let standalone: JsonSchema = JsonSchema {
            schema: Some("https://json-schema.org/draft/2020-12/schema".to_string()),
            ..standalone_with_def("Inner", string_schema())
        };
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            ..standalone_with_def("Inner", string_schema())
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_hoists_boolean_def() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(BTreeMap::from([("Never".to_string(), Schema::Bool(false))])),
            ..ref_to("Never")
        };
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/Never","$defs":{"Never":false}}"##.to_string());
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
        let actual: JsonSchema = SchemaGenerator::default().inline_subschema(standalone);
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let named: JsonSchema = generator.named_subschema("a::S", "S", |_| string_schema());
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(object_schema(&[("named", named), ("inlined", inlined)]))
            .map(GeneratedSchema::into_schema);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("Node", array_of(resource_ref("#")))])),
            ..ref_to("Node")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_keeps_named_dependencies_in_its_own_defs() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.resource_subschema(
            "a::Resource",
            "Resource",
            "https://example.com/resource",
            |generator: &mut SchemaGenerator| {
                array_of(generator.named_subschema("a::Leaf", "Leaf", |_| string_schema()))
            },
        );
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[(
                "Resource",
                JsonSchema {
                    defs: Some(defs(&[("Leaf", string_schema())])),
                    ..array_of(ref_to("Leaf"))
                },
            )])),
            ..ref_to("Resource")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_inside_a_resource_is_embedded_at_the_root_and_referred_to_by_id() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", string_schema()),
            ])),
            ..ref_to("A")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_refers_to_a_pending_outer_resource_by_its_id() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", array_of(resource_ref("https://example.com/a"))),
            ])),
            ..ref_to("A")
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn resource_subschema_built_inside_a_resource_is_referred_to_from_the_document_by_its_defs_entry()
     {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("A", array_of(resource_ref("https://example.com/b"))),
                ("B", string_schema()),
            ])),
            ..object_schema(&[("via", ref_to("A")), ("direct", ref_to("B"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(object_schema(&[("via", via), ("direct", direct)]))
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_resource_root_schema_refers_to_itself_as_hash_and_stays_out_of_defs() {
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_resource_root_schema(
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
            )
            .map(GeneratedSchema::into_schema);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("Inner", string_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
                ("Outer", object_schema(&[("b", ref_to("Inner1"))])),
            ])),
            ..object_schema(&[("a", ref_to("Outer")), ("c", ref_to("Inner0"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_rewrites_ref_into_a_def_by_pointer_prefix() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("Inner", object_schema(&[("x", string_schema())]))])),
            ref_: Some("#/$defs/Inner/properties/x".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let _: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| integer_schema());
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("Inner0", integer_schema()),
                ("Inner1", object_schema(&[("x", string_schema())])),
            ])),
            ref_: Some("#/$defs/Inner1/properties/x".to_string()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschema_escapes_pointer_tokens_in_def_keys() {
        let standalone: JsonSchema = JsonSchema {
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        };
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("a/b~c", string_schema())])),
            ref_: Some("#/$defs/a~1b~0c".to_string()),
            ..Default::default()
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(inlined)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_conflicting_bodies_are_all_suffixed() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let first: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let second: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", integer_schema()));
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[
                ("Inner0", string_schema()),
                ("Inner1", integer_schema()),
            ])),
            ..object_schema(&[("first", ref_to("Inner0")), ("second", ref_to("Inner1"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_defs_with_equal_bodies_from_different_schemas_merge() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_def_equal_to_named_type_merges_into_it() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let named: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| string_schema());
        let hoisted: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let body: JsonSchema = object_schema(&[("hoisted", hoisted), ("named", named)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[("Inner", string_schema())])),
            ..object_schema(&[("hoisted", ref_to("Inner")), ("named", ref_to("Inner"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(JsonSchema {
            title: Some("Second".to_string()),
            ..standalone
        });
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn inlining_an_equal_recursive_schema_twice_reuses_its_definitions() {
        let standalone: JsonSchema =
            standalone_with_def("List", object_schema(&[("next", ref_to("List"))]));
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let first: JsonSchema = generator.inline_subschema(standalone.clone());
        let second: JsonSchema = generator.inline_subschema(standalone);
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            defs: Some(defs(&[(
                "List",
                object_schema(&[("next", ref_to("List"))]),
            )])),
            ..object_schema(&[("first", ref_to("List")), ("second", ref_to("List"))])
        });
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(body)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_root_schema_without_defs_leaves_defs_none() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
            ..string_schema()
        });
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_root_schema(string_schema())
            .map(GeneratedSchema::into_schema);
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
                root_schema_for::<Self>().map(GeneratedSchema::into_schema)
            }

            fn json_schema_with(generator: &mut SchemaGenerator) -> JsonSchema {
                generator.named_subschema("a::Named", "Named", |_| string_schema())
            }
        }
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let first: JsonSchema = cycle_member(&mut generator, members, 0);
        generator
            .into_root_schema(object_schema(&[(property, first)]))
            .map(GeneratedSchema::into_schema)
    }

    /// Why `id` is not a URI reference, as the generator reports it.
    fn invalid_id_reason(id: &str) -> String {
        super::resource_uri::place(&super::resource_uri::placeholder_bases(1), id)
            .expect_err("not a URI reference")
    }

    #[test]
    fn ref_between_absolute_ids_is_the_id_as_written() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(object_schema(&[("ping", ping)]))
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_between_relative_ids_in_the_same_directory_is_the_id_as_written() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let first: JsonSchema = cycle_member(&mut generator, &members, 0);
        let second: JsonSchema = cycle_member(&mut generator, &members, 1);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(object_schema(&[("ping", first), ("pong", second)]))
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn absolute_id_root_embeds_relative_ids_once_and_they_refer_back_to_it() {
        let expected: ReverseCodeGenResult = Ok(JsonSchema {
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_resource_root_schema(
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
            )
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    /// The root schema of a document whose properties refer to the derived types `nodes`
    /// (`(schema_id, id, body)`, all named `Node`), by schema id.
    fn nodes_document(nodes: &[(&str, &str, JsonSchema)]) -> ReverseCodeGenResult {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let properties: Vec<(&str, JsonSchema)> = nodes
            .iter()
            .map(|(schema_id, id, body)| {
                let reference: JsonSchema =
                    generator.resource_subschema(schema_id, "Node", id, |_| body.clone());
                (*schema_id, reference)
            })
            .collect();
        generator
            .into_root_schema(object_schema(&properties))
            .map(GeneratedSchema::into_schema)
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
            schema: Some(SCHEMA_2020_12.to_string()),
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
        let actual: ReverseCodeGenResult = SchemaGenerator::default()
            .into_resource_root_schema("a::Root", "Root", "not a uri", |_| {
                with_id("not a uri", string_schema())
            })
            .map(GeneratedSchema::into_schema);
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
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        generator.report_errors(vec![reported_error()]);
        let expected: ReverseCodeGenResult = Err(vec![reported_error()]);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(string_schema())
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn into_root_schema_returns_reported_errors_before_resource_errors() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
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
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
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

    // --- ReverseCodeGenSettings: dialect, layout, meta schema, number formats ---

    /// `settings` applied to `{"properties": {"a": <ref to A>}}` plus `extensions`, with `A` a
    /// string type, serialized.
    fn written(
        settings: ReverseCodeGenSettings,
        extensions: BTreeMap<String, serde_json::Value>,
    ) -> Result<String, Vec<ReverseCodeGenError>> {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings);
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
        let body: JsonSchema = JsonSchema {
            extensions,
            ..object_schema(&[("a", reference)])
        };
        serialized(generator.into_root_schema(body))
    }

    /// `settings` applied to a bare string schema, serialized.
    fn written_string(
        settings: ReverseCodeGenSettings,
    ) -> Result<String, Vec<ReverseCodeGenError>> {
        serialized(SchemaGenerator::new(settings).into_root_schema(string_schema()))
    }

    fn settings_for(dialect: TargetDialect) -> ReverseCodeGenSettings {
        ReverseCodeGenSettings {
            dialect: Some(dialect),
            ..ReverseCodeGenSettings::default()
        }
    }

    #[test]
    fn default_settings_write_2020_12_with_defs() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","$defs":{"A":{"type":"string"}},"properties":{"a":{"$ref":"#/$defs/A"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written(ReverseCodeGenSettings::default(), BTreeMap::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft_07_writes_definitions_and_its_schema_uri() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"A":{"type":"string"}},"properties":{"a":{"$ref":"#/definitions/A"}},"type":"object"}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written(settings_for(TargetDialect::Draft07), BTreeMap::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_31_writes_components_schemas_and_no_schema_uri() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"type":"object","properties":{"a":{"$ref":"#/components/schemas/A"}},"components":{"schemas":{"A":{"type":"string"}}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written(settings_for(TargetDialect::OpenApi31), BTreeMap::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_30_without_definitions_still_has_no_schema_uri() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written_string(settings_for(TargetDialect::OpenApi30));
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_custom_definitions_path_places_the_definitions_and_their_refs() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"a":{"$ref":"#/x/y/A"}},"x":{"y":{"A":{"type":"string"}}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/x/y".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            BTreeMap::new(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_ignores_one_leading_hash_and_one_trailing_slash() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"a":{"$ref":"#/x/A"}},"x":{"A":{"type":"string"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("#/x/".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            BTreeMap::new(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_token_is_unescaped_for_the_key_and_kept_escaped_in_refs() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"a":{"$ref":"#/a~1b/A"}},"a/b":{"A":{"type":"string"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/a~1b".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            BTreeMap::new(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_definitions_key_is_a_valid_path_in_2020_12() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","definitions":{"A":{"type":"string"}},"properties":{"a":{"$ref":"#/definitions/A"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/definitions".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            BTreeMap::new(),
        );
        assert_eq!(expected, actual);
    }

    /// The error of the one invalid `path` for `dialect`.
    fn invalid_path(dialect: Option<TargetDialect>, path: &str) -> Vec<ReverseCodeGenError> {
        written(
            ReverseCodeGenSettings {
                dialect,
                definitions_path: Some(path.to_string()),
                ..ReverseCodeGenSettings::default()
            },
            BTreeMap::new(),
        )
        .expect_err("the path is invalid")
    }

    fn invalid_path_error(path: &str, reason: &str) -> Vec<ReverseCodeGenError> {
        vec![ReverseCodeGenError::InvalidDefinitionsPath {
            path: path.to_string(),
            reason: reason.to_string(),
        }]
    }

    #[test]
    fn an_empty_definitions_path_is_invalid() {
        let expected: Vec<Vec<ReverseCodeGenError>> = vec![
            invalid_path_error("", "it is empty"),
            invalid_path_error("#", "it is empty"),
            invalid_path_error("/", "it is empty"),
            invalid_path_error("#/", "it is empty"),
        ];
        let actual: Vec<Vec<ReverseCodeGenError>> = ["", "#", "/", "#/"]
            .into_iter()
            .map(|path: &str| invalid_path(None, path))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_must_start_with_a_slash() {
        let expected: Vec<ReverseCodeGenError> =
            invalid_path_error("x/y", "a JSON Pointer starts with `/`");
        let actual: Vec<ReverseCodeGenError> = invalid_path(None, "x/y");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_cannot_have_an_empty_token() {
        let expected: Vec<ReverseCodeGenError> =
            invalid_path_error("/x//y", "it has an empty token");
        let actual: Vec<ReverseCodeGenError> = invalid_path(None, "/x//y");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_cannot_have_a_bad_tilde_escape() {
        let expected: Vec<Vec<ReverseCodeGenError>> = vec![
            invalid_path_error("/a~2", "`~` must be followed by `0` or `1`"),
            invalid_path_error("/a~", "`~` must be followed by `0` or `1`"),
        ];
        let actual: Vec<Vec<ReverseCodeGenError>> = ["/a~2", "/a~"]
            .into_iter()
            .map(|path: &str| invalid_path(None, path))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_cannot_start_with_a_keyword_of_the_target() {
        let expected: Vec<ReverseCodeGenError> = invalid_path_error(
            "/properties/x",
            "`properties` is a keyword of draft-07, not a place for definitions",
        );
        let actual: Vec<ReverseCodeGenError> =
            invalid_path(Some(TargetDialect::Draft07), "/properties/x");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_cannot_start_with_a_keyword_the_target_lacks_but_2020_12_has() {
        let expected: Vec<ReverseCodeGenError> = invalid_path_error(
            "/const",
            "`const` is a keyword of OpenAPI 3.0, not a place for definitions",
        );
        let actual: Vec<ReverseCodeGenError> =
            invalid_path(Some(TargetDialect::OpenApi30), "/const");
        assert_eq!(expected, actual);
    }

    #[test]
    fn defs_is_not_a_definitions_path_where_the_target_has_no_defs() {
        let expected: Vec<ReverseCodeGenError> =
            invalid_path_error("/$defs", "`$defs` is not a keyword of draft-07");
        let actual: Vec<ReverseCodeGenError> = invalid_path(Some(TargetDialect::Draft07), "/$defs");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_through_an_existing_non_object_is_occupied() {
        let extensions: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("x".to_string(), serde_json::json!(1))]);
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Err(vec![ReverseCodeGenError::DefinitionsPathOccupied {
                path: "/x/y".to_string(),
            }]);
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/x/y".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            extensions,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_that_already_holds_a_non_object_is_occupied() {
        let extensions: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("x".to_string(), serde_json::json!([]))]);
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Err(vec![ReverseCodeGenError::DefinitionsPathOccupied {
                path: "/x".to_string(),
            }]);
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/x".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            extensions,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_through_an_existing_object_merges_into_it() {
        let extensions: BTreeMap<String, serde_json::Value> =
            BTreeMap::from([("x".to_string(), serde_json::json!({"note": 1}))]);
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"a":{"$ref":"#/x/y/A"}},"x":{"note":1,"y":{"A":{"type":"string"}}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = written(
            ReverseCodeGenSettings {
                definitions_path: Some("/x/y".to_string()),
                ..ReverseCodeGenSettings::default()
            },
            extensions,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_resource_cannot_sit_under_a_custom_definitions_path() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(ReverseCodeGenSettings {
            definitions_path: Some("/x".to_string()),
            ..ReverseCodeGenSettings::default()
        });
        let reference: JsonSchema =
            generator.resource_subschema("a::A", "A", "https://example.com/a", |_| {
                with_id("https://example.com/a", string_schema())
            });
        let expected: ReverseCodeGenResult =
            Err(vec![ReverseCodeGenError::ResourceOutsideDefinitions {
                path: "/x".to_string(),
                schema_id: "a::A".to_string(),
            }]);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_resource_sits_in_the_defs_of_2020_12_and_the_definitions_of_draft_07() {
        let resource = |dialect: TargetDialect| -> Result<String, Vec<ReverseCodeGenError>> {
            let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
            let reference: JsonSchema =
                generator.resource_subschema("a::A", "A", "https://example.com/a", |_| {
                    with_id("https://example.com/a", string_schema())
                });
            serialized(generator.into_root_schema(reference))
        };
        let expected: Vec<Result<String, Vec<ReverseCodeGenError>>> = vec![
            Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/A","$defs":{"A":{"$id":"https://example.com/a","type":"string"}}}"##.to_string()),
            Ok(r##"{"$schema":"http://json-schema.org/draft-07/schema#","allOf":[{"$ref":"#/definitions/A"}],"definitions":{"A":{"$id":"https://example.com/a","type":"string"}}}"##.to_string()),
            Ok(r##"{"$schema":"http://json-schema.org/draft-04/schema#","allOf":[{"$ref":"#/definitions/A"}],"definitions":{"A":{"id":"https://example.com/a","type":"string"}}}"##.to_string()),
        ];
        let actual: Vec<Result<String, Vec<ReverseCodeGenError>>> = vec![
            resource(TargetDialect::Draft202012),
            resource(TargetDialect::Draft07),
            resource(TargetDialect::Draft04),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_meta_schema_can_be_omitted() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"string"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written_string(ReverseCodeGenSettings {
                meta_schema: MetaSchema::Omit,
                ..ReverseCodeGenSettings::default()
            });
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_meta_schema_can_be_an_absolute_uri() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"$schema":"https://example.com/meta","type":"string"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written_string(ReverseCodeGenSettings {
                meta_schema: MetaSchema::Uri("https://example.com/meta".to_string()),
                ..ReverseCodeGenSettings::default()
            });
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_relative_meta_schema_uri_is_an_error() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Err(vec![ReverseCodeGenError::InvalidMetaSchemaUri {
                uri: "meta.json".to_string(),
                reason: "it has no scheme (it is a relative reference)".to_string(),
            }]);
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            written_string(ReverseCodeGenSettings {
                meta_schema: MetaSchema::Uri("meta.json".to_string()),
                ..ReverseCodeGenSettings::default()
            });
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_meta_schema_is_the_roots_only() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.inline_subschema(JsonSchema {
            schema: Some("http://json-schema.org/draft-07/schema#".to_string()),
            ..standalone_with_def("A", string_schema())
        });
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/A","$defs":{"A":{"type":"string"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(reference));
        assert_eq!(expected, actual);
    }

    /// The `format` of each number type under `formats`, in the order i8, i16, i32, i64, u8,
    /// u16, u32, u64, f32, f64.
    fn number_formats(formats: Option<NumberFormats>) -> Vec<Option<String>> {
        let settings: ReverseCodeGenSettings = ReverseCodeGenSettings {
            number_formats: formats,
            ..ReverseCodeGenSettings::default()
        };
        let format = |result: GeneratedSchemaResult| -> Option<String> {
            result
                .expect("a number type has a schema")
                .into_schema()
                .format
        };
        let generate = |generator: SchemaGenerator| generator;
        vec![
            format(generate(SchemaGenerator::new(settings.clone())).into_root_schema_for::<i8>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<i16>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<i32>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<i64>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<u8>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<u16>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<u32>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<u64>()),
            format(SchemaGenerator::new(settings.clone()).into_root_schema_for::<f32>()),
            format(SchemaGenerator::new(settings).into_root_schema_for::<f64>()),
        ]
    }

    #[test]
    fn number_types_have_no_format_by_default() {
        let expected: Vec<Option<&str>> = vec![None; 10];
        let formats: Vec<Option<String>> = number_formats(None);
        let actual: Vec<Option<&str>> = formats.iter().map(Option::as_deref).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_types_have_openapi_formats() {
        let expected: Vec<Option<&str>> = vec![
            Some("int32"),
            Some("int32"),
            Some("int32"),
            Some("int64"),
            Some("int32"),
            Some("int32"),
            Some("int64"),
            None,
            Some("float"),
            Some("double"),
        ];
        let formats: Vec<Option<String>> = number_formats(Some(NumberFormats::OpenApi));
        let actual: Vec<Option<&str>> = formats.iter().map(Option::as_deref).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_types_have_schemars_formats() {
        let expected: Vec<Option<&str>> = vec![
            Some("int8"),
            Some("int16"),
            Some("int32"),
            Some("int64"),
            Some("uint8"),
            Some("uint16"),
            Some("uint32"),
            Some("uint64"),
            Some("float"),
            Some("double"),
        ];
        let formats: Vec<Option<String>> = number_formats(Some(NumberFormats::Schemars));
        let actual: Vec<Option<&str>> = formats.iter().map(Option::as_deref).collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_number_format_is_part_of_the_whole_schema() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"integer","minimum":0,"maximum":255,"format":"uint8"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(
            SchemaGenerator::new(ReverseCodeGenSettings {
                number_formats: Some(NumberFormats::Schemars),
                ..ReverseCodeGenSettings::default()
            })
            .into_root_schema_for::<u8>(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_types_reached_through_a_container_have_formats() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"array","items":{"type":"integer","minimum":-2147483648,"maximum":2147483647,"format":"int32"}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(
            SchemaGenerator::new(ReverseCodeGenSettings {
                number_formats: Some(NumberFormats::OpenApi),
                ..ReverseCodeGenSettings::default()
            })
            .into_root_schema_for::<Vec<i32>>(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_standalone_number_schema_has_no_format() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r#"{"type":"integer","minimum":-2147483648,"maximum":2147483647}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(i32::json_schema());
        assert_eq!(expected, actual);
    }

    // --- inline_subschemas ---

    fn inlining() -> ReverseCodeGenSettings {
        ReverseCodeGenSettings {
            inline_subschemas: true,
            ..ReverseCodeGenSettings::default()
        }
    }

    #[test]
    fn inline_subschemas_writes_a_named_type_where_it_is_used() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(inlining());
        let first: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
        let second: JsonSchema =
            generator.named_subschema("a::A", "A", |_| panic!("an inlined type is built once"));
        let body: JsonSchema = object_schema(&[("first", first), ("second", second)]);
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"first":{"type":"string"},"second":{"type":"string"}}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(body));
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschemas_keeps_a_ref_and_a_definition_for_a_recursive_type() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(inlining());
        let reference: JsonSchema =
            generator.named_subschema("a::Node", "Node", |generator: &mut SchemaGenerator| {
                JsonSchema {
                    items: Some(
                        generator
                            .named_subschema("a::Node", "Node", |_| {
                                panic!("a pending type is not rebuilt")
                            })
                            .into(),
                    ),
                    ..Default::default()
                }
            });
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/Node","$defs":{"Node":{"items":{"$ref":"#/$defs/Node"}}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(reference));
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschemas_inlines_a_type_nested_in_an_inlined_type() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(inlining());
        let outer: JsonSchema =
            generator.named_subschema("a::Outer", "Outer", |generator: &mut SchemaGenerator| {
                object_schema(&[(
                    "inner",
                    generator.named_subschema("a::Inner", "Inner", |_| string_schema()),
                )])
            });
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"inner":{"type":"string"}}}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(outer));
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschemas_keeps_hand_written_definitions() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(inlining());
        let inlined: JsonSchema =
            generator.inline_subschema(standalone_with_def("Inner", string_schema()));
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/Inner","$defs":{"Inner":{"type":"string"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(inlined));
        assert_eq!(expected, actual);
    }

    #[test]
    fn inline_subschemas_never_inlines_a_resource() {
        let mut generator: SchemaGenerator = SchemaGenerator::new(inlining());
        let reference: JsonSchema =
            generator.resource_subschema("a::A", "A", "https://example.com/a", |_| {
                with_id("https://example.com/a", string_schema())
            });
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$ref":"#/$defs/A","$defs":{"A":{"$id":"https://example.com/a","type":"string"}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(reference));
        assert_eq!(expected, actual);
    }

    // --- dialect conversion of a whole document ---

    /// `body` (written as 2020-12) as `dialect`.
    fn converted_document(
        dialect: TargetDialect,
        body: serde_json::Value,
    ) -> Result<String, Vec<ReverseCodeGenError>> {
        let body: JsonSchema = serde_json::from_value(body).expect("a schema object");
        serialized(SchemaGenerator::new(settings_for(dialect)).into_root_schema(body))
    }

    #[test]
    fn a_hand_written_root_is_converted_to_the_target() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r#"{"$schema":"http://json-schema.org/draft-04/schema#","exclusiveMinimum":true,"minimum":0,"type":"integer"}"#.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = converted_document(
            TargetDialect::Draft04,
            serde_json::json!({"type": "integer", "exclusiveMinimum": 0}),
        );
        assert_eq!(expected, actual);
    }

    /// The 2020-12 model of `body` (written as 2020-12) generated for `dialect`.
    fn model_document(dialect: TargetDialect, body: serde_json::Value) -> JsonSchema {
        let body: JsonSchema = serde_json::from_value(body).expect("a schema object");
        SchemaGenerator::new(settings_for(dialect))
            .into_root_schema(body)
            .expect("a representable schema")
            .into_schema()
    }

    #[test]
    fn the_model_of_a_converted_document_stays_2020_12() {
        let expected: JsonSchema = serde_json::from_value(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "integer",
            "exclusiveMinimum": 0
        }))
        .expect("a schema object");
        let actual: JsonSchema = model_document(
            TargetDialect::Draft04,
            serde_json::json!({"type": "integer", "exclusiveMinimum": 0}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_model_of_a_draft_07_document_keeps_its_definitions_in_defs_where_its_refs_resolve() {
        let expected: Option<BTreeMap<String, Schema>> = Some(BTreeMap::from([(
            "A".to_string(),
            Schema::from(string_schema()),
        )]));
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft07));
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
        let actual: Option<BTreeMap<String, Schema>> = generator
            .into_root_schema(object_schema(&[("a", reference)]))
            .expect("a representable schema")
            .into_schema()
            .defs;
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_model_of_a_document_without_a_meta_schema_has_none() {
        let expected: Option<String> = None;
        let body: JsonSchema = string_schema();
        let actual: Option<String> = SchemaGenerator::new(ReverseCodeGenSettings {
            dialect: Some(TargetDialect::Draft04),
            meta_schema: MetaSchema::Omit,
            ..ReverseCodeGenSettings::default()
        })
        .into_root_schema(body)
        .expect("a representable schema")
        .into_schema()
        .schema;
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_native_document_is_its_own_target_form() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "string"
        });
        let actual: serde_json::Value = SchemaGenerator::default()
            .into_root_schema(string_schema())
            .expect("a schema")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_converted_document_writes_the_target_form_as_a_value() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "exclusiveMinimum": true,
            "minimum": 0,
            "type": "integer"
        });
        let body: JsonSchema =
            serde_json::from_value(serde_json::json!({"type": "integer", "exclusiveMinimum": 0}))
                .expect("a schema object");
        let actual: serde_json::Value = SchemaGenerator::new(settings_for(TargetDialect::Draft04))
            .into_root_schema(body)
            .expect("a representable schema")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_root_ref_is_wrapped_in_all_of_when_definitions_come_beside_it() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"http://json-schema.org/draft-07/schema#","allOf":[{"$ref":"#/definitions/A"}],"definitions":{"A":{"type":"string"}}}"##.to_string());
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft07));
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(reference));
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_root_ref_is_wrapped_in_all_of_when_only_the_meta_schema_comes_beside_it() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"http://json-schema.org/draft-07/schema#","allOf":[{"$ref":"#/definitions/Missing"}]}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> = converted_document(
            TargetDialect::Draft07,
            serde_json::json!({"$ref": "#/$defs/Missing"}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_lone_root_ref_stays_without_siblings_when_nothing_comes_beside_it() {
        let expected: Result<String, Vec<ReverseCodeGenError>> =
            Ok(r##"{"$ref":"#/definitions/Missing"}"##.to_string());
        let body: JsonSchema =
            serde_json::from_value(serde_json::json!({"$ref": "#/$defs/Missing"}))
                .expect("a schema object");
        let actual: Result<String, Vec<ReverseCodeGenError>> = serialized(
            SchemaGenerator::new(ReverseCodeGenSettings {
                dialect: Some(TargetDialect::Draft07),
                meta_schema: MetaSchema::Omit,
                ..ReverseCodeGenSettings::default()
            })
            .into_root_schema(body),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_path_cannot_start_with_an_openapi_30_keyword() {
        let expected: Vec<ReverseCodeGenError> = invalid_path_error(
            "/discriminator/x",
            "`discriminator` is a keyword of OpenAPI 3.0, not a place for definitions",
        );
        let actual: Vec<ReverseCodeGenError> =
            invalid_path(Some(TargetDialect::OpenApi30), "/discriminator/x");
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_unrepresentable_keyword_is_reported_together() {
        let expected: Result<String, Vec<ReverseCodeGenError>> = Err(vec![
            ReverseCodeGenError::UnrepresentableKeyword {
                dialect: TargetDialect::Draft07,
                keyword: "unevaluatedProperties".to_string(),
                pointer: String::new(),
            },
            ReverseCodeGenError::UnrepresentableKeyword {
                dialect: TargetDialect::Draft07,
                keyword: "minContains".to_string(),
                pointer: "/properties/a".to_string(),
            },
        ]);
        let actual: Result<String, Vec<ReverseCodeGenError>> = converted_document(
            TargetDialect::Draft07,
            serde_json::json!({
                "unevaluatedProperties": false,
                "properties": {"a": {"minContains": 1}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn definitions_are_converted_with_their_pointer_under_the_definitions_path() {
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft07));
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            unique_items: Some(true),
            extensions: BTreeMap::from([("if".to_string(), serde_json::json!(true))]),
            ..Default::default()
        });
        let expected: Result<String, Vec<ReverseCodeGenError>> = Ok(r##"{"$schema":"http://json-schema.org/draft-07/schema#","allOf":[{"$ref":"#/definitions/A"}],"definitions":{"A":{"if":true,"uniqueItems":true}}}"##.to_string());
        let actual: Result<String, Vec<ReverseCodeGenError>> =
            serialized(generator.into_root_schema(reference));
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_keyword_missing_from_the_target_inside_a_definition_names_its_pointer() {
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft04));
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            extensions: BTreeMap::from([("if".to_string(), serde_json::json!(true))]),
            ..Default::default()
        });
        let expected: ReverseCodeGenResult =
            Err(vec![ReverseCodeGenError::UnrepresentableKeyword {
                dialect: TargetDialect::Draft04,
                keyword: "if".to_string(),
                pointer: "/definitions/A".to_string(),
            }]);
        let actual: ReverseCodeGenResult = generator
            .into_root_schema(reference)
            .map(GeneratedSchema::into_schema);
        assert_eq!(expected, actual);
    }

    // --- A hand-written root that already holds an entry the generator also hoists ---

    /// The document of `settings` for a hand-written root `body_json` (a schema in the JSON of
    /// the 2020-12 model, with `a` unset) plus `a`, a reference to a hoisted string type named
    /// `A`.
    fn with_hand_written_entry(
        settings: ReverseCodeGenSettings,
        body_json: serde_json::Value,
    ) -> serde_json::Value {
        generated_with_hand_written_entry(settings, body_json)
            .expect("generated")
            .to_value()
    }

    fn generated_with_hand_written_entry(
        settings: ReverseCodeGenSettings,
        body_json: serde_json::Value,
    ) -> GeneratedSchemaResult {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings);
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
        let mut body: JsonSchema = serde_json::from_value(body_json).expect("a schema");
        body.properties
            .insert("a".to_string(), Schema::from(reference));
        generator.into_root_schema(body)
    }

    fn instance_is_valid(document: &serde_json::Value, instance: &serde_json::Value) -> bool {
        let schema: Schema = Schema::try_from(document.to_string().as_str()).expect("schema");
        crate::validate(&schema, instance).is_ok()
    }

    #[test]
    fn hoisted_definition_equal_to_a_hand_written_defs_entry_merges_into_it() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"A": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/A"}, "user": {"$ref": "#/$defs/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            ReverseCodeGenSettings::default(),
            serde_json::json!({
                "type": "object",
                "$defs": {"A": {"type": "string"}},
                "properties": {"user": {"$ref": "#/$defs/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_differing_from_a_hand_written_defs_entry_gets_a_suffix() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"A": {"type": "integer"}, "A0": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/A0"}, "user": {"$ref": "#/$defs/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            ReverseCodeGenSettings::default(),
            serde_json::json!({
                "type": "object",
                "$defs": {"A": {"type": "integer"}},
                "properties": {"user": {"$ref": "#/$defs/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn suffix_avoids_every_hand_written_defs_name() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {
                "A": {"type": "integer"},
                "A0": {"type": "boolean"},
                "A1": {"type": "string"}
            },
            "properties": {"a": {"$ref": "#/$defs/A1"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            ReverseCodeGenSettings::default(),
            serde_json::json!({
                "type": "object",
                "$defs": {"A": {"type": "integer"}, "A0": {"type": "boolean"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn refs_resolve_to_the_right_schema_after_a_defs_collision() {
        let document: serde_json::Value = with_hand_written_entry(
            ReverseCodeGenSettings::default(),
            serde_json::json!({
                "type": "object",
                "$defs": {"A": {"type": "integer"}},
                "properties": {"user": {"$ref": "#/$defs/A"}}
            }),
        );
        let expected: Vec<bool> = vec![true, false, false];
        let actual: Vec<bool> = [
            serde_json::json!({"a": "x", "user": 5}),
            serde_json::json!({"a": 5, "user": 5}),
            serde_json::json!({"a": "x", "user": "x"}),
        ]
        .iter()
        .map(|instance: &serde_json::Value| instance_is_valid(&document, instance))
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_equal_to_a_hand_written_definitions_entry_merges_into_it() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"A": {"type": "string"}},
            "properties": {"a": {"$ref": "#/definitions/A"}, "user": {"$ref": "#/definitions/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({
                "type": "object",
                "definitions": {"A": {"type": "string"}},
                "properties": {"user": {"$ref": "#/definitions/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_differing_from_a_hand_written_definitions_entry_gets_a_suffix() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"A": {"type": "integer"}, "A0": {"type": "string"}},
            "properties": {"a": {"$ref": "#/definitions/A0"}, "user": {"$ref": "#/definitions/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({
                "type": "object",
                "definitions": {"A": {"type": "integer"}},
                "properties": {"user": {"$ref": "#/definitions/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_differing_from_a_hand_written_defs_entry_gets_a_suffix_in_draft_07() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"A": {"type": "integer"}, "A0": {"type": "string"}},
            "properties": {"a": {"$ref": "#/definitions/A0"}, "user": {"$ref": "#/definitions/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({
                "type": "object",
                "$defs": {"A": {"type": "integer"}},
                "properties": {"user": {"$ref": "#/$defs/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn refs_resolve_to_the_right_schema_after_a_draft_07_collision() {
        let document: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({
                "type": "object",
                "definitions": {"A": {"type": "integer"}},
                "properties": {"user": {"$ref": "#/definitions/A"}}
            }),
        );
        let expected: Vec<bool> = vec![true, false, false];
        let actual: Vec<bool> = [
            serde_json::json!({"a": "x", "user": 5}),
            serde_json::json!({"a": 5, "user": 5}),
            serde_json::json!({"a": "x", "user": "x"}),
        ]
        .iter()
        .map(|instance: &serde_json::Value| instance_is_valid(&document, instance))
        .collect();
        assert_eq!(expected, actual);
    }

    fn custom_path_settings() -> ReverseCodeGenSettings {
        ReverseCodeGenSettings {
            definitions_path: Some("/x/y".to_string()),
            ..ReverseCodeGenSettings::default()
        }
    }

    #[test]
    fn hoisted_definition_equal_to_a_hand_written_custom_path_entry_merges_into_it() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "x": {"y": {"A": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/x/y/A"}, "user": {"$ref": "#/x/y/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            custom_path_settings(),
            serde_json::json!({
                "type": "object",
                "x": {"y": {"A": {"type": "string"}}},
                "properties": {"user": {"$ref": "#/x/y/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_differing_from_a_hand_written_custom_path_entry_gets_a_suffix() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "x": {"y": {"A": {"type": "integer"}, "A0": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/x/y/A0"}, "user": {"$ref": "#/x/y/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            custom_path_settings(),
            serde_json::json!({
                "type": "object",
                "x": {"y": {"A": {"type": "integer"}}},
                "properties": {"user": {"$ref": "#/x/y/A"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn hoisted_definition_equal_to_a_hand_written_entry_with_a_ref_merges_into_it() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let inner: JsonSchema = generator.named_subschema("a::Inner", "Inner", |_| string_schema());
        let outer: JsonSchema = generator.named_subschema("a::Outer", "Outer", |_| inner);
        let body: JsonSchema = JsonSchema {
            defs: Some(defs(&[
                (
                    "Outer",
                    JsonSchema {
                        ref_: Some("#/$defs/Inner".to_string()),
                        ..JsonSchema::default()
                    },
                ),
                ("Inner", string_schema()),
            ])),
            ..outer
        };
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": "#/$defs/Outer",
            "$defs": {"Inner": {"type": "string"}, "Outer": {"$ref": "#/$defs/Inner"}}
        });
        let actual: serde_json::Value = serde_json::to_value(
            generator
                .into_root_schema(body)
                .expect("generated")
                .into_schema(),
        )
        .expect("json");
        assert_eq!(expected, actual);
    }

    // --- Review round: converted targets, recursion, container paths, resources, ids ---

    fn path_settings(dialect: Option<TargetDialect>, path: &str) -> ReverseCodeGenSettings {
        ReverseCodeGenSettings {
            dialect,
            definitions_path: Some(path.to_string()),
            ..ReverseCodeGenSettings::default()
        }
    }

    #[test]
    fn a_converted_target_with_a_custom_path_never_merges_into_an_unconverted_entry() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "x": {"A": {"type": "string"}, "A0": {"type": "string"}},
            "properties": {"a": {"$ref": "#/x/A0"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(Some(TargetDialect::Draft07), "/x"),
            serde_json::json!({"type": "object", "x": {"A": {"type": "string"}}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_native_target_with_a_custom_path_still_merges() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "x": {"A": {"type": "string"}},
            "properties": {"a": {"$ref": "#/x/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(None, "/x"),
            serde_json::json!({"type": "object", "x": {"A": {"type": "string"}}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_converted_definitions_target_merges_into_an_equal_defs_entry_in_the_model() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"A": {"type": "string"}},
            "properties": {"a": {"$ref": "#/$defs/A"}}
        });
        let generated: GeneratedSchema = generated_with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({"type": "object", "$defs": {"A": {"type": "string"}}}),
        )
        .expect("generated");
        let actual: serde_json::Value = serde_json::to_value(generated.schema()).expect("json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_converted_definitions_target_merges_into_an_equal_defs_entry_in_the_document() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"A": {"type": "string"}},
            "properties": {"a": {"$ref": "#/definitions/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::Draft07),
            serde_json::json!({"type": "object", "$defs": {"A": {"type": "string"}}}),
        );
        assert_eq!(expected, actual);
    }

    // --- A hand-written `$defs` entry that refs a hoisted definition, on a draft-07 target ---

    /// A root whose `$defs` entry `C` refs the hoisted definition `B`, which the root's own
    /// `$defs` does not hold.
    fn generated_with_ref_to_hoisted_definition(
        settings: ReverseCodeGenSettings,
    ) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings);
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"B": {"type": "string"}},
            "$ref": "#/$defs/B"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"C": {"$ref": "#/$defs/B"}}
        }))
        .expect("a schema");
        body.properties
            .insert("b".to_string(), Schema::from(inlined));
        generator.into_root_schema(body).expect("generated")
    }

    #[test]
    fn a_defs_entry_ref_to_a_hoisted_definition_resolves_in_the_model_on_a_converted_target() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"B": {"type": "string"}, "C": {"$ref": "#/$defs/B"}},
            "properties": {"b": {"$ref": "#/$defs/B"}}
        });
        let generated: GeneratedSchema =
            generated_with_ref_to_hoisted_definition(settings_for(TargetDialect::Draft07));
        let actual: serde_json::Value = serde_json::to_value(generated.schema()).expect("json");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_defs_entry_ref_to_a_hoisted_definition_resolves_in_the_document_on_a_converted_target() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"B": {"type": "string"}, "C": {"$ref": "#/definitions/B"}},
            "properties": {"b": {"$ref": "#/definitions/B"}}
        });
        let actual: serde_json::Value =
            generated_with_ref_to_hoisted_definition(settings_for(TargetDialect::Draft07))
                .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_model_and_the_document_agree_on_instances_through_a_ref_to_a_hoisted_definition() {
        let generated: GeneratedSchema =
            generated_with_ref_to_hoisted_definition(settings_for(TargetDialect::Draft07));
        let model: serde_json::Value = serde_json::to_value(generated.schema()).expect("json");
        let document: serde_json::Value = generated.to_value();
        let instances: Vec<serde_json::Value> =
            vec![serde_json::json!({"b": "x"}), serde_json::json!({"b": 5})];
        let expected: Vec<bool> = vec![true, false];
        let through_model: Vec<bool> = instances
            .iter()
            .map(|instance: &serde_json::Value| instance_is_valid(&model, instance))
            .collect();
        let through_document: Vec<bool> = instances
            .iter()
            .map(|instance: &serde_json::Value| instance_is_valid(&document, instance))
            .collect();
        assert_eq!(
            (expected.clone(), expected),
            (through_model, through_document)
        );
    }

    #[test]
    fn a_generated_definition_ref_to_a_hoisted_definition_merges_into_an_equal_hand_written_entry()
    {
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft07));
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"B": {"type": "string"}},
            "$ref": "#/$defs/B"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            properties: BTreeMap::from([("x".to_string(), Schema::from(inlined.clone()))]),
            ..object_schema(&[])
        });
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"A": {"type": "object", "properties": {"x": {"$ref": "#/$defs/B"}}}}
        }))
        .expect("a schema");
        body.properties
            .insert("a".to_string(), Schema::from(reference));
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {
                "A": {"type": "object", "properties": {"x": {"$ref": "#/definitions/B"}}},
                "B": {"type": "string"}
            },
            "properties": {"a": {"$ref": "#/definitions/A"}}
        });
        let actual: serde_json::Value = generator
            .into_root_schema(body)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    fn array_type(schema: JsonSchema) -> JsonSchema {
        JsonSchema {
            type_: Some(JsonType::Array.into()),
            ..schema
        }
    }

    fn node_body(reference: &str) -> serde_json::Value {
        serde_json::json!({"type": "array", "items": {"$ref": reference}})
    }

    #[test]
    fn a_self_recursive_definition_equal_to_a_hand_written_entry_merges_into_it() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.named_subschema("a::Node", "Node", |generator| {
            let inner: JsonSchema = generator.named_subschema("a::Node", "Node", |_| {
                panic!("a pending type must not be rebuilt")
            });
            array_type(array_of(inner))
        });
        let body: JsonSchema = JsonSchema {
            defs: Some(
                serde_json::from_value(serde_json::json!({"Node": node_body("#/$defs/Node")}))
                    .expect("defs"),
            ),
            ..reference
        };
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": "#/$defs/Node",
            "$defs": {"Node": node_body("#/$defs/Node")}
        });
        let actual: serde_json::Value = generator
            .into_root_schema(body)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_mutually_recursive_pair_equal_to_hand_written_entries_merges_into_them() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |generator| {
            let b: JsonSchema = generator.named_subschema("a::B", "B", |generator| {
                let a: JsonSchema = generator.named_subschema("a::A", "A", |_| {
                    panic!("a pending type must not be rebuilt")
                });
                array_type(array_of(a))
            });
            array_type(array_of(b))
        });
        let body: JsonSchema = JsonSchema {
            defs: Some(
                serde_json::from_value(serde_json::json!({
                    "A": node_body("#/$defs/B"),
                    "B": node_body("#/$defs/A")
                }))
                .expect("defs"),
            ),
            ..reference
        };
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": "#/$defs/A",
            "$defs": {"A": node_body("#/$defs/B"), "B": node_body("#/$defs/A")}
        });
        let actual: serde_json::Value = generator
            .into_root_schema(body)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_recursive_definition_differing_from_a_hand_written_entry_gets_a_suffix() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.named_subschema("a::Node", "Node", |generator| {
            let inner: JsonSchema = generator.named_subschema("a::Node", "Node", |_| {
                panic!("a pending type must not be rebuilt")
            });
            array_type(array_of(inner))
        });
        let body: JsonSchema = JsonSchema {
            defs: Some(
                serde_json::from_value(serde_json::json!({"Node": {"type": "string"}}))
                    .expect("defs"),
            ),
            ..reference
        };
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": "#/$defs/Node0",
            "$defs": {"Node": {"type": "string"}, "Node0": node_body("#/$defs/Node0")}
        });
        let actual: serde_json::Value = generator
            .into_root_schema(body)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_container_prefixed_custom_path_merges_into_the_model_field() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"sub": {"A": {"type": "string"}}, "other": {"type": "integer"}},
            "properties": {"a": {"$ref": "#/$defs/sub/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(None, "/$defs/sub"),
            serde_json::json!({
                "type": "object",
                "$defs": {"sub": {"A": {"type": "string"}}, "other": {"type": "integer"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_container_prefixed_custom_path_suffixes_a_different_entry() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"sub": {"A": {"type": "integer"}, "A0": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/$defs/sub/A0"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(None, "/$defs/sub"),
            serde_json::json!({
                "type": "object",
                "$defs": {"sub": {"A": {"type": "integer"}}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_container_prefixed_custom_path_creates_the_container_entry() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {"sub": {"A": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/$defs/sub/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(None, "/$defs/sub"),
            serde_json::json!({"type": "object"}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_container_prefixed_custom_path_through_a_non_object_is_occupied() {
        let expected: GeneratedSchemaResult =
            Err(vec![ReverseCodeGenError::DefinitionsPathOccupied {
                path: "/$defs/sub".to_string(),
            }]);
        let actual: GeneratedSchemaResult = generated_with_hand_written_entry(
            path_settings(None, "/$defs/sub"),
            serde_json::json!({"type": "object", "$defs": {"sub": true}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_definitions_prefixed_custom_path_on_a_draft_merges_into_the_model_field() {
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "definitions": {"sub": {"A": {"type": "integer"}, "A0": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/definitions/sub/A0"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            path_settings(Some(TargetDialect::Draft07), "/definitions/sub"),
            serde_json::json!({
                "type": "object",
                "definitions": {"sub": {"A": {"type": "integer"}}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_hand_written_definitions_entry_in_a_resource_scope_is_avoided_on_a_converted_target() {
        let mut generator: SchemaGenerator =
            SchemaGenerator::new(settings_for(TargetDialect::Draft07));
        let reference: JsonSchema = generator.resource_subschema(
            "a::R",
            "R",
            "https://example.com/r",
            |generator: &mut SchemaGenerator| {
                let a: JsonSchema = generator.named_subschema("a::A", "A", |_| string_schema());
                let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
                    "$id": "https://example.com/r",
                    "type": "object",
                    "definitions": {"A": {"type": "integer"}}
                }))
                .expect("a schema");
                body.properties.insert("a".to_string(), Schema::from(a));
                body
            },
        );
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [{"$ref": "#/definitions/R"}],
            "definitions": {
                "R": {
                    "$id": "https://example.com/r",
                    "type": "object",
                    "definitions": {"A": {"type": "integer"}, "A0": {"type": "string"}},
                    "properties": {"a": {"$ref": "#/definitions/A0"}}
                }
            }
        });
        let actual: serde_json::Value = generator
            .into_root_schema(reference)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_suffixed_definition_with_a_hand_written_entrys_id_is_a_duplicate_resource_id() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            id: Some("https://x/a".to_string()),
            ..string_schema()
        });
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"A": {"$id": "https://x/a", "type": "integer"}}
        }))
        .expect("a schema");
        body.properties
            .insert("a".to_string(), Schema::from(reference));
        let expected: GeneratedSchemaResult = Err(vec![ReverseCodeGenError::DuplicateResourceId {
            first: "#/$defs/A".to_string(),
            first_id: "https://x/a".to_string(),
            second: "a::A".to_string(),
            second_id: "https://x/a".to_string(),
        }]);
        let actual: GeneratedSchemaResult = generator.into_root_schema(body);
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_31_components_schemas_merges_an_equal_entry() {
        let expected: serde_json::Value = serde_json::json!({
            "type": "object",
            "components": {"schemas": {"A": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/components/schemas/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::OpenApi31),
            serde_json::json!({
                "type": "object",
                "components": {"schemas": {"A": {"type": "string"}}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_31_components_schemas_suffixes_a_different_entry() {
        let expected: serde_json::Value = serde_json::json!({
            "type": "object",
            "components": {"schemas": {"A": {"type": "integer"}, "A0": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/components/schemas/A0"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::OpenApi31),
            serde_json::json!({
                "type": "object",
                "components": {"schemas": {"A": {"type": "integer"}}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_30_reports_a_hand_written_components_root_key() {
        // OpenAPI 3.0's Schema Object has no `components`, so a collision there is an error.
        let expected: GeneratedSchemaResult =
            Err(vec![ReverseCodeGenError::UnrepresentableKeyword {
                dialect: TargetDialect::OpenApi30,
                keyword: "components".to_string(),
                pointer: String::new(),
            }]);
        let actual: GeneratedSchemaResult = generated_with_hand_written_entry(
            settings_for(TargetDialect::OpenApi30),
            serde_json::json!({
                "type": "object",
                "components": {"schemas": {"A": {"type": "string"}}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn openapi_30_components_schemas_holds_the_definitions_without_a_collision() {
        let expected: serde_json::Value = serde_json::json!({
            "type": "object",
            "components": {"schemas": {"A": {"type": "string"}}},
            "properties": {"a": {"$ref": "#/components/schemas/A"}}
        });
        let actual: serde_json::Value = with_hand_written_entry(
            settings_for(TargetDialect::OpenApi30),
            serde_json::json!({"type": "object"}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_colliding_name_that_needs_escaping_keeps_its_refs_escaped() {
        let mut generator: SchemaGenerator = SchemaGenerator::default();
        let reference: JsonSchema =
            generator.named_subschema("a::Slash", "a/b", |_| string_schema());
        let tilde: JsonSchema = generator.named_subschema("a::Tilde", "a~b", |_| string_schema());
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"a/b": {"type": "integer"}, "a~b": {"type": "string"}}
        }))
        .expect("a schema");
        body.properties
            .insert("s".to_string(), Schema::from(reference));
        body.properties.insert("t".to_string(), Schema::from(tilde));
        let expected: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "$defs": {
                "a/b": {"type": "integer"},
                "a/b0": {"type": "string"},
                "a~b": {"type": "string"}
            },
            "properties": {"s": {"$ref": "#/$defs/a~1b0"}, "t": {"$ref": "#/$defs/a~0b"}}
        });
        let actual: serde_json::Value = generator
            .into_root_schema(body)
            .expect("generated")
            .to_value();
        assert_eq!(expected, actual);
    }

    // --- Hand-written refs to generated definitions on a draft 4-7 target: `#/$defs/G` and
    // `#/definitions/G` both resolve, in the model and in the document ---

    /// The 2020-12 model of a root whose hand-written `container` holds `hand_written` and
    /// whose generated definitions are `generated` (kept in `$defs`).
    fn expected_model(
        container: &str,
        hand_written: serde_json::Value,
        generated: serde_json::Value,
        properties: serde_json::Value,
    ) -> serde_json::Value {
        let (serde_json::Value::Object(hand_written), serde_json::Value::Object(generated)) =
            (hand_written, generated)
        else {
            unreachable!("definition maps are objects");
        };
        let mut model: serde_json::Value = serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object"
        });
        model["properties"] = properties;
        let mut defs: serde_json::Map<String, serde_json::Value> = generated;
        if container == "$defs" {
            defs.extend(hand_written);
        } else {
            model["definitions"] = serde_json::Value::Object(hand_written);
        }
        model["$defs"] = serde_json::Value::Object(defs);
        model
    }

    /// A root whose hand-written `container` entry `C` and property `p` ref the hoisted
    /// definition `B` in the `#/<form>/B` form.
    fn generated_with_hand_written_ref_to_hoisted(
        dialect: TargetDialect,
        container: &str,
        form: &str,
    ) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"B": {"type": "string"}},
            "$ref": "#/$defs/B"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let reference: String = format!("#/{form}/B");
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            container: {"C": {"$ref": reference}},
            "properties": {"p": {"$ref": reference}}
        }))
        .expect("a schema");
        body.properties
            .insert("b".to_string(), Schema::from(inlined));
        generator.into_root_schema(body).expect("generated")
    }

    /// A generated type `A` whose `x` is the hoisted definition `B`, beside a hand-written
    /// `container` entry `A` equal to it that refs `B` in the `#/<form>/B` form.
    fn generated_with_equal_hand_written_entry(
        dialect: TargetDialect,
        container: &str,
        form: &str,
    ) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"B": {"type": "string"}},
            "$ref": "#/$defs/B"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            properties: BTreeMap::from([("x".to_string(), Schema::from(inlined.clone()))]),
            ..object_schema(&[])
        });
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            container: {"A": {"type": "object", "properties": {"x": {"$ref": format!("#/{form}/B")}}}}
        }))
        .expect("a schema");
        body.properties
            .insert("a".to_string(), Schema::from(reference));
        generator.into_root_schema(body).expect("generated")
    }

    /// The tests of one draft, one hand-written container and one ref form.
    macro_rules! hand_written_ref_tests {
        ($dialect:expr, $schema_uri:literal, $container:literal, $form:literal) => {
            use super::*;

            #[test]
            fn a_ref_to_a_hoisted_definition_resolves_in_the_model() {
                let expected: serde_json::Value = expected_model(
                    $container,
                    serde_json::json!({"C": {"$ref": "#/$defs/B"}}),
                    serde_json::json!({"B": {"type": "string"}}),
                    serde_json::json!({"b": {"$ref": "#/$defs/B"}, "p": {"$ref": "#/$defs/B"}}),
                );
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_hand_written_ref_to_hoisted($dialect, $container, $form)
                        .schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_ref_to_a_hoisted_definition_resolves_in_the_document() {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": $schema_uri,
                    "type": "object",
                    "definitions": {
                        "B": {"type": "string"},
                        "C": {"$ref": "#/definitions/B"}
                    },
                    "properties": {
                        "b": {"$ref": "#/definitions/B"},
                        "p": {"$ref": "#/definitions/B"}
                    }
                });
                let actual: serde_json::Value =
                    generated_with_hand_written_ref_to_hoisted($dialect, $container, $form)
                        .to_value();
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_on_instances_through_the_ref() {
                let generated: GeneratedSchema =
                    generated_with_hand_written_ref_to_hoisted($dialect, $container, $form);
                let model: serde_json::Value =
                    serde_json::to_value(generated.schema()).expect("json");
                let document: serde_json::Value = generated.to_value();
                let instances: Vec<serde_json::Value> =
                    vec![serde_json::json!({"p": "x"}), serde_json::json!({"p": 5})];
                let expected: Vec<Vec<bool>> = vec![vec![true, false], vec![true, false]];
                let actual: Vec<Vec<bool>> = [&model, &document]
                    .iter()
                    .map(|schema: &&serde_json::Value| {
                        instances
                            .iter()
                            .map(|instance: &serde_json::Value| instance_is_valid(schema, instance))
                            .collect()
                    })
                    .collect();
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_generated_definition_merges_into_an_equal_hand_written_entry_in_the_document() {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": $schema_uri,
                    "type": "object",
                    "definitions": {
                        "A": {"type": "object", "properties": {"x": {"$ref": "#/definitions/B"}}},
                        "B": {"type": "string"}
                    },
                    "properties": {"a": {"$ref": "#/definitions/A"}}
                });
                let actual: serde_json::Value =
                    generated_with_equal_hand_written_entry($dialect, $container, $form)
                        .to_value();
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_generated_definition_merges_into_an_equal_hand_written_entry_in_the_model() {
                let expected: serde_json::Value = expected_model(
                    $container,
                    serde_json::json!({
                        "A": {"type": "object", "properties": {"x": {"$ref": "#/$defs/B"}}}
                    }),
                    serde_json::json!({"B": {"type": "string"}}),
                    serde_json::json!({"a": {"$ref": concat!("#/", $container, "/A")}}),
                );
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_equal_hand_written_entry($dialect, $container, $form).schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }
        };
    }

    macro_rules! hand_written_ref_tests_per_draft {
        ($container:literal, $form:literal) => {
            mod draft_04 {
                hand_written_ref_tests!(
                    TargetDialect::Draft04,
                    "http://json-schema.org/draft-04/schema#",
                    $container,
                    $form
                );
            }
            mod draft_06 {
                hand_written_ref_tests!(
                    TargetDialect::Draft06,
                    "http://json-schema.org/draft-06/schema#",
                    $container,
                    $form
                );
            }
            mod draft_07 {
                hand_written_ref_tests!(
                    TargetDialect::Draft07,
                    "http://json-schema.org/draft-07/schema#",
                    $container,
                    $form
                );
            }
        };
    }

    mod defs_entries_with_defs_refs {
        use super::*;
        hand_written_ref_tests_per_draft!("$defs", "$defs");
    }

    mod defs_entries_with_definitions_refs {
        use super::*;
        hand_written_ref_tests_per_draft!("$defs", "definitions");
    }

    mod definitions_entries_with_defs_refs {
        use super::*;
        hand_written_ref_tests_per_draft!("definitions", "$defs");
    }

    mod definitions_entries_with_definitions_refs {
        use super::*;
        hand_written_ref_tests_per_draft!("definitions", "definitions");
    }

    // --- A nested `$id` resource's own `#/definitions/G` refs resolve against the resource, so
    // the alias to the generated `G` leaves them alone ---

    /// A root that hoists a generated string `G` and holds a property `r`, an embedded resource
    /// with its own integer `G` that its property `a` refs as `#/definitions/G`.
    fn generated_with_resource_defining_its_own_definition(
        dialect: TargetDialect,
    ) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"G": {"type": "string"}},
            "$ref": "#/$defs/G"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let resource: JsonSchema = serde_json::from_value(serde_json::json!({
            "$id": "http://x.test/r",
            "definitions": {"G": {"type": "integer"}},
            "properties": {"a": {"$ref": "#/definitions/G"}}
        }))
        .expect("a schema");
        let mut body: JsonSchema = object_schema(&[]);
        body.properties
            .insert("g".to_string(), Schema::from(inlined));
        body.properties
            .insert("r".to_string(), Schema::from(resource));
        generator.into_root_schema(body).expect("generated")
    }

    /// A generated type `A` holding the resource of
    /// [`generated_with_resource_defining_its_own_definition`], beside a hand-written `$defs`
    /// entry `A` equal to it, with a hoisted `G` that the resource's ref must not alias.
    fn generated_with_equal_hand_written_entry_holding_a_resource(
        dialect: TargetDialect,
    ) -> GeneratedSchema {
        let resource: serde_json::Value = serde_json::json!({
            "$id": "http://x.test/r",
            "definitions": {"G": {"type": "integer"}},
            "properties": {"a": {"$ref": "#/definitions/G"}}
        });
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let standalone: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"G": {"type": "string"}},
            "$ref": "#/$defs/G"
        }))
        .expect("a schema");
        let inlined: JsonSchema = generator.inline_subschema(standalone);
        let held: JsonSchema = serde_json::from_value(resource.clone()).expect("a schema");
        let reference: JsonSchema = generator.named_subschema("a::A", "A", |_| JsonSchema {
            properties: BTreeMap::from([("r".to_string(), Schema::from(held))]),
            ..object_schema(&[])
        });
        let mut body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"A": {"type": "object", "properties": {"r": resource}}}
        }))
        .expect("a schema");
        body.properties
            .insert("g".to_string(), Schema::from(inlined));
        body.properties
            .insert("a".to_string(), Schema::from(reference));
        generator.into_root_schema(body).expect("generated")
    }

    #[test]
    fn a_generated_definition_holding_a_nested_resource_merges_into_its_equal_hand_written_entry() {
        let expected: serde_json::Value = serde_json::json!({"g": "#/$defs/G", "a": "#/$defs/A"});
        let model: serde_json::Value = serde_json::to_value(
            generated_with_equal_hand_written_entry_holding_a_resource(TargetDialect::Draft07)
                .schema(),
        )
        .expect("json");
        let actual: serde_json::Value = serde_json::json!({
            "g": model["properties"]["g"]["$ref"],
            "a": model["properties"]["a"]["$ref"]
        });
        assert_eq!(expected, actual);
    }

    macro_rules! resource_alias_tests {
        ($dialect:expr) => {
            use super::*;

            #[test]
            fn a_ref_inside_a_nested_resource_keeps_resolving_against_the_resource_in_the_model() {
                let expected: serde_json::Value = serde_json::json!({"a": {"$ref": "#/definitions/G"}});
                let model: serde_json::Value = serde_json::to_value(
                    generated_with_resource_defining_its_own_definition($dialect).schema(),
                )
                .expect("json");
                let actual: serde_json::Value = model["properties"]["r"]["properties"].clone();
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_on_instances_through_a_nested_resource_ref() {
                let generated: GeneratedSchema =
                    generated_with_resource_defining_its_own_definition($dialect);
                let model: serde_json::Value =
                    serde_json::to_value(generated.schema()).expect("json");
                let document: serde_json::Value = generated.to_value();
                let instances: Vec<serde_json::Value> = vec![
                    serde_json::json!({"r": {"a": 5}}),
                    serde_json::json!({"r": {"a": "x"}}),
                    serde_json::json!({"g": "x"}),
                    serde_json::json!({"g": 5}),
                ];
                let expected: Vec<Vec<bool>> = vec![vec![true, false, true, false]; 2];
                let actual: Vec<Vec<bool>> = [&model, &document]
                    .iter()
                    .map(|schema: &&serde_json::Value| {
                        instances
                            .iter()
                            .map(|instance: &serde_json::Value| instance_is_valid(schema, instance))
                            .collect()
                    })
                    .collect();
                assert_eq!(expected, actual);
            }
        };
    }

    mod nested_resource_ref_draft_04 {
        resource_alias_tests!(TargetDialect::Draft04);
    }

    mod nested_resource_ref_draft_07 {
        resource_alias_tests!(TargetDialect::Draft07);
    }

    // --- Every ref the model of a draft 4-7 target holds resolves where the converted document
    // resolves it: refs in generated bodies, percent-encoded names, and hand-written entries
    // of the container conversion joins ---

    /// The refs `alias_resolver` gives for a root with a hand-written `$defs`-only `H`, a
    /// `definitions`-only `D`, an `S` in both, and the generated definitions `G`, `A B` and
    /// `a/b`.
    #[test]
    fn the_model_alias_rewrites_each_ref_to_the_container_the_model_holds_it_in() {
        let body: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"H": {}, "S": {}},
            "definitions": {"D": {}, "S": {}}
        }))
        .expect("a schema");
        let hand_written: HandWrittenContainers =
            HandWrittenContainers::of(&body, ModelAliases::HandWrittenAndGenerated);
        let final_names: Vec<Option<String>> = vec![
            Some("G".to_string()),
            Some("A B".to_string()),
            Some("a/b".to_string()),
        ];
        let alias = alias_resolver(
            ModelAliases::HandWrittenAndGenerated,
            &final_names,
            &BTreeSet::new(),
            &hand_written,
        );
        let references: [&str; 13] = [
            "#/definitions/G",
            "#/definitions/G/properties/x",
            "#/definitions/A%20B",
            "#/definitions/a~1b",
            "#/definitions/a%7E1b",
            "#/definitions/H",
            "#/definitions/S",
            "#/definitions/D",
            "#/$defs/D",
            "#/$defs/H",
            "#/$defs/G",
            "#/definitions/A%2",
            "#/definitions/Missing",
        ];
        let expected: Vec<Option<String>> = vec![
            Some("#/$defs/G".to_string()),
            Some("#/$defs/G/properties/x".to_string()),
            Some("#/$defs/A%20B".to_string()),
            Some("#/$defs/a~1b".to_string()),
            Some("#/$defs/a%7E1b".to_string()),
            Some("#/$defs/H".to_string()),
            None,
            None,
            Some("#/definitions/D".to_string()),
            None,
            None,
            None,
            None,
        ];
        let actual: Vec<Option<String>> = references
            .iter()
            .map(|reference: &&str| alias(reference))
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_hand_written_model_alias_leaves_generated_definitions_alone() {
        let body: JsonSchema = serde_json::from_value(serde_json::json!({
            "$defs": {"H": {}}
        }))
        .expect("a schema");
        let hand_written: HandWrittenContainers =
            HandWrittenContainers::of(&body, ModelAliases::HandWritten);
        let final_names: Vec<Option<String>> = vec![Some("G".to_string())];
        let alias = alias_resolver(
            ModelAliases::HandWritten,
            &final_names,
            &BTreeSet::new(),
            &hand_written,
        );
        let expected: Vec<Option<String>> = vec![None, Some("#/$defs/H".to_string())];
        let actual: Vec<Option<String>> = vec![alias("#/definitions/G"), alias("#/definitions/H")];
        assert_eq!(expected, actual);
    }

    /// A generated `A` whose body refs the generated `B` as `#/definitions/B`, beside a root
    /// property `b` of `B`, and (with `twin`) a hand-written `definitions` entry `A` equal to it.
    fn generated_with_definitions_ref_in_a_generated_body(
        dialect: TargetDialect,
        twin: bool,
    ) -> GeneratedSchema {
        let a_body: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {"x": {"$ref": "#/definitions/B"}}
        });
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let a_schema: JsonSchema = serde_json::from_value(a_body.clone()).expect("a schema");
        let a: JsonSchema = generator.named_subschema("x::A", "A", |_| a_schema);
        let b: JsonSchema = generator.named_subschema("x::B", "B", |_| string_schema());
        let mut body: JsonSchema = object_schema(&[("a", a), ("b", b)]);
        if twin {
            body.definitions = Some(BTreeMap::from([(
                "A".to_string(),
                serde_json::from_value(a_body).expect("a schema"),
            )]));
        }
        generator.into_root_schema(body).expect("generated")
    }

    /// A generated `A B` that only the hand-written root property `p` refs, as
    /// `#/definitions/A%20B`.
    fn generated_with_percent_encoded_definitions_ref(dialect: TargetDialect) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        // Registers the definition; the generator's own ref to it is not used.
        let _: JsonSchema = generator.named_subschema("x::AB", "A B", |_| string_schema());
        let p: JsonSchema = serde_json::from_value(serde_json::json!({
            "$ref": "#/definitions/A%20B"
        }))
        .expect("a schema");
        let body: JsonSchema = object_schema(&[("p", p)]);
        generator.into_root_schema(body).expect("generated")
    }

    /// A hand-written root with the `$defs`-only string `H` and the `definitions`-only integer
    /// `D`, which its properties `h` and `d` ref through the other container's form.
    fn generated_with_cross_container_refs(settings: ReverseCodeGenSettings) -> GeneratedSchema {
        let body: JsonSchema = serde_json::from_value(serde_json::json!({
            "type": "object",
            "$defs": {"H": {"type": "string"}},
            "definitions": {"D": {"type": "integer"}},
            "properties": {
                "h": {"$ref": "#/definitions/H"},
                "d": {"$ref": "#/$defs/D"}
            }
        }))
        .expect("a schema");
        SchemaGenerator::new(settings)
            .into_root_schema(body)
            .expect("generated")
    }

    /// A generated string `G` (root property `g`) beside a generated resource `R` (root
    /// property `r`) whose own hand-written `definitions` entry `G` is an integer that its
    /// property `a` refs as `#/definitions/G`.
    fn generated_with_generated_resource_defining_its_own_definition(
        dialect: TargetDialect,
    ) -> GeneratedSchema {
        let mut generator: SchemaGenerator = SchemaGenerator::new(settings_for(dialect));
        let g: JsonSchema = generator.named_subschema("x::G", "G", |_| string_schema());
        let r: JsonSchema = generator.resource_subschema("x::R", "R", "http://x.test/r", |_| {
            serde_json::from_value(serde_json::json!({
                "$id": "http://x.test/r",
                "definitions": {"G": {"type": "integer"}},
                "properties": {"a": {"$ref": "#/definitions/G"}}
            }))
            .expect("a schema")
        });
        generator
            .into_root_schema(object_schema(&[("g", g), ("r", r)]))
            .expect("generated")
    }

    /// Whether each of `instances` is valid against the model and against the document of
    /// `generated`, in that order.
    fn model_and_document_verdicts(
        generated: &GeneratedSchema,
        instances: &[serde_json::Value],
    ) -> Vec<Vec<bool>> {
        let model: serde_json::Value = serde_json::to_value(generated.schema()).expect("json");
        let document: serde_json::Value = generated.to_value();
        [&model, &document]
            .iter()
            .map(|schema: &&serde_json::Value| {
                instances
                    .iter()
                    .map(|instance: &serde_json::Value| instance_is_valid(schema, instance))
                    .collect()
            })
            .collect()
    }

    /// The tests of one draft 4-7 target.
    macro_rules! model_alias_tests {
        ($dialect:expr, $schema_uri:literal) => {
            use super::*;

            #[test]
            fn a_definitions_ref_in_a_generated_body_resolves_in_the_model() {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "$defs": {
                        "A": {"type": "object", "properties": {"x": {"$ref": "#/$defs/B"}}},
                        "B": {"type": "string"}
                    },
                    "properties": {"a": {"$ref": "#/$defs/A"}, "b": {"$ref": "#/$defs/B"}}
                });
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_definitions_ref_in_a_generated_body($dialect, false).schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_through_a_definitions_ref_in_a_generated_body() {
                let instances: [serde_json::Value; 2] = [
                    serde_json::json!({"a": {"x": "s"}}),
                    serde_json::json!({"a": {"x": 5}}),
                ];
                let expected: Vec<Vec<bool>> = vec![vec![true, false]; 2];
                let actual: Vec<Vec<bool>> = model_and_document_verdicts(
                    &generated_with_definitions_ref_in_a_generated_body($dialect, false),
                    &instances,
                );
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_generated_body_with_a_definitions_ref_merges_into_its_equal_hand_written_twin_in_the_model(
            ) {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "definitions": {
                        "A": {"type": "object", "properties": {"x": {"$ref": "#/$defs/B"}}}
                    },
                    "$defs": {"B": {"type": "string"}},
                    "properties": {
                        "a": {"$ref": "#/definitions/A"},
                        "b": {"$ref": "#/$defs/B"}
                    }
                });
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_definitions_ref_in_a_generated_body($dialect, true).schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_generated_body_with_a_definitions_ref_merges_into_its_equal_hand_written_twin_in_the_document(
            ) {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": $schema_uri,
                    "type": "object",
                    "definitions": {
                        "A": {"type": "object", "properties": {"x": {"$ref": "#/definitions/B"}}},
                        "B": {"type": "string"}
                    },
                    "properties": {
                        "a": {"$ref": "#/definitions/A"},
                        "b": {"$ref": "#/definitions/B"}
                    }
                });
                let actual: serde_json::Value =
                    generated_with_definitions_ref_in_a_generated_body($dialect, true).to_value();
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_percent_encoded_definitions_ref_to_a_generated_definition_resolves_in_the_model() {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "$defs": {"A B": {"type": "string"}},
                    "properties": {"p": {"$ref": "#/$defs/A%20B"}}
                });
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_percent_encoded_definitions_ref($dialect).schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_through_a_percent_encoded_definitions_ref() {
                let instances: [serde_json::Value; 2] =
                    [serde_json::json!({"p": "s"}), serde_json::json!({"p": 5})];
                let expected: Vec<Vec<bool>> = vec![vec![true, false]; 2];
                let actual: Vec<Vec<bool>> = model_and_document_verdicts(
                    &generated_with_percent_encoded_definitions_ref($dialect),
                    &instances,
                );
                assert_eq!(expected, actual);
            }

            #[test]
            fn a_ref_to_a_hand_written_entry_through_the_other_container_resolves_in_the_model() {
                let expected: serde_json::Value = serde_json::json!({
                    "$schema": "https://json-schema.org/draft/2020-12/schema",
                    "type": "object",
                    "$defs": {"H": {"type": "string"}},
                    "definitions": {"D": {"type": "integer"}},
                    "properties": {
                        "h": {"$ref": "#/$defs/H"},
                        "d": {"$ref": "#/definitions/D"}
                    }
                });
                let actual: serde_json::Value = serde_json::to_value(
                    generated_with_cross_container_refs(settings_for($dialect)).schema(),
                )
                .expect("json");
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_through_a_ref_to_the_other_container() {
                let instances: [serde_json::Value; 4] = [
                    serde_json::json!({"h": "s"}),
                    serde_json::json!({"h": 5}),
                    serde_json::json!({"d": 5}),
                    serde_json::json!({"d": "s"}),
                ];
                let expected: Vec<Vec<bool>> = vec![vec![true, false, true, false]; 2];
                let actual: Vec<Vec<bool>> = model_and_document_verdicts(
                    &generated_with_cross_container_refs(settings_for($dialect)),
                    &instances,
                );
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_through_a_generated_resource_ref_to_its_own_entry(
            ) {
                let instances: [serde_json::Value; 4] = [
                    serde_json::json!({"r": {"a": 5}}),
                    serde_json::json!({"r": {"a": "s"}}),
                    serde_json::json!({"g": "s"}),
                    serde_json::json!({"g": 5}),
                ];
                let expected: Vec<Vec<bool>> = vec![vec![true, false, true, false]; 2];
                let actual: Vec<Vec<bool>> = model_and_document_verdicts(
                    &generated_with_generated_resource_defining_its_own_definition($dialect),
                    &instances,
                );
                assert_eq!(expected, actual);
            }

            #[test]
            fn the_model_and_the_document_agree_through_a_ref_to_the_other_container_under_a_custom_path(
            ) {
                let settings: ReverseCodeGenSettings = ReverseCodeGenSettings {
                    definitions_path: Some("/custom".to_string()),
                    ..settings_for($dialect)
                };
                let instances: [serde_json::Value; 4] = [
                    serde_json::json!({"h": "s"}),
                    serde_json::json!({"h": 5}),
                    serde_json::json!({"d": 5}),
                    serde_json::json!({"d": "s"}),
                ];
                let expected: Vec<Vec<bool>> = vec![vec![true, false, true, false]; 2];
                let actual: Vec<Vec<bool>> = model_and_document_verdicts(
                    &generated_with_cross_container_refs(settings),
                    &instances,
                );
                assert_eq!(expected, actual);
            }
        };
    }

    mod model_alias_draft_04 {
        model_alias_tests!(
            TargetDialect::Draft04,
            "http://json-schema.org/draft-04/schema#"
        );
    }

    mod model_alias_draft_06 {
        model_alias_tests!(
            TargetDialect::Draft06,
            "http://json-schema.org/draft-06/schema#"
        );
    }

    mod model_alias_draft_07 {
        model_alias_tests!(
            TargetDialect::Draft07,
            "http://json-schema.org/draft-07/schema#"
        );
    }
}
