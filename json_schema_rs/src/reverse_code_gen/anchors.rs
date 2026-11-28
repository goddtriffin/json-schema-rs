//! Plain-name anchors in the target dialect.
//!
//! Before 2019-09 a plain-name anchor is the fragment of a `$id` (`"$id": "#a"`, draft-04 core
//! §7.2, draft-07 core §8.2.3). From 2019-09 `$anchor` declares it, and `$id` MUST NOT hold a
//! non-empty fragment (2019-09 core §8.2.2 and §8.2.3, 2020-12 core §8.2.1 and §8.2.2; both
//! core metaschemas give `$id` the pattern `^[^#]*#?$`). So for a target with `$anchor`, every
//! `$id` with a plain-name fragment is written as the `$id` without it plus that `$anchor`
//! (`#a` becomes `"$anchor": "a"`, `https://x/y#a` becomes `"$id": "https://x/y"` and
//! `"$anchor": "a"`), which declares the same anchor in the same resource, so every `#a` ref
//! still resolves. An empty fragment (`#`, `https://x/y#`) is legal there and kept.
//!
//! The same walk reports an anchor declared twice in one resource, which the registry rejects
//! (2020-12 core §8.2.2): two fragment ids that were harmless as written can become two equal
//! `$anchor`s.

use super::error::ReverseCodeGenError;
use super::escape_pointer_token;
use super::resource_uri::starts_resource;
use super::settings::TargetDialect;
use crate::json_schema::{Dialect, JsonSchema, Schema, SpecVersion, SubschemaPosition};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// A class of bytes in an anchor-name grammar.
type ByteClass = fn(u8) -> bool;

/// Whether `name` is a legal `$anchor` value in `version`: a letter, then letters, digits, `-`,
/// `_`, `:` and `.` in 2019-09 (core §8.2.3); a letter or `_`, then letters, digits, `-`, `_` and
/// `.` in 2020-12 (core §8.2.2). `None` for a draft without `$anchor`.
pub(super) fn is_anchor_name(version: SpecVersion, name: &str) -> Option<bool> {
    let (first, rest): (ByteClass, ByteClass) = match version {
        SpecVersion::Draft201909 => (
            |c: u8| c.is_ascii_alphabetic(),
            |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.'),
        ),
        SpecVersion::Draft202012 => (
            |c: u8| c.is_ascii_alphabetic() || c == b'_',
            |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'),
        ),
        _ => return None,
    };
    let bytes: &[u8] = name.as_bytes();
    Some(
        bytes
            .first()
            .is_some_and(|&c| first(c) && bytes[1..].iter().all(|&c| rest(c))),
    )
}

/// A schema [`ids_to_anchors`] still has to visit.
enum Node<'a> {
    Typed(&'a mut JsonSchema),
    Raw(&'a mut Value),
}

/// One step of a JSON Pointer below its parent.
enum Token<'a> {
    Keyword(&'static str),
    Key(&'a str),
    Index(usize),
}

/// The walk of one schema tree: the JSON Pointer of every schema reached, as its parent's index
/// and its last token, built into a string only when an error names it.
struct Walk<'a> {
    /// The pointer of the tree's root.
    root: &'a str,
    steps: Vec<(usize, Token<'a>)>,
}

impl<'a> Walk<'a> {
    fn child(&mut self, parent: usize, token: Token<'a>) -> usize {
        self.steps.push((parent, token));
        self.steps.len() - 1
    }

    fn pointer(&self, at: usize) -> String {
        let mut tokens: Vec<&Token<'a>> = Vec::new();
        let mut current: usize = at;
        while current != 0 {
            let (parent, token) = &self.steps[current];
            tokens.push(token);
            current = *parent;
        }
        let mut pointer: String = self.root.to_string();
        for token in tokens.into_iter().rev() {
            pointer.push('/');
            match token {
                Token::Keyword(keyword) => pointer.push_str(keyword),
                Token::Key(key) => pointer.push_str(&escape_pointer_token(key)),
                Token::Index(index) => pointer.push_str(&index.to_string()),
            }
        }
        pointer
    }
}

/// Whether `version` declares plain-name anchors with `$anchor` (2019-09 on).
fn has_anchor(version: SpecVersion) -> bool {
    is_anchor_name(version, "a").is_some()
}

/// A schema still to visit: the schema, its index in [`Walk::steps`], the draft whose anchor
/// grammar applies to it, and the index of the resource it sits in.
type Pending<'a> = (Node<'a>, usize, SpecVersion, usize);

/// The anchors seen so far, by resource index and name, each with the JSON Pointer of the
/// schema that declares it first.
type Declared = HashMap<(usize, String), String>;

/// For a target with `$anchor`, writes the plain-name fragment of every `$id` in `body` and in
/// `definitions` (whose entries sit under the JSON Pointer `path`, in the document's root
/// resource) as `$anchor`, as the module docs say, at any depth, through typed subschemas and
/// the subschema keywords of raw `extensions` values. An embedded resource with its own
/// `$schema` follows that draft's anchor-name grammar, and one of a draft without `$anchor` (or
/// an unknown `$schema`) is left as written, since its fragment ids are its anchors.
///
/// A fragment that cannot become an `$anchor` is a
/// [`ReverseCodeGenError::UnrepresentableKeyword`] for `$id` and stays as written: one that is
/// no anchor name of the draft (a JSON Pointer fragment `#/a`, `#1a`, or `#a:b` in 2020-12), or
/// one beside an `$anchor` of another name (a schema declares one `$anchor`). Beside an
/// `$anchor` of the same name only the fragment goes.
///
/// An anchor (`$anchor`, and `$dynamicAnchor` in 2020-12) that an earlier schema of the same
/// resource already declares is a [`ReverseCodeGenError::DuplicateAnchor`].
pub(super) fn ids_to_anchors(
    target: TargetDialect,
    body: &mut JsonSchema,
    definitions: &mut BTreeMap<String, Schema>,
    path: &str,
    errors: &mut Vec<ReverseCodeGenError>,
) {
    let Some(version) = target.spec_version().filter(|version| has_anchor(*version)) else {
        return;
    };
    let mut declared: Declared = HashMap::new();
    let mut resources: usize = 1;
    let mut context = Context {
        target,
        declared: &mut declared,
        resources: &mut resources,
        errors,
    };
    context.walk(version, Node::Typed(body), "", true);
    for (name, definition) in definitions.iter_mut() {
        let Some(object) = definition.as_object_mut() else {
            continue;
        };
        let root: String = format!("{path}/{}", escape_pointer_token(name));
        context.walk(version, Node::Typed(object), &root, false);
    }
}

/// What [`ids_to_anchors`] carries from one tree to the next.
struct Context<'c> {
    target: TargetDialect,
    declared: &'c mut Declared,
    /// How many resources the walks have entered, the document's root resource (index 0) included.
    resources: &'c mut usize,
    errors: &'c mut Vec<ReverseCodeGenError>,
}

impl Context<'_> {
    /// [`ids_to_anchors`] for one tree at `root`, in the document's root resource.
    /// `document_root` marks the document's root, whose `$schema` the generator sets itself.
    fn walk(&mut self, version: SpecVersion, node: Node<'_>, root: &str, document_root: bool) {
        let mut walk: Walk<'_> = Walk {
            root,
            steps: vec![(0, Token::Index(0))],
        };
        let mut stack: Vec<Pending<'_>> = vec![(node, 0, version, 0)];
        while let Some((node, at, parent_version, parent_resource)) = stack.pop() {
            let own_schema: Option<&str> = match &node {
                Node::Typed(schema) => schema.schema.as_deref(),
                Node::Raw(Value::Object(map)) => map.get("$schema").and_then(Value::as_str),
                Node::Raw(_) => continue,
            };
            let version: SpecVersion = match own_schema {
                Some(uri) if !(document_root && at == 0) => {
                    match SpecVersion::from_schema_uri(uri).filter(|version| has_anchor(*version)) {
                        Some(version) => version,
                        None => continue,
                    }
                }
                _ => parent_version,
            };
            let dialect: Dialect = Dialect::for_spec_version(version);
            match node {
                Node::Typed(schema) => {
                    let anchor: Option<&Value> = schema.extensions.get("$anchor");
                    match split_id(version, schema.id.as_deref(), anchor) {
                        Split::Keep => {}
                        Split::Unrepresentable => self.unrepresentable(&walk, at),
                        Split::Anchor { id, anchor } => {
                            schema.id = id;
                            schema
                                .extensions
                                .insert("$anchor".to_string(), Value::String(anchor));
                        }
                    }
                    let resource: usize = self.resource(
                        schema.id.as_deref(),
                        document_root && at == 0,
                        parent_resource,
                    );
                    let anchors: [Option<&str>; 2] =
                        declared_anchors(dialect, |keyword| schema.extensions.get(keyword));
                    self.declare(anchors, resource, &walk, at);
                    push_typed_children(
                        schema,
                        (at, version, resource),
                        &mut walk,
                        &mut stack,
                        dialect,
                    );
                }
                Node::Raw(value) => {
                    let Value::Object(map) = value else {
                        continue;
                    };
                    let id: Option<&str> = map.get("$id").and_then(Value::as_str);
                    match split_id(version, id, map.get("$anchor")) {
                        Split::Keep => {}
                        Split::Unrepresentable => self.unrepresentable(&walk, at),
                        Split::Anchor { id, anchor } => {
                            match id {
                                Some(id) => map.insert("$id".to_string(), Value::String(id)),
                                None => map.remove("$id"),
                            };
                            map.insert("$anchor".to_string(), Value::String(anchor));
                        }
                    }
                    let resource: usize = self.resource(
                        map.get("$id").and_then(Value::as_str),
                        document_root && at == 0,
                        parent_resource,
                    );
                    let anchors: [Option<&str>; 2] =
                        declared_anchors(dialect, |keyword| map.get(keyword));
                    self.declare(anchors, resource, &walk, at);
                    for (key, child) in map.iter_mut() {
                        push_raw(
                            dialect,
                            key,
                            child,
                            (at, version, resource),
                            &mut walk,
                            &mut stack,
                        );
                    }
                }
            }
        }
    }

    /// The resource a schema with the `$id` `id` sits in: a new one when the `$id` starts one
    /// (except at the document's root, the root resource), else its parent's.
    fn resource(&mut self, id: Option<&str>, document_root: bool, parent: usize) -> usize {
        if document_root || !id.is_some_and(starts_resource) {
            return parent;
        }
        *self.resources += 1;
        *self.resources - 1
    }

    /// Records the `anchors` the schema at `at` declares in `resource`, reporting each that
    /// another schema there already declares.
    fn declare(&mut self, anchors: [Option<&str>; 2], resource: usize, walk: &Walk<'_>, at: usize) {
        for name in anchors.into_iter().flatten() {
            let pointer: String = walk.pointer(at);
            match self.declared.entry((resource, name.to_string())) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(pointer);
                }
                std::collections::hash_map::Entry::Occupied(entry) => {
                    self.errors.push(ReverseCodeGenError::DuplicateAnchor {
                        first: format!("#{}", entry.get()),
                        second: format!("#{pointer}"),
                        anchor: name.to_string(),
                    });
                }
            }
        }
    }

    fn unrepresentable(&mut self, walk: &Walk<'_>, at: usize) {
        self.errors
            .push(ReverseCodeGenError::UnrepresentableKeyword {
                dialect: self.target,
                keyword: "$id".to_string(),
                pointer: walk.pointer(at),
            });
    }
}

/// The plain-name anchors a schema whose keyword values `get` gives declares in `dialect`: its
/// `$anchor`, and its `$dynamicAnchor` where the dialect has one, once each.
fn declared_anchors<'v>(
    dialect: Dialect,
    get: impl Fn(&str) -> Option<&'v Value>,
) -> [Option<&'v str>; 2] {
    let anchor: Option<&str> = get("$anchor").and_then(Value::as_str);
    let dynamic: Option<&str> = get("$dynamicAnchor")
        .and_then(Value::as_str)
        .filter(|name| dialect.keyword("$dynamicAnchor").is_some() && anchor != Some(*name));
    [anchor, dynamic]
}

/// What becomes of one schema's `$id`.
enum Split {
    /// It has no non-empty fragment: kept as written.
    Keep,
    /// Its fragment cannot become an `$anchor`.
    Unrepresentable,
    /// It becomes `id` (none when it was fragment-only) and the `$anchor` `anchor`.
    Anchor { id: Option<String>, anchor: String },
}

/// How the `$id` `id` of a schema whose `$anchor` is `anchor` is written in `version`.
fn split_id(version: SpecVersion, id: Option<&str>, anchor: Option<&Value>) -> Split {
    let Some((resource, fragment)) = id.and_then(|id| id.split_once('#')) else {
        return Split::Keep;
    };
    if fragment.is_empty() {
        return Split::Keep;
    }
    if is_anchor_name(version, fragment) != Some(true)
        || anchor.is_some_and(|anchor| anchor.as_str() != Some(fragment))
    {
        return Split::Unrepresentable;
    }
    Split::Anchor {
        id: (!resource.is_empty()).then(|| resource.to_string()),
        anchor: fragment.to_string(),
    }
}

/// Pushes the typed subschemas of `schema` and the raw ones its `extensions` hold.
fn push_typed_children<'a>(
    schema: &'a mut JsonSchema,
    parent: (usize, SpecVersion, usize),
    walk: &mut Walk<'a>,
    stack: &mut Vec<Pending<'a>>,
    dialect: Dialect,
) {
    let (at, version, resource) = parent;
    let mut push = |child: &'a mut Schema, index: usize| {
        if let Some(object) = child.as_object_mut() {
            stack.push((Node::Typed(object), index, version, resource));
        }
    };
    for (keyword, map) in [
        ("$defs", schema.defs.as_mut()),
        ("definitions", schema.definitions.as_mut()),
        ("properties", Some(&mut schema.properties)),
    ] {
        let Some(map) = map else {
            continue;
        };
        let container: usize = walk.child(at, Token::Keyword(keyword));
        for (key, child) in map.iter_mut() {
            let index: usize = walk.child(container, Token::Key(key));
            push(child, index);
        }
    }
    for (keyword, child) in [
        (
            "additionalProperties",
            schema.additional_properties.as_mut(),
        ),
        ("items", schema.items.as_mut()),
    ] {
        if let Some(child) = child {
            let index: usize = walk.child(at, Token::Keyword(keyword));
            push(child, index);
        }
    }
    for (keyword, list) in [
        ("allOf", schema.all_of.as_mut()),
        ("anyOf", schema.any_of.as_mut()),
        ("oneOf", schema.one_of.as_mut()),
    ] {
        let Some(list) = list else {
            continue;
        };
        let container: usize = walk.child(at, Token::Keyword(keyword));
        for (position, child) in list.iter_mut().enumerate() {
            let index: usize = walk.child(container, Token::Index(position));
            push(child, index);
        }
    }
    for (key, child) in &mut schema.extensions {
        push_raw(dialect, key, child, parent, walk, stack);
    }
}

/// Pushes the subschemas the raw `keyword: value` of the schema `parent` (its index, draft and
/// resource) holds, per `dialect`'s keyword table.
fn push_raw<'a>(
    dialect: Dialect,
    keyword: &'a str,
    value: &'a mut Value,
    parent: (usize, SpecVersion, usize),
    walk: &mut Walk<'a>,
    stack: &mut Vec<Pending<'a>>,
) {
    let (at, version, resource) = parent;
    let Some(position) = dialect.keyword(keyword).and_then(|k| k.subschemas) else {
        return;
    };
    match (position, value) {
        (SubschemaPosition::Array | SubschemaPosition::SingleOrArray, Value::Array(items)) => {
            let container: usize = walk.child(at, Token::Key(keyword));
            for (position, item) in items.iter_mut().enumerate() {
                let index: usize = walk.child(container, Token::Index(position));
                stack.push((Node::Raw(item), index, version, resource));
            }
        }
        (SubschemaPosition::Map, Value::Object(map)) => {
            let container: usize = walk.child(at, Token::Key(keyword));
            for (key, entry) in map.iter_mut() {
                let index: usize = walk.child(container, Token::Key(key));
                stack.push((Node::Raw(entry), index, version, resource));
            }
        }
        (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, value) => {
            let index: usize = walk.child(at, Token::Key(keyword));
            stack.push((Node::Raw(value), index, version, resource));
        }
        (SubschemaPosition::Array | SubschemaPosition::Map, _) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::is_anchor_name;
    use crate::json_schema::SpecVersion;

    #[test]
    fn anchor_names_follow_each_drafts_grammar() {
        let names: [&str; 7] = ["a", "a:b", "_a", "1a", "a-b.c", "", "a/b"];
        let expected: Vec<[Option<bool>; 3]> = vec![
            [Some(true), Some(true), None],
            [Some(true), Some(false), None],
            [Some(false), Some(true), None],
            [Some(false), Some(false), None],
            [Some(true), Some(true), None],
            [Some(false), Some(false), None],
            [Some(false), Some(false), None],
        ];
        let actual: Vec<[Option<bool>; 3]> = names
            .iter()
            .map(|name: &&str| {
                [
                    SpecVersion::Draft201909,
                    SpecVersion::Draft202012,
                    SpecVersion::Draft07,
                ]
                .map(|version: SpecVersion| is_anchor_name(version, name))
            })
            .collect();
        assert_eq!(expected, actual);
    }
}
