//! Dereferencing: the root schema with every reference replaced by its target, for tools that
//! do not resolve references (see [`Registry::dereference`]).

use super::{Registry, Resolved, Scope};
use crate::json_schema::dialect::{Dialect, RefSiblings, SubschemaPosition};
use crate::json_schema::json_schema::SchemaRef;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt;

/// Why [`Registry::dereference`] gave no schema.
#[derive(Debug)]
#[non_exhaustive]
pub enum DereferenceError {
    /// A schema of another dialect than the root's: once inlined, it would be read with the
    /// root's dialect. [`Registry::bundle`] keeps each document's dialect.
    MixedDialects {
        /// The location of the schema of the other dialect, as an absolute URI.
        location: String,
    },
    /// A reference the registry did not resolve.
    UnknownRef {
        /// The reference as written.
        reference: String,
        /// The location of the schema holding the reference, as an absolute URI.
        location: String,
    },
    /// A document that cannot be serialized to JSON.
    Serialize(serde_json::Error),
}

impl fmt::Display for DereferenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DereferenceError::MixedDialects { location } => write!(
                f,
                "{location}: this schema's dialect differs from the root's, so it cannot be \
                 inlined (bundle the schema instead)"
            ),
            DereferenceError::UnknownRef {
                reference,
                location,
            } => write!(f, "{location}: reference \"{reference}\" is not resolved"),
            DereferenceError::Serialize(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DereferenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DereferenceError::Serialize(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for DereferenceError {
    fn from(error: serde_json::Error) -> Self {
        DereferenceError::Serialize(error)
    }
}

/// Annotation keywords a 2019-09+ `$ref` may sit beside and still be merged into its target:
/// they assert nothing and hold no subschema.
const MERGEABLE: &[&str] = &[
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "$comment",
];

/// The reference keywords, in the order a schema with several is split.
const REFERENCES: &[&str] = &["$ref", "$dynamicRef", "$recursiveRef"];

/// One schema still to visit: where it is in the output, where it came from, the scope it
/// sits in (before entering its own `$id`), and the dynamic scope that reached it.
struct Visit {
    out: Vec<String>,
    document: usize,
    source: Vec<String>,
    scope: Scope,
    /// The innermost entry of the dynamic scope, an index into the walk's chain.
    dynamic: Option<usize>,
}

/// Identifies a schema on the walk: its document, its location there, and the resources of
/// its dynamic scope that a dynamic reference could reach (outermost first, each once). Two
/// visits with the same identity read the same.
type Identity = (usize, Vec<String>, Vec<Scope>);

enum Step {
    Visit(Visit),
    /// Leaving a schema: it is no longer an ancestor.
    Leave(Identity),
}

impl Registry<'_> {
    /// The root schema with every reference replaced by the schema it reaches: one schema that
    /// reads the same without resolving any reference, for tools that cannot (code generators,
    /// form builders, documentation).
    ///
    /// - A `$ref` is replaced by its target, itself dereferenced. Beside other keywords
    ///   (2019-09 and later), the target is merged into the schema when the others are only
    ///   annotations it lacks (`title`, `description`, `default`, ...), and otherwise added to
    ///   the schema's `allOf`. In draft 4, 6 and 7 the keywords beside `$ref` are ignored, so
    ///   they are dropped.
    /// - A recursive reference, whose target is a schema it sits in, cannot be inlined: it
    ///   becomes a JSON Pointer reference to where that schema is in the output
    ///   (`{"$ref": "#/properties/children/items"}`). Dereferencing a recursive schema
    ///   therefore keeps those references; every other reference is gone.
    /// - `$defs` / `definitions` are dropped (nothing references them any more), and so are
    ///   every `$id` but the root's, `$anchor`, `$dynamicAnchor` and `$recursiveAnchor`: the
    ///   output is one schema resource.
    /// - A `$dynamicRef` or `$recursiveRef` is resolved in the dynamic scope that reaches it
    ///   and inlined as `$ref` is. A schema reached through different dynamic scopes that
    ///   resolve its dynamic references differently is copied once per scope, so the output
    ///   reads the same for every evaluation path.
    ///
    /// A target reached by several references is copied at each, so the output can be much
    /// larger than the input (exponentially, for schemas that reference shared definitions
    /// level after level). [`Registry::bundle`] is the lossless alternative: it keeps every
    /// reference and needs no copies.
    ///
    /// A registry built from the output gives every instance the verdict the original gives
    /// it; error and annotation locations change, as their keyword paths now run through the
    /// inlined schemas.
    ///
    /// ```
    /// use json_schema_rs::{JsonSchemaSettings, Registry, Schema};
    /// use serde_json::{Value, json};
    ///
    /// let settings: JsonSchemaSettings = JsonSchemaSettings::default();
    /// let root: Schema = Schema::new_from_serde_value(
    ///     &json!({"$id": "https://example.com/order", "properties": {"price": {"$ref": "money"}}}),
    ///     &settings,
    /// )
    /// .unwrap();
    /// let registry: Registry<'_> = Registry::builder()
    ///     .document("https://example.com/money", json!({"type": "number", "minimum": 0}))
    ///     .build(&root)
    ///     .unwrap();
    /// let dereferenced: Value = registry.dereference().unwrap();
    /// assert_eq!(
    ///     json!({
    ///         "$id": "https://example.com/order",
    ///         "properties": {"price": {"type": "number", "minimum": 0}}
    ///     }),
    ///     dereferenced
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`DereferenceError`] when a reference cannot be resolved, when a reached schema is of another dialect than the root, or when a document cannot be
    /// serialized to JSON.
    #[expect(
        clippy::too_many_lines,
        reason = "one walk: splitting it would thread its whole state through helpers"
    )]
    pub fn dereference(&self) -> Result<Value, DereferenceError> {
        let mut documents: Vec<Option<Value>> = vec![None; self.documents.len()];
        let mut output: Value = self.document_value(&mut documents, 0)?.clone();
        let root_scope: Scope = self.root_scope();
        let root_dialect: Dialect = match &output {
            Value::Object(object) => self.dialect(self.enter_raw(root_scope, object)),
            _ => self.dialect(root_scope),
        };
        // The root's own identifiers, which the output root keeps whatever it inlines.
        let identifiers: [&str; 3] = ["$schema", "$id", root_dialect.id_keyword()];
        let root_identifiers: Vec<(String, Value)> = match &output {
            Value::Object(object) => identifiers
                .iter()
                .enumerate()
                .filter(|(i, keyword)| !identifiers[..*i].contains(keyword))
                .filter_map(|(_, keyword)| {
                    Some(((*keyword).to_string(), object.get(*keyword)?.clone()))
                })
                .collect(),
            _ => Vec::new(),
        };
        // Schemas on the path from the root to the one being visited → where each is in the
        // output.
        let mut ancestors: HashMap<Identity, Vec<String>> = HashMap::new();
        // The dynamic scopes: each entry is a resource declaring a dynamic or recursive anchor
        // and the index of the entry outside it.
        let mut chain: Vec<(Scope, Option<usize>)> = Vec::new();
        let mut stack: Vec<Step> = vec![Step::Visit(Visit {
            out: Vec::new(),
            document: 0,
            source: Vec::new(),
            scope: root_scope,
            dynamic: None,
        })];
        while let Some(step) = stack.pop() {
            let visit: Visit = match step {
                Step::Visit(visit) => visit,
                Step::Leave(identity) => {
                    ancestors.remove(&identity);
                    continue;
                }
            };
            let Some(Value::Object(object)) = at_mut(&mut output, &visit.out) else {
                continue;
            };
            let scope: Scope = self.enter_raw(visit.scope, object);
            let dialect: Dialect = self.dialect(scope);
            let location = || -> String {
                self.location_uri(scope, &visit.source)
                    .unwrap_or_else(|| self.base_uri(scope).to_string())
            };
            if dialect != root_dialect {
                return Err(DereferenceError::MixedDialects {
                    location: location(),
                });
            }
            let dynamic: Option<usize> =
                if self.declares_dynamic_anchor(scope) && !in_chain(&chain, visit.dynamic, scope) {
                    chain.push((scope, visit.dynamic));
                    Some(chain.len() - 1)
                } else {
                    visit.dynamic
                };
            let scopes: Vec<Scope> = outermost_first(&chain, dynamic);
            let identity: Identity = (visit.document, visit.source.clone(), scopes.clone());
            if ancestors
                .insert(identity.clone(), visit.out.clone())
                .is_none()
            {
                stack.push(Step::Leave(identity));
            }
            strip_identifiers(object, dialect, visit.out.is_empty());
            let mut references: Vec<(&str, String)> = REFERENCES
                .iter()
                .filter(|keyword| **keyword == "$ref" || dialect.keyword(keyword).is_some())
                .filter_map(|keyword| match object.get(*keyword) {
                    Some(Value::String(reference)) => Some((*keyword, reference.clone())),
                    _ => None,
                })
                .collect();
            // A schema with several references keeps the first; each other moves into an
            // `allOf` entry of its own, visited as a subschema.
            // `allOf` entries this visit queues itself, which the walk over subschemas skips.
            let mut queued: Vec<usize> = Vec::new();
            for (keyword, reference) in references.drain(1.min(references.len())..) {
                object.remove(keyword);
                let index: usize = push_all_of(
                    object,
                    Value::Object(Map::from_iter([(
                        keyword.to_string(),
                        Value::String(reference),
                    )])),
                );
                queued.push(index);
                let mut source: Vec<String> = visit.source.clone();
                source.push(keyword.to_string());
                stack.push(Step::Visit(Visit {
                    out: child(&visit.out, &["allOf", &index.to_string()]),
                    document: visit.document,
                    source,
                    scope,
                    dynamic,
                }));
            }
            if let Some((keyword, reference)) = references.pop() {
                let unknown = || DereferenceError::UnknownRef {
                    reference: reference.clone(),
                    location: location(),
                };
                let resolved: Resolved<'_> =
                    self.resolve(scope, &reference).map_err(|_| unknown())?;
                let resolved: Resolved<'_> = match keyword {
                    "$dynamicRef" => self.dynamic_target(&reference, resolved, &scopes),
                    "$recursiveRef" => self.recursive_target(resolved, &scopes),
                    _ => resolved,
                };
                let target_document: usize = self.resources[resolved.scope.0].document;
                // The dynamic scope the target is reached with: its identity's last part, as
                // the target's own visit will compute it.
                let target_scopes: Vec<Scope> = {
                    let entered: Scope = match resolved.schema {
                        SchemaRef::Object(schema) => self.enter(resolved.scope, schema),
                        SchemaRef::Bool(_) => resolved.scope,
                    };
                    let mut scopes: Vec<Scope> = scopes.clone();
                    if self.declares_dynamic_anchor(entered) && !scopes.contains(&entered) {
                        scopes.push(entered);
                    }
                    scopes
                };
                let target_identity: Identity =
                    (target_document, resolved.pointer.to_vec(), target_scopes);
                object.remove(keyword);
                if let Some(ancestor) = ancestors.get(&target_identity) {
                    // Recursive: point at the ancestor where it is in the output.
                    let pointer: String = fragment(ancestor);
                    if dialect.ref_siblings == RefSiblings::Ignored {
                        object.clear();
                    }
                    object.insert("$ref".to_string(), Value::String(pointer));
                } else {
                    let target: Value = self
                        .document_value(&mut documents, target_document)?
                        .pointer(&pointer_string(resolved.pointer))
                        .cloned()
                        .ok_or_else(unknown)?;
                    let target_visit = |out: Vec<String>| -> Step {
                        Step::Visit(Visit {
                            out,
                            document: target_document,
                            source: resolved.pointer.to_vec(),
                            scope: resolved.scope,
                            dynamic,
                        })
                    };
                    if dialect.ref_siblings == RefSiblings::Ignored || object.is_empty() {
                        stack.push(target_visit(visit.out.clone()));
                        if let Some(slot) = at_mut(&mut output, &visit.out) {
                            *slot = target;
                        }
                        continue;
                    }
                    let mergeable: bool = target.is_object()
                        && object.keys().all(|key| {
                            visit.out.is_empty() && identifiers.contains(&key.as_str())
                                || MERGEABLE.contains(&key.as_str())
                                    && target.as_object().is_some_and(|t| !t.contains_key(key))
                        });
                    if mergeable {
                        let Value::Object(mut merged) = target else {
                            unreachable!("only an object target is mergeable");
                        };
                        // The target keeps its own identifiers while the walk reads it; the
                        // root's are restored at the end.
                        merged.extend(std::mem::take(object).into_iter().filter(|(key, _)| {
                            !(visit.out.is_empty() && identifiers.contains(&key.as_str()))
                        }));
                        stack.push(target_visit(visit.out.clone()));
                        if let Some(slot) = at_mut(&mut output, &visit.out) {
                            *slot = Value::Object(merged);
                        }
                        continue;
                    }
                    let index: usize = push_all_of(object, target);
                    queued.push(index);
                    stack.push(target_visit(child(
                        &visit.out,
                        &["allOf", &index.to_string()],
                    )));
                }
            }
            for path in subschema_paths(object, dialect) {
                if path[0] == "allOf" && queued.iter().any(|index| path[1] == index.to_string()) {
                    continue;
                }
                let mut source: Vec<String> = visit.source.clone();
                source.extend(path.iter().cloned());
                stack.push(Step::Visit(Visit {
                    out: child(
                        &visit.out,
                        &path.iter().map(String::as_str).collect::<Vec<_>>(),
                    ),
                    document: visit.document,
                    source,
                    scope,
                    dynamic,
                }));
            }
        }
        if let Value::Object(object) = &mut output {
            for keyword in identifiers {
                object.remove(keyword);
            }
            let mut out: Map<String, Value> =
                Map::with_capacity(object.len() + root_identifiers.len());
            out.extend(root_identifiers);
            out.extend(std::mem::take(object));
            *object = out;
        }
        Ok(output)
    }

    /// Whether the resource `scope` declares a `$dynamicAnchor`, or is a `$recursiveAnchor:
    /// true` resource: whether a dynamic reference could reach it through the dynamic scope.
    fn declares_dynamic_anchor(&self, scope: Scope) -> bool {
        !self.resources[scope.0].dynamic_anchors.is_empty()
            || self.resources[scope.0].refs.contains_key("#")
                && matches!(
                    self.resolve(scope, "#").map(|resolved| resolved.schema),
                    Ok(SchemaRef::Object(root))
                        if root.extensions.get("$recursiveAnchor") == Some(&Value::Bool(true))
                )
    }

    /// The target of a `$dynamicRef` whose static target is `resolved`, in the dynamic scope
    /// `scopes` (outermost first): when the static target declares the `$dynamicAnchor` the
    /// fragment names, the anchor of the outermost resource declaring it; otherwise the static
    /// target (2020-12 core §8.2.3.2).
    fn dynamic_target<'s>(
        &'s self,
        reference: &str,
        resolved: Resolved<'s>,
        scopes: &[Scope],
    ) -> Resolved<'s> {
        let Some(anchor) = reference
            .rsplit_once('#')
            .map(|(_, fragment)| fragment)
            .filter(|fragment| !fragment.is_empty() && !fragment.starts_with('/'))
        else {
            return resolved;
        };
        let bookended: bool = matches!(
            resolved.schema,
            SchemaRef::Object(target)
                if target.extensions.get("$dynamicAnchor").and_then(Value::as_str) == Some(anchor)
        );
        if !bookended {
            return resolved;
        }
        scopes
            .iter()
            .filter(|scope| self.has_dynamic_anchor(**scope, anchor))
            .find_map(|scope| self.resolve(*scope, &format!("#{anchor}")).ok())
            .unwrap_or(resolved)
    }

    /// The target of a `$recursiveRef` whose static target is `resolved`, in the dynamic scope
    /// `scopes` (outermost first): when the static target has `$recursiveAnchor: true`, the
    /// outermost resource that has it too; otherwise the static target (2019-09 core
    /// §8.2.4.2).
    fn recursive_target<'s>(&'s self, resolved: Resolved<'s>, scopes: &[Scope]) -> Resolved<'s> {
        let anchored = |schema: SchemaRef<'_>| -> bool {
            matches!(schema, SchemaRef::Object(root)
                if root.extensions.get("$recursiveAnchor") == Some(&Value::Bool(true)))
        };
        if !anchored(resolved.schema) {
            return resolved;
        }
        scopes
            .iter()
            .filter_map(|scope| self.resolve(*scope, "#").ok())
            .find(|target| anchored(target.schema))
            .unwrap_or(resolved)
    }

    /// The JSON of document `index`, serialized once.
    fn document_value<'v>(
        &self,
        documents: &'v mut [Option<Value>],
        index: usize,
    ) -> Result<&'v Value, serde_json::Error> {
        if documents[index].is_none() {
            let scope: Scope = self.documents[index].scope;
            let dialect: Dialect = match self.documents[index].root.schema_ref() {
                SchemaRef::Object(schema) => self.dialect(self.enter(scope, schema)),
                SchemaRef::Bool(_) => self.dialect(scope),
            };
            let mut value: Value = match self.documents[index].root.schema_ref() {
                SchemaRef::Bool(b) => Value::Bool(b),
                SchemaRef::Object(schema) => serde_json::to_value(schema)?,
            };
            super::bundle::restore_draft4_shapes(&mut value, dialect);
            documents[index] = Some(value);
        }
        Ok(documents[index].as_ref().expect("serialized above"))
    }
}

/// Removes what names a resource or a location inside one, which the single-resource output
/// no longer needs: `$id` (kept on the root, with `$schema`), the anchors, and
/// `$defs` / `definitions`.
fn strip_identifiers(object: &mut Map<String, Value>, dialect: Dialect, root: bool) {
    if !root {
        object.remove("$schema");
        object.remove("$id");
        object.remove(dialect.id_keyword());
    }
    for keyword in [
        "$anchor",
        "$dynamicAnchor",
        "$recursiveAnchor",
        "$defs",
        "definitions",
    ] {
        if dialect.keyword(keyword).is_some() {
            object.remove(keyword);
        }
    }
}

/// Appends `schema` to `object`'s `allOf` (creating it) and returns its index.
fn push_all_of(object: &mut Map<String, Value>, schema: Value) -> usize {
    let all_of: &mut Value = object
        .entry("allOf")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !all_of.is_array() {
        *all_of = Value::Array(Vec::new());
    }
    let Value::Array(items) = all_of else {
        unreachable!("made an array above");
    };
    items.push(schema);
    items.len() - 1
}

/// The paths (relative to `object`) of every subschema directly under it, by `dialect`'s
/// keyword table.
fn subschema_paths(object: &Map<String, Value>, dialect: Dialect) -> Vec<Vec<String>> {
    let mut paths: Vec<Vec<String>> = Vec::new();
    for (key, value) in object {
        let Some(position) = dialect.keyword(key).and_then(|k| k.subschemas) else {
            continue;
        };
        match (position, value) {
            (SubschemaPosition::Array | SubschemaPosition::SingleOrArray, Value::Array(items)) => {
                paths.extend((0..items.len()).map(|i| vec![key.clone(), i.to_string()]));
            }
            (SubschemaPosition::Map, Value::Object(map)) => {
                paths.extend(map.keys().map(|name| vec![key.clone(), name.clone()]));
            }
            (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, _) => {
                paths.push(vec![key.clone()]);
            }
            (SubschemaPosition::Array | SubschemaPosition::Map, _) => {}
        }
    }
    paths
}

/// `base` followed by `segments`.
fn child(base: &[String], segments: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(base.len() + segments.len());
    out.extend(base.iter().cloned());
    out.extend(segments.iter().map(|segment| (*segment).to_string()));
    out
}

/// The value at `path` in `value`.
fn at_mut<'v>(value: &'v mut Value, path: &[String]) -> Option<&'v mut Value> {
    let mut current: &mut Value = value;
    for segment in path {
        current = match current {
            Value::Object(map) => map.get_mut(segment)?,
            Value::Array(items) => items.get_mut(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// `path` as a JSON Pointer.
fn pointer_string(path: &[String]) -> String {
    let mut out: String = String::new();
    for segment in path {
        out.push('/');
        out.push_str(&segment.replace('~', "~0").replace('/', "~1"));
    }
    out
}

/// `path` as a URI fragment reference (`#/properties/a`).
fn fragment(path: &[String]) -> String {
    let mut out: String = String::from("#");
    crate::json_pointer::push_uri_fragment(path.iter().map(String::as_str), &mut out);
    out
}

/// Whether `scope` is in the dynamic scope whose innermost entry is `entry`.
fn in_chain(chain: &[(Scope, Option<usize>)], mut entry: Option<usize>, scope: Scope) -> bool {
    while let Some(index) = entry {
        if chain[index].0 == scope {
            return true;
        }
        entry = chain[index].1;
    }
    false
}

/// The resources of the dynamic scope whose innermost entry is `entry`, outermost first.
fn outermost_first(chain: &[(Scope, Option<usize>)], mut entry: Option<usize>) -> Vec<Scope> {
    let mut scopes: Vec<Scope> = Vec::new();
    while let Some(index) = entry {
        scopes.push(chain[index].0);
        entry = chain[index].1;
    }
    scopes.reverse();
    scopes
}

#[cfg(test)]
mod tests {
    use super::DereferenceError;
    use crate::json_schema::json_schema::Schema;
    use crate::json_schema::registry::Registry;
    use crate::validator::CompiledSchema;
    use serde_json::{Value, json};

    const ROOT_URI: &str = "https://example.com/schemas/root.json";

    /// The registry of `root`, read from [`ROOT_URI`], with `documents` available by URI.
    fn registry<'a>(root: &'a Schema, documents: &[(&str, Value)]) -> Registry<'a> {
        let mut builder = Registry::builder().base_uri(ROOT_URI);
        for (uri, document) in documents {
            builder = builder.document(*uri, document.clone());
        }
        builder.build(root).expect("build registry")
    }

    /// The dereferenced `root`, with `documents` available by URI.
    fn dereference(root: &Value, documents: &[(&str, Value)]) -> Result<Value, DereferenceError> {
        let schema: Schema = Schema::try_from(root).expect("parse root");
        registry(&schema, documents).dereference()
    }

    /// The verdict of each of `instances` against `root`, then against its dereferenced form.
    fn verdicts(
        root: &Value,
        documents: &[(&str, Value)],
        instances: &[Value],
    ) -> (Vec<bool>, Vec<bool>) {
        let schema: Schema = Schema::try_from(root).expect("parse root");
        let registry: Registry<'_> = registry(&schema, documents);
        let original: CompiledSchema = CompiledSchema::new(&registry).expect("compile");
        let dereferenced: Schema =
            Schema::try_from(&registry.dereference().expect("dereference")).expect("parse");
        let dereferenced: CompiledSchema = CompiledSchema::new(
            &Registry::builder()
                .base_uri(ROOT_URI)
                .build(&dereferenced)
                .expect("build dereferenced registry"),
        )
        .expect("compile dereferenced");
        (
            instances.iter().map(|i| original.is_valid(i)).collect(),
            instances.iter().map(|i| dereferenced.is_valid(i)).collect(),
        )
    }

    // --- $ref ---

    #[test]
    fn schema_without_references_dereferences_to_itself() {
        let root: Value = json!({"type": "object", "properties": {"a": {"type": "string"}}});
        let expected: Value = root.clone();
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_alone_is_replaced_by_its_target() {
        let root: Value = json!({"properties": {"home": {"$ref": "address.json"}}});
        let address: Value = json!({"type": "object", "required": ["city"]});
        let expected: Value = json!({
            "properties": {"home": {"type": "object", "required": ["city"]}}
        });
        let actual: Value = dereference(
            &root,
            &[("https://example.com/schemas/address.json", address)],
        )
        .expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn references_inside_a_target_are_replaced_too() {
        let root: Value = json!({"$ref": "#/$defs/a", "$defs": {
            "a": {"items": {"$ref": "#/$defs/b"}},
            "b": {"type": "integer"}
        }});
        let expected: Value = json!({"items": {"type": "integer"}});
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn target_reached_twice_is_copied_at_each_reference() {
        let root: Value = json!({
            "properties": {"a": {"$ref": "#/$defs/n"}, "b": {"$ref": "#/$defs/n"}},
            "$defs": {"n": {"type": "number"}}
        });
        let expected: Value = json!({
            "properties": {"a": {"type": "number"}, "b": {"type": "number"}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_beside_a_reference_are_merged_into_its_target() {
        let root: Value = json!({
            "properties": {"a": {"$ref": "#/$defs/n", "description": "An amount"}},
            "$defs": {"n": {"type": "number"}}
        });
        let expected: Value = json!({
            "properties": {"a": {"type": "number", "description": "An amount"}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotation_the_target_also_has_moves_the_target_into_all_of() {
        let root: Value = json!({
            "properties": {"a": {"$ref": "#/$defs/n", "description": "An amount"}},
            "$defs": {"n": {"type": "number", "description": "A number"}}
        });
        let expected: Value = json!({
            "properties": {"a": {
                "description": "An amount",
                "allOf": [{"type": "number", "description": "A number"}]
            }}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn assertion_beside_a_reference_moves_the_target_into_all_of() {
        let root: Value = json!({
            "properties": {"a": {"$ref": "#/$defs/n", "maximum": 10, "allOf": [{"minimum": 0}]}},
            "$defs": {"n": {"type": "number"}}
        });
        let expected: Value = json!({
            "properties": {"a": {"maximum": 10, "allOf": [{"minimum": 0}, {"type": "number"}]}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_drops_the_keywords_beside_a_reference() {
        let root: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"a": {"$ref": "#/definitions/n", "maximum": 10}},
            "definitions": {"n": {"type": "number"}}
        });
        let expected: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"a": {"type": "number"}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn several_references_in_one_schema_each_get_an_all_of_entry() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "properties": {"a": {"$ref": "#/$defs/n", "$dynamicRef": "#/$defs/m"}},
            "$defs": {"n": {"type": "number"}, "m": {"minimum": 0}}
        });
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "properties": {"a": {"allOf": [{"minimum": 0}, {"type": "number"}]}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    // --- Identifiers ---

    #[test]
    fn anchors_and_definitions_are_dropped() {
        let root: Value = json!({
            "$ref": "#positive",
            "$defs": {"p": {"$anchor": "positive", "exclusiveMinimum": 0}}
        });
        let expected: Value = json!({"exclusiveMinimum": 0});
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_keeps_its_id_and_schema_while_inlined_documents_lose_theirs() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "items": {"$ref": "money.json"}
        });
        let money: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/money.json",
            "type": "number"
        });
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "items": {"type": "number"}
        });
        let actual: Value =
            dereference(&root, &[("https://example.com/schemas/money.json", money)])
                .expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_that_is_a_reference_keeps_its_schema_keyword() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2019-09/schema",
            "$ref": "money.json"
        });
        let money: Value = json!({"type": "number"});
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2019-09/schema",
            "type": "number"
        });
        let actual: Value =
            dereference(&root, &[("https://example.com/schemas/money.json", money)])
                .expect("dereference");
        assert_eq!(expected, actual);
    }

    // --- Recursion ---

    #[test]
    fn recursive_reference_points_at_its_ancestor_in_the_output() {
        let root: Value = json!({
            "properties": {"tree": {"$ref": "#/$defs/node"}},
            "$defs": {"node": {"properties": {"children": {"items": {"$ref": "#/$defs/node"}}}}}
        });
        let expected: Value = json!({
            "properties": {"tree": {"properties": {"children": {
                "items": {"$ref": "#/properties/tree"}
            }}}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn recursive_reference_to_the_root_points_at_the_root() {
        let root: Value = json!({"properties": {"next": {"$ref": "#"}}});
        let expected: Value = json!({"properties": {"next": {"$ref": "#"}}});
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_recursive_reference_drops_its_siblings() {
        let root: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"next": {"$ref": "#", "maximum": 1}}
        });
        let expected: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {"next": {"$ref": "#"}}
        });
        let actual: Value = dereference(&root, &[]).expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn recursive_schema_keeps_its_verdicts() {
        let root: Value = json!({
            "properties": {"tree": {"$ref": "#/$defs/node"}},
            "$defs": {"node": {
                "required": ["value"],
                "properties": {"children": {"items": {"$ref": "#/$defs/node"}}}
            }}
        });
        let instances: Vec<Value> = vec![
            json!({"tree": {"value": 1, "children": [{"value": 2, "children": [{"value": 3}]}]}}),
            json!({"tree": {"value": 1, "children": [{"value": 2, "children": [{}]}]}}),
        ];
        let (expected, actual): (Vec<bool>, Vec<bool>) = verdicts(&root, &[], &instances);
        assert_eq!(expected, actual);
    }

    // --- Dynamic references ---

    #[test]
    fn dynamic_reference_resolves_to_the_outermost_dynamic_anchor() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "$ref": "list.json",
            "$defs": {"item": {"$dynamicAnchor": "item", "type": "string"}}
        });
        let list: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/list.json",
            "items": {"$dynamicRef": "#item"},
            "$defs": {"item": {"$dynamicAnchor": "item"}}
        });
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "items": {"type": "string"}
        });
        let actual: Value = dereference(&root, &[("https://example.com/schemas/list.json", list)])
            .expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn dynamic_reference_without_a_matching_anchor_resolves_statically() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "$ref": "list.json",
            "$defs": {"item": {"$dynamicAnchor": "item", "type": "string"}}
        });
        let list: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/list.json",
            "items": {"$dynamicRef": "#item"},
            "$defs": {"item": {"$anchor": "item", "type": "integer"}}
        });
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "items": {"type": "integer"}
        });
        let actual: Value = dereference(&root, &[("https://example.com/schemas/list.json", list)])
            .expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_reached_in_two_dynamic_scopes_is_copied_for_each() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "properties": {
                "names": {"$ref": "names.json"},
                "plain": {"$ref": "list.json"}
            }
        });
        let names: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/names.json",
            "$ref": "list.json",
            "$defs": {"item": {"$dynamicAnchor": "item", "type": "string"}}
        });
        let list: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/list.json",
            "items": {"$dynamicRef": "#item"},
            "$defs": {"item": {"$dynamicAnchor": "item"}}
        });
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/schemas/root.json",
            "properties": {
                "names": {"items": {"type": "string"}},
                "plain": {"items": {}}
            }
        });
        let actual: Value = dereference(
            &root,
            &[
                ("https://example.com/schemas/names.json", names),
                ("https://example.com/schemas/list.json", list),
            ],
        )
        .expect("dereference");
        assert_eq!(expected, actual);
    }

    #[test]
    fn recursive_reference_resolves_to_the_outermost_recursive_anchor() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2019-09/schema",
            "$id": "https://example.com/schemas/root.json",
            "$recursiveAnchor": true,
            "$ref": "tree.json",
            "required": ["name"]
        });
        let tree: Value = json!({
            "$schema": "https://json-schema.org/draft/2019-09/schema",
            "$id": "https://example.com/schemas/tree.json",
            "$recursiveAnchor": true,
            "properties": {"children": {"items": {"$recursiveRef": "#"}}}
        });
        let instances: Vec<Value> = vec![
            json!({"name": "a", "children": [{"name": "b", "children": [{"name": "c"}]}]}),
            json!({"name": "a", "children": [{"name": "b", "children": [{}]}]}),
        ];
        let (expected, actual): (Vec<bool>, Vec<bool>) = verdicts(
            &root,
            &[("https://example.com/schemas/tree.json", tree)],
            &instances,
        );
        assert_eq!(expected, actual);
    }

    // --- Errors ---

    #[test]
    fn reference_into_another_dialect_is_an_error() {
        let root: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "items": {"$ref": "old.json"}
        });
        let old: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "string"
        });
        let actual: Result<Value, DereferenceError> =
            dereference(&root, &[("https://example.com/schemas/old.json", old)]);
        assert!(
            matches!(
                &actual,
                Err(DereferenceError::MixedDialects { location })
                    if location == "https://example.com/schemas/old.json#"
            ),
            "{actual:?}"
        );
    }
}
