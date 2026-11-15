//! Bundling (2020-12 core §9.3): one document holding the root schema and every document its
//! references reach, each embedded as a schema resource (see [`Registry::bundle`]).

use super::{Node, Registry, Scope, Uri, resolve_uri, without_fragment};
use crate::json_schema::dialect::{Dialect, ExclusiveBounds, RefSiblings, SubschemaPosition};
use crate::json_schema::json_schema::SchemaRef;
use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};

/// One document of the bundle: where its value goes, what it is called there, and how its
/// references read.
struct Part {
    /// The document's index in the registry.
    document: usize,
    /// The URI of the document's root resource (its root `$id`, or else its retrieval URI),
    /// without fragment.
    canonical: Uri,
    /// The document's dialect (the dialect of its root resource).
    dialect: Dialect,
}

impl Registry<'_> {
    /// One document holding the root schema and, embedded in it, every document its
    /// references reach: a schema that needs no retrieval and no other files (2020-12 core
    /// §9.3, *Compound Documents*).
    ///
    /// - Each referenced document is embedded whole, as a schema resource with its own `$id`,
    ///   under the root's `$defs` (`definitions` when the root is draft 4, 6 or 7), keyed by
    ///   that `$id`. Documents reached through other embedded documents are embedded too, each
    ///   once; the standard metaschemas are not.
    /// - An embedded document's `$id` is its URI written relative to the root's base URI
    ///   (`types/address.json`) when they share a scheme and authority, or else absolute, so a
    ///   bundle of local files names no absolute path and reads the same wherever it is moved.
    /// - References are kept as written, as §9.3.1 requires, with one exception: a reference to
    ///   a document by the URI it was retrieved from, when its root `$id` names it otherwise,
    ///   is rewritten to that `$id` (the bundle has no resource at the retrieval URI).
    /// - An embedded document of another draft than the root's gets its `$schema`, and both
    ///   `id` and `$id` when the two drafts name a resource with different keywords.
    /// - A draft 4, 6 or 7 document whose root has `$ref` ignores the `$id` beside it, so the
    ///   `$ref` moves into `allOf` (`{"$id": ..., "allOf": [{"$ref": ...}], "definitions":
    ///   ...}`). The schema means the same; error locations gain `/allOf/0`.
    /// - A boolean document becomes an object: `true` as `{"$id": ...}`, `false` as
    ///   `{"$id": ..., "not": {}}`.
    ///
    /// A registry built from the bundle alone gives every instance the verdict the original
    /// documents give it. A root without an `$id` keeps its base URI from wherever the bundle
    /// is read, so an embedded document's reference back to the root expects the bundle where
    /// the root was.
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
    /// let bundle: Value = registry.bundle().unwrap();
    /// assert_eq!(
    ///     json!({
    ///         "$id": "https://example.com/order",
    ///         "properties": {"price": {"$ref": "money"}},
    ///         "$defs": {"money": {"$id": "money", "type": "number", "minimum": 0}}
    ///     }),
    ///     bundle
    /// );
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the [`serde_json::Error`] of a document that cannot be serialized to JSON.
    pub fn bundle(&self) -> Result<Value, serde_json::Error> {
        let parts: Vec<Part> = self.bundle_parts();
        // A document reached by its retrieval URI, which its root `$id` names otherwise.
        let renamed: HashMap<Uri, Uri> = parts
            .iter()
            .filter_map(|part| {
                let retrieval: Uri =
                    without_fragment(&self.resources[self.documents[part.document].scope.0].uri);
                (retrieval != part.canonical).then(|| (retrieval, part.canonical.clone()))
            })
            .collect();
        let root: &Part = &parts[0];
        let mut values: Vec<Value> = Vec::with_capacity(parts.len());
        for part in &parts {
            let mut value: Value = match self.documents[part.document].root.schema_ref() {
                SchemaRef::Bool(b) => Value::Bool(b),
                SchemaRef::Object(schema) => serde_json::to_value(schema)?,
            };
            restore_draft4_shapes(&mut value, part.dialect);
            if !renamed.is_empty() {
                rewrite_refs(&mut value, &part.canonical, part.dialect, &renamed);
            }
            values.push(value);
        }
        let mut values = values.into_iter();
        let Some(mut bundle) = values.next() else {
            unreachable!("a registry always has its root document");
        };
        if parts.len() == 1 {
            return Ok(bundle);
        }
        if root.dialect.ref_siblings == RefSiblings::Ignored {
            bundle = ref_into_all_of(bundle, None, root.dialect);
        }
        let container_key: &str = if root.dialect.keyword("$defs").is_some() {
            "$defs"
        } else {
            "definitions"
        };
        let Value::Object(object) = &mut bundle else {
            unreachable!("a boolean root references no document");
        };
        let container: &mut Value = object
            .entry(container_key)
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(container) = container else {
            unreachable!("the model holds `$defs` and `definitions` as maps");
        };
        for (part, value) in parts[1..].iter().zip(values) {
            let id: String = relative_uri(&root.canonical, &part.canonical);
            let value: Value = embedded(value, &id, part.dialect, root.dialect);
            let mut key: String = id.clone();
            let mut suffix: usize = 2;
            while container.contains_key(&key) {
                key = format!("{id}_{suffix}");
                suffix += 1;
            }
            container.insert(key, value);
        }
        Ok(bundle)
    }

    /// The documents of the bundle, in order: the root, then each document a reference
    /// reaches, breadth first (references in the order written). The standard metaschemas are
    /// left out: every implementation has them.
    fn bundle_parts(&self) -> Vec<Part> {
        let mut seen: Vec<bool> = vec![false; self.documents.len()];
        let mut order: Vec<usize> = Vec::new();
        let mut queue: VecDeque<usize> = VecDeque::from([0]);
        seen[0] = true;
        // Resources by document, in discovery order.
        let mut resources_of: Vec<Vec<usize>> = vec![Vec::new(); self.documents.len()];
        for (index, resource) in self.resources.iter().enumerate() {
            resources_of[resource.document].push(index);
        }
        while let Some(document) = queue.pop_front() {
            order.push(document);
            for &resource in &resources_of[document] {
                let mut refs: Vec<(&String, Scope)> = self.resources[resource]
                    .refs
                    .iter()
                    .map(|(written, target)| (written, target.scope))
                    .collect();
                refs.sort_unstable_by(|a, b| a.0.cmp(b.0));
                for (_, scope) in refs {
                    let target: usize = self.resources[scope.0].document;
                    if !seen[target] && self.documents[target].kind.is_some() {
                        seen[target] = true;
                        queue.push_back(target);
                    }
                }
            }
        }
        order
            .into_iter()
            .map(|document| {
                let scope: Scope = self.documents[document].scope;
                let scope: Scope = match self.documents[document].root.schema_ref() {
                    SchemaRef::Object(schema) => self.enter_node(scope, Node::Typed(schema)),
                    SchemaRef::Bool(_) => scope,
                };
                Part {
                    document,
                    canonical: without_fragment(&self.resources[scope.0].uri),
                    dialect: self.resources[scope.0].dialect,
                }
            })
            .collect()
    }
}

/// `value`, an embedded document of `dialect`, made a schema resource named `id` inside a root
/// of `root_dialect`.
fn embedded(value: Value, id: &str, dialect: Dialect, root_dialect: Dialect) -> Value {
    let mut object: Map<String, Value> = match value {
        Value::Object(object) => object,
        Value::Bool(true) => Map::new(),
        Value::Bool(false) => Map::from_iter([("not".to_string(), Value::Object(Map::new()))]),
        other => return other,
    };
    if dialect.ref_siblings == RefSiblings::Ignored && object.contains_key("$ref") {
        let Value::Object(wrapped) = ref_into_all_of(Value::Object(object), Some(id), dialect)
        else {
            unreachable!("an object stays an object");
        };
        object = wrapped;
    }
    // The id keyword of each draft, and a `$id` a draft-4 document wrote (which this crate
    // reads as its identifier too): all name the embedded resource.
    let mut keys: Vec<&str> = vec![dialect.id_keyword(), root_dialect.id_keyword()];
    if object.contains_key("$id") {
        keys.push("$id");
    }
    keys.sort_unstable();
    keys.dedup();
    let mut out: Map<String, Value> = Map::with_capacity(object.len() + 3);
    if dialect.spec_version != root_dialect.spec_version && !object.contains_key("$schema") {
        out.insert(
            "$schema".to_string(),
            Value::String(dialect.spec_version.schema_uri().to_string()),
        );
    }
    for key in keys {
        out.insert(key.to_string(), Value::String(id.to_string()));
    }
    for (key, value) in object {
        out.entry(key).or_insert(value);
    }
    Value::Object(out)
}

/// A draft 4–7 document whose root has `$ref`, which makes every sibling ignored (`$id` and
/// the bundle's container included), as `{"$schema", id, "allOf": [{"$ref"}], "definitions"}`:
/// the same schema, with `$ref` in a subschema of its own. The other siblings are dropped (they
/// had no effect), except `definitions`, which references may point into. Any other object is
/// returned as is.
fn ref_into_all_of(value: Value, id: Option<&str>, dialect: Dialect) -> Value {
    let Value::Object(mut object) = value else {
        return value;
    };
    let Some(reference) = object.remove("$ref") else {
        return Value::Object(object);
    };
    let mut out: Map<String, Value> = Map::new();
    if let Some(schema) = object.remove("$schema") {
        out.insert("$schema".to_string(), schema);
    }
    if let Some(id) = id {
        out.insert(
            dialect.id_keyword().to_string(),
            Value::String(id.to_string()),
        );
    }
    out.insert(
        "allOf".to_string(),
        Value::Array(vec![Value::Object(Map::from_iter([(
            "$ref".to_string(),
            reference,
        )]))]),
    );
    if let Some(definitions) = object.remove("definitions") {
        out.insert("definitions".to_string(), definitions);
    }
    Value::Object(out)
}

/// `target` written relative to `base` (both absolute, without fragment): the shortest
/// reference that resolves against `base` to `target` when the two share a scheme and
/// authority and `target` has a hierarchical path, otherwise `target` itself.
fn relative_uri(base: &Uri, target: &Uri) -> String {
    let absolute: String = target.as_str().to_string();
    let (Some(base_tail), Some(target_tail)) = (hierarchical_tail(base), hierarchical_tail(target))
    else {
        return absolute;
    };
    let base_prefix: &str = &base.as_str()[..base.as_str().len() - base_tail.len()];
    let target_prefix: &str = &target.as_str()[..target.as_str().len() - target_tail.len()];
    if base_prefix != target_prefix {
        return absolute;
    }
    let base_path: &str = base.path().as_str();
    let target_path: &str = target.path().as_str();
    let base_dirs: Vec<&str> = base_path.split('/').collect();
    let base_dirs: &[&str] = &base_dirs[..base_dirs.len() - 1];
    let target_segments: Vec<&str> = target_path.split('/').collect();
    let (target_dirs, file) = target_segments.split_at(target_segments.len() - 1);
    let common: usize = base_dirs
        .iter()
        .zip(target_dirs)
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative: String = String::new();
    for _ in common..base_dirs.len() {
        relative.push_str("../");
    }
    for dir in &target_dirs[common..] {
        relative.push_str(dir);
        relative.push('/');
    }
    relative.push_str(file[0]);
    if let Some(query) = target.query() {
        relative.push('?');
        relative.push_str(query.as_str());
    }
    // A first segment with `:` would read as a scheme; an empty reference names the base.
    if relative.is_empty()
        || relative
            .split('/')
            .next()
            .is_some_and(|first| first.contains(':'))
    {
        relative.insert_str(0, "./");
    }
    // The safety net: a reference that does not resolve back stays absolute.
    match resolve_uri(base, &relative) {
        Ok(resolved) if without_fragment(&resolved) == *target => relative,
        _ => absolute,
    }
}

/// The path and query of `uri`, when its path is hierarchical (starts with `/`).
fn hierarchical_tail(uri: &Uri) -> Option<&str> {
    let text: &str = uri.as_str();
    let path: &str = uri.path().as_str();
    if !path.starts_with('/') {
        return None;
    }
    let start: usize = text.len() - path.len() - uri.query().map_or(0, |q| q.as_str().len() + 1);
    Some(&text[start..])
}

/// Rewrites every reference in `value` (a document whose root resource is `base`) whose URI
/// without fragment is a key of `renamed` to the URI it maps to, written relative to the base
/// the reference is resolved against. Iterative.
fn rewrite_refs(value: &mut Value, base: &Uri, dialect: Dialect, renamed: &HashMap<Uri, Uri>) {
    let mut stack: Vec<(&mut Value, Uri)> = vec![(value, base.clone())];
    while let Some((node, base)) = stack.pop() {
        let Value::Object(object) = node else {
            continue;
        };
        let ignores_id: bool =
            dialect.ref_siblings == RefSiblings::Ignored && object.contains_key("$ref");
        let id: Option<&str> = if ignores_id {
            None
        } else {
            object
                .get("$id")
                .or_else(|| object.get(dialect.id_keyword()))
                .and_then(Value::as_str)
        };
        let base: Uri = match id.map(|id| resolve_uri(&base, id)) {
            Some(Ok(resolved)) => without_fragment(&resolved),
            _ => base,
        };
        for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let Some(Value::String(reference)) = object.get_mut(keyword) else {
                continue;
            };
            let Ok(resolved) = resolve_uri(&base, reference) else {
                continue;
            };
            if let Some(target) = renamed.get(&without_fragment(&resolved)) {
                let mut rewritten: String = relative_uri(&base, target);
                if let Some(fragment) = resolved.fragment() {
                    rewritten.push('#');
                    rewritten.push_str(fragment.as_str());
                }
                *reference = rewritten;
            }
        }
        push_subschemas(object, dialect, |child| stack.push((child, base.clone())));
    }
}

/// Calls `push` with every subschema directly under `object`, by `dialect`'s keyword table.
fn push_subschemas<'v>(
    object: &'v mut Map<String, Value>,
    dialect: Dialect,
    mut push: impl FnMut(&'v mut Value),
) {
    for (key, child) in object.iter_mut() {
        let Some(position) = dialect.keyword(key).and_then(|k| k.subschemas) else {
            continue;
        };
        match (position, child) {
            (SubschemaPosition::Array | SubschemaPosition::SingleOrArray, Value::Array(items)) => {
                items.iter_mut().for_each(&mut push);
            }
            (SubschemaPosition::Map, Value::Object(map)) => map.values_mut().for_each(&mut push),
            (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, child) => push(child),
            (SubschemaPosition::Array | SubschemaPosition::Map, _) => {}
        }
    }
}

/// Rewrites the shapes the model normalises draft 4 into back into draft 4's own, as `dialect`
/// says, so a draft-4 implementation reads the bundle: `$id` becomes `id` (an object with both
/// keeps both), and under [`ExclusiveBounds::BooleanModifier`] a numeric `exclusiveMinimum`
/// without `minimum` becomes `minimum` with `exclusiveMinimum: true` (likewise for
/// `exclusiveMaximum`). Iterative.
fn restore_draft4_shapes(value: &mut Value, dialect: Dialect) {
    let legacy_id: bool = dialect.id_keyword() == "id";
    let boolean_bounds: bool = dialect.exclusive_bounds == ExclusiveBounds::BooleanModifier;
    if !legacy_id && !boolean_bounds {
        return;
    }
    let mut stack: Vec<&mut Value> = vec![value];
    while let Some(node) = stack.pop() {
        let Value::Object(object) = node else {
            continue;
        };
        if legacy_id
            && !object.contains_key("id")
            && let Some(id) = object.remove("$id")
        {
            object.insert("id".to_string(), id);
        }
        if boolean_bounds {
            for (modifier, bound) in [
                ("exclusiveMinimum", "minimum"),
                ("exclusiveMaximum", "maximum"),
            ] {
                if object.get(modifier).is_some_and(Value::is_number)
                    && !object.contains_key(bound)
                    && let Some(limit) = object.remove(modifier)
                {
                    object.insert(bound.to_string(), limit);
                    object.insert(modifier.to_string(), Value::Bool(true));
                }
            }
        }
        push_subschemas(object, dialect, |child| stack.push(child));
    }
}

#[cfg(test)]
mod tests {
    use super::relative_uri;
    use crate::json_schema::json_schema::Schema;
    use crate::json_schema::registry::{OfflineRetriever, Registry, Uri, parse_uri};
    use crate::validator::CompiledSchema;
    use serde_json::{Value, json};

    const ROOT_URI: &str = "https://example.com/schemas/root.json";

    /// The bundle of `root`, read from [`ROOT_URI`], with `documents` available by URI.
    fn bundle(root: &Value, documents: &[(&str, Value)]) -> Value {
        let schema: Schema = Schema::try_from(root).expect("parse root");
        let mut builder = Registry::builder().base_uri(ROOT_URI);
        for (uri, document) in documents {
            builder = builder.document(*uri, document.clone());
        }
        let registry: Registry<'_> = builder.build(&schema).expect("build registry");
        registry.bundle().expect("bundle")
    }

    /// The verdict of each of `instances` against `root` (read from [`ROOT_URI`] with
    /// `documents`), then against its bundle alone, read from `bundle_uri` with no retrieval.
    fn verdicts(
        root: &Value,
        documents: &[(&str, Value)],
        bundle_uri: &str,
        instances: &[Value],
    ) -> (Vec<bool>, Vec<bool>) {
        let schema: Schema = Schema::try_from(root).expect("parse root");
        let mut builder = Registry::builder().base_uri(ROOT_URI);
        for (uri, document) in documents {
            builder = builder.document(*uri, document.clone());
        }
        let registry: Registry<'_> = builder.build(&schema).expect("build registry");
        let original: CompiledSchema = CompiledSchema::new(&registry).expect("compile");
        let bundled: Schema =
            Schema::try_from(&registry.bundle().expect("bundle")).expect("parse bundle");
        let registry: Registry<'_> = Registry::builder()
            .base_uri(bundle_uri)
            .retriever(OfflineRetriever)
            .build(&bundled)
            .expect("build bundle registry");
        let bundled: CompiledSchema = CompiledSchema::new(&registry).expect("compile bundle");
        (
            instances.iter().map(|i| original.is_valid(i)).collect(),
            instances.iter().map(|i| bundled.is_valid(i)).collect(),
        )
    }

    fn uri(text: &str) -> Uri {
        parse_uri(text).expect("absolute URI")
    }

    // --- Embedding ---

    #[test]
    fn schema_without_external_references_bundles_to_itself() {
        let root: Value = json!({"$defs": {"a": {"type": "string"}}, "$ref": "#/$defs/a"});
        let expected: Value = root.clone();
        let actual: Value = bundle(&root, &[]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn referenced_document_is_embedded_under_defs_with_a_relative_id() {
        let root: Value = json!({"properties": {"home": {"$ref": "address.json"}}});
        let address: Value = json!({"type": "object", "required": ["city"]});
        let expected: Value = json!({
            "properties": {"home": {"$ref": "address.json"}},
            "$defs": {
                "address.json": {"$id": "address.json", "type": "object", "required": ["city"]}
            }
        });
        let actual: Value = bundle(
            &root,
            &[("https://example.com/schemas/address.json", address)],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn documents_reached_through_embedded_documents_are_embedded_once() {
        let root: Value = json!({"properties": {
            "a": {"$ref": "types/a.json"},
            "b": {"$ref": "types/b.json#/$defs/x"}
        }});
        let a: Value = json!({"items": {"$ref": "b.json#/$defs/x"}});
        let b: Value = json!({"$defs": {"x": {"type": "integer"}, "back": {"$ref": "a.json"}}});
        let expected: Value = json!({
            "properties": {
                "a": {"$ref": "types/a.json"},
                "b": {"$ref": "types/b.json#/$defs/x"}
            },
            "$defs": {
                "types/a.json": {"$id": "types/a.json", "items": {"$ref": "b.json#/$defs/x"}},
                "types/b.json": {
                    "$id": "types/b.json",
                    "$defs": {"x": {"type": "integer"}, "back": {"$ref": "a.json"}}
                }
            }
        });
        let actual: Value = bundle(
            &root,
            &[
                ("https://example.com/schemas/types/a.json", a),
                ("https://example.com/schemas/types/b.json", b),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_to_a_retrieval_uri_is_rewritten_to_the_document_id() {
        let root: Value = json!({"$ref": "money.json#/$defs/amount"});
        let money: Value = json!({
            "$id": "https://other.org/money",
            "$defs": {"amount": {"type": "number"}}
        });
        let expected: Value = json!({
            "$ref": "https://other.org/money#/$defs/amount",
            "$defs": {
                "https://other.org/money": {
                    "$id": "https://other.org/money",
                    "$defs": {"amount": {"type": "number"}}
                }
            }
        });
        let actual: Value = bundle(&root, &[("https://example.com/schemas/money.json", money)]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn existing_definition_keeps_its_key_and_the_document_takes_a_suffix() {
        let root: Value = json!({
            "$ref": "a.json",
            "$defs": {"a.json": {"type": "null"}}
        });
        let expected: Value = json!({
            "$ref": "a.json",
            "$defs": {
                "a.json": {"type": "null"},
                "a.json_2": {"$id": "a.json", "type": "string"}
            }
        });
        let actual: Value = bundle(
            &root,
            &[(
                "https://example.com/schemas/a.json",
                json!({"type": "string"}),
            )],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_documents_become_objects() {
        let root: Value = json!({"anyOf": [{"$ref": "yes.json"}, {"$ref": "no.json"}]});
        let expected: Value = json!({
            "anyOf": [{"$ref": "yes.json"}, {"$ref": "no.json"}],
            "$defs": {
                "no.json": {"$id": "no.json", "not": {}},
                "yes.json": {"$id": "yes.json"}
            }
        });
        let actual: Value = bundle(
            &root,
            &[
                ("https://example.com/schemas/yes.json", json!(true)),
                ("https://example.com/schemas/no.json", json!(false)),
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn standard_metaschema_is_not_embedded() {
        let root: Value = json!({"$ref": "https://json-schema.org/draft/2020-12/schema"});
        let expected: Value = root.clone();
        let actual: Value = bundle(&root, &[]);
        assert_eq!(expected, actual);
    }

    // --- Drafts ---

    #[test]
    fn draft7_root_embeds_under_definitions() {
        let root: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "items": {"$ref": "item.json"}
        });
        let expected: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "items": {"$ref": "item.json"},
            "definitions": {"item.json": {"$id": "item.json", "type": "integer"}}
        });
        let actual: Value = bundle(
            &root,
            &[(
                "https://example.com/schemas/item.json",
                json!({"type": "integer"}),
            )],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_document_with_root_ref_moves_the_ref_into_all_of() {
        let root: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$ref": "alias.json"
        });
        let alias: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$ref": "#/definitions/n",
            "type": "string",
            "definitions": {"n": {"type": "number"}}
        });
        let expected: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [{"$ref": "alias.json"}],
            "definitions": {
                "alias.json": {
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "$id": "alias.json",
                    "allOf": [{"$ref": "#/definitions/n"}],
                    "definitions": {"n": {"type": "number"}}
                }
            }
        });
        let actual: Value = bundle(&root, &[("https://example.com/schemas/alias.json", alias)]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_document_in_a_newer_root_keeps_draft4_shapes() {
        let root: Value = json!({"$ref": "old.json"});
        let old: Value = json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "properties": {"n": {"id": "#n", "minimum": 0, "exclusiveMinimum": true}}
        });
        let expected: Value = json!({
            "$ref": "old.json",
            "$defs": {
                "old.json": {
                    "$schema": "http://json-schema.org/draft-04/schema#",
                    "$id": "old.json",
                    "id": "old.json",
                    "properties": {"n": {"id": "#n", "minimum": 0, "exclusiveMinimum": true}}
                }
            }
        });
        let actual: Value = bundle(&root, &[("https://example.com/schemas/old.json", old)]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn document_of_another_draft_gets_its_schema() {
        let root: Value = json!({"$ref": "new.json"});
        let new: Value = json!({"$schema": "https://json-schema.org/draft/2019-09/schema"});
        let expected: Value = json!({
            "$ref": "new.json",
            "$defs": {
                "new.json": {
                    "$schema": "https://json-schema.org/draft/2019-09/schema",
                    "$id": "new.json"
                }
            }
        });
        let actual: Value = bundle(&root, &[("https://example.com/schemas/new.json", new)]);
        assert_eq!(expected, actual);
    }

    // --- Same verdicts ---

    #[test]
    fn bundle_validates_like_the_documents_it_holds() {
        let root: Value = json!({
            "type": "object",
            "properties": {
                "price": {"$ref": "money.json#/$defs/amount"},
                "tags": {"$ref": "types/list.json"}
            }
        });
        let documents: [(&str, Value); 3] = [
            (
                "https://example.com/schemas/money.json",
                json!({"$id": "https://other.org/money", "$defs": {"amount": {"minimum": 0}}}),
            ),
            (
                "https://example.com/schemas/types/list.json",
                json!({"type": "array", "items": {"$ref": "item.json"}}),
            ),
            (
                "https://example.com/schemas/types/item.json",
                json!({"type": "string", "maxLength": 3}),
            ),
        ];
        let instances: [Value; 4] = [
            json!({"price": 1, "tags": ["a"]}),
            json!({"price": -1}),
            json!({"tags": ["long"]}),
            json!({"tags": [1]}),
        ];
        let (expected, actual): (Vec<bool>, Vec<bool>) =
            verdicts(&root, &documents, ROOT_URI, &instances);
        assert_eq!(expected, actual);
    }

    #[test]
    fn bundle_validates_the_same_when_read_from_another_location() {
        let root: Value = json!({"items": {"$ref": "types/item.json"}});
        let documents: [(&str, Value); 1] = [(
            "https://example.com/schemas/types/item.json",
            json!({"type": "integer"}),
        )];
        let instances: [Value; 2] = [json!([1]), json!(["x"])];
        let (expected, actual): (Vec<bool>, Vec<bool>) = verdicts(
            &root,
            &documents,
            "file:///elsewhere/bundle.json",
            &instances,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_bundle_validates_like_the_documents_it_holds() {
        let root: Value = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$ref": "alias.json"
        });
        let documents: [(&str, Value); 1] = [(
            "https://example.com/schemas/alias.json",
            json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "$ref": "#/definitions/n",
                "type": "string",
                "definitions": {"n": {"type": "number"}}
            }),
        )];
        let instances: [Value; 2] = [json!(1), json!("x")];
        let (expected, actual): (Vec<bool>, Vec<bool>) =
            verdicts(&root, &documents, ROOT_URI, &instances);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_bundle_validates_like_the_documents_it_holds() {
        let root: Value = json!({"$ref": "old.json"});
        let documents: [(&str, Value); 1] = [(
            "https://example.com/schemas/old.json",
            json!({
                "$schema": "http://json-schema.org/draft-04/schema#",
                "minimum": 0,
                "exclusiveMinimum": true
            }),
        )];
        let instances: [Value; 2] = [json!(0), json!(1)];
        let (expected, actual): (Vec<bool>, Vec<bool>) =
            verdicts(&root, &documents, ROOT_URI, &instances);
        assert_eq!(expected, actual);
    }

    // --- Relative URIs ---

    #[test]
    fn relative_uri_in_the_same_directory_is_the_file_name() {
        let expected: &str = "b.json";
        let actual: String = relative_uri(&uri("file:///s/a.json"), &uri("file:///s/b.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_uri_climbs_to_a_parent_directory() {
        let expected: &str = "../c/d.json";
        let actual: String = relative_uri(&uri("file:///s/a/b.json"), &uri("file:///s/c/d.json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_uri_keeps_the_query() {
        let expected: &str = "d?v=2";
        let actual: String = relative_uri(&uri("https://h/c/a?v=1"), &uri("https://h/c/d?v=2"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_uri_to_another_authority_is_absolute() {
        let expected: &str = "https://other.org/x";
        let actual: String =
            relative_uri(&uri("https://example.com/a"), &uri("https://other.org/x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_uri_without_a_hierarchical_path_is_absolute() {
        let expected: &str = "urn:example:b";
        let actual: String = relative_uri(&uri("urn:example:a"), &uri("urn:example:b"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_uri_with_a_colon_in_its_first_segment_starts_with_a_dot() {
        let expected: &str = "./a:b.json";
        let actual: String = relative_uri(&uri("file:///s/x.json"), &uri("file:///s/a:b.json"));
        assert_eq!(expected, actual);
    }
}
