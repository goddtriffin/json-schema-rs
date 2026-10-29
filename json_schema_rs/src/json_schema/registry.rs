//! The schema registry: every schema resource reachable from a root document, and where each
//! `$ref` in them points.
//!
//! A [`Registry`] is built once from a root schema and is immutable afterwards; the validator
//! and codegen both resolve `$ref` through it. Building it:
//!
//! 1. **Discovery.** An iterative walk (explicit stack) over every schema position of a document
//!    records each schema resource (the document root, under its retrieval URI, and every
//!    subschema with `$id` / draft-4 `id`), each anchor (`$anchor`, `$dynamicAnchor`, a draft
//!    4–7 plain-name fragment `$id`), and each `$ref` with the resource it appears in. Schema
//!    positions are the typed fields of [`JsonSchema`] plus, inside [`JsonSchema::extensions`],
//!    the keywords whose value holds subschemas in the resource's [`Dialect`] keyword table;
//!    an unknown keyword is not a schema position, so an `$id` inside it is not an identifier.
//! 2. **Resolution.** Each `$ref` is resolved against its resource's base URI (RFC 3986,
//!    [`Uri::resolve`]); the fragment is a JSON Pointer from the target resource's root or an
//!    anchor name. A URI that no loaded document defines is loaded: an embedded standard
//!    metaschema, a document given to the [`RegistryBuilder`], or one the [`Retrieve`]r
//!    returns; the loaded document is discovered in turn, and the loop repeats until every
//!    `$ref` is resolved.
//! 3. **Targets outside the typed model.** A `$ref` may point into a value the typed model
//!    keeps as raw JSON (an unknown keyword, `examples`, a keyword the model does not hold).
//!    That value is parsed into a [`Schema`] once, at build time, and kept by the registry.
//!
//! Errors are typed and found at build time: a duplicate `$id` or anchor, a `$ref` that
//! resolves to nothing or to a location that is not a schema, a document that cannot be
//! retrieved or parsed, and a custom metaschema that requires an unknown vocabulary.
//!
//! **Scopes.** A consumer walking a schema carries the [`Scope`] (resource) it is in. Before
//! looking at a node it calls [`Registry::enter`], which moves into the node's own resource when
//! the node has an `$id`. [`Registry::resolve`] then maps the node's `$ref` (as written) to the
//! target schema and the scope to carry into it.

use super::dialect::{Dialect, FormatMode, RefSiblings, SubschemaPosition, VocabularyKind};
use super::json_schema::{JsonSchema, Schema, SchemaRef};
use super::settings::{DialectSettings, JsonSchemaSettings};
use super::spec_version::SpecVersion;
use crate::error_list::ErrorList;
use crate::json_pointer::JsonPointer;
use fluent_uri::{Iri, IriRef};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;
use std::sync::Arc;

/// The base URI of a root document that has no retrieval URI and no absolute `$id`.
pub const DEFAULT_BASE_URI: &str = "json-schema:///";

/// An absolute URI in normal form (RFC 3986 §6.2.2, via `fluent-uri`): two spellings of one
/// identifier are equal.
pub(crate) type Uri = Iri<String>;

/// Parses and normalises an absolute URI (an IRI: non-ASCII characters are allowed), or says why
/// `text` is not one.
pub(crate) fn parse_uri(text: &str) -> Result<Uri, String> {
    match Iri::parse(text) {
        Ok(iri) => Ok(iri.normalize()),
        Err(_) if IriRef::parse(text).is_ok() => {
            Err("it has no scheme (it is a relative reference)".to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Resolves the URI reference `reference` against `base` (RFC 3986 §5.2.2, ignoring `base`'s
/// fragment) and normalises the result, which keeps the reference's fragment. Fails when
/// `reference` is not a URI reference, or is a relative path against a base without a
/// hierarchical path (a `urn:`; a fragment-only reference always resolves).
pub(crate) fn resolve_uri(base: &Uri, reference: &str) -> Result<Uri, String> {
    let reference: IriRef<&str> = IriRef::parse(reference).map_err(|e| e.to_string())?;
    reference
        .resolve_against(&base.strip_fragment())
        .map(|resolved| resolved.normalize())
        .map_err(|e| e.to_string())
}

/// `uri` without its fragment.
fn without_fragment(uri: &Uri) -> Uri {
    uri.strip_fragment().normalize()
}

/// The standard metaschemas, embedded so that `$ref` and `$schema` to them never need the
/// network: `(URI without fragment, document text)`.
const METASCHEMAS: &[(&str, &str)] = &[
    (
        "http://json-schema.org/draft-04/schema",
        include_str!("../../metaschemas/draft-04/schema.json"),
    ),
    (
        "http://json-schema.org/draft-06/schema",
        include_str!("../../metaschemas/draft-06/schema.json"),
    ),
    (
        "http://json-schema.org/draft-07/schema",
        include_str!("../../metaschemas/draft-07/schema.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/schema",
        include_str!("../../metaschemas/draft/2019-09/schema.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/core",
        include_str!("../../metaschemas/draft/2019-09/meta/core.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/applicator",
        include_str!("../../metaschemas/draft/2019-09/meta/applicator.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/validation",
        include_str!("../../metaschemas/draft/2019-09/meta/validation.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/meta-data",
        include_str!("../../metaschemas/draft/2019-09/meta/meta-data.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/format",
        include_str!("../../metaschemas/draft/2019-09/meta/format.json"),
    ),
    (
        "https://json-schema.org/draft/2019-09/meta/content",
        include_str!("../../metaschemas/draft/2019-09/meta/content.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/schema",
        include_str!("../../metaschemas/draft/2020-12/schema.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/core",
        include_str!("../../metaschemas/draft/2020-12/meta/core.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/applicator",
        include_str!("../../metaschemas/draft/2020-12/meta/applicator.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/unevaluated",
        include_str!("../../metaschemas/draft/2020-12/meta/unevaluated.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/validation",
        include_str!("../../metaschemas/draft/2020-12/meta/validation.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/meta-data",
        include_str!("../../metaschemas/draft/2020-12/meta/meta-data.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/format-annotation",
        include_str!("../../metaschemas/draft/2020-12/meta/format-annotation.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/format-assertion",
        include_str!("../../metaschemas/draft/2020-12/meta/format-assertion.json"),
    ),
    (
        "https://json-schema.org/draft/2020-12/meta/content",
        include_str!("../../metaschemas/draft/2020-12/meta/content.json"),
    ),
];

/// Why a document could not be retrieved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrieveError {
    /// Human-readable reason.
    pub reason: String,
}

impl fmt::Display for RetrieveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for RetrieveError {}

/// Fetches schema documents that a registry needs and does not have.
///
/// The registry asks only for absolute URIs without a fragment, at most once each, and only
/// while it is being built. Embedded standard metaschemas and documents given to
/// [`RegistryBuilder::document`] are never retrieved.
pub trait Retrieve {
    /// Returns the JSON document identified by `uri`.
    ///
    /// # Errors
    ///
    /// Returns [`RetrieveError`] when the document is not available.
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError>;
}

impl<T: Retrieve + ?Sized> Retrieve for &T {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        (**self).retrieve(uri)
    }
}

/// The default retriever: retrieves nothing, so a `$ref` to a document the registry was not
/// given is a build error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OfflineRetriever;

impl Retrieve for OfflineRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        Err(RetrieveError {
            reason: format!("retrieval is disabled; no document was given for \"{uri}\""),
        })
    }
}

/// An in-memory retriever keyed by absolute URI (without fragment, in normalised form: lower-case
/// scheme and host, no default port), for tests and callers that load documents themselves.
impl<S: BuildHasher> Retrieve for HashMap<String, Value, S> {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        self.get(uri).cloned().ok_or_else(|| RetrieveError {
            reason: format!("no in-memory document for \"{uri}\""),
        })
    }
}

/// An in-memory retriever keyed by absolute URI (without fragment).
impl Retrieve for BTreeMap<String, Value> {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        self.get(uri).cloned().ok_or_else(|| RetrieveError {
            reason: format!("no in-memory document for \"{uri}\""),
        })
    }
}

/// Every problem building one [`Registry`] found, one [`RegistryError`] each.
pub type RegistryErrors = ErrorList<RegistryError>;

/// Why a [`Registry`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// A URI is not valid: the builder's base URI or a document URI given to it is not an
    /// absolute URI, or a `$id` is not a valid URI reference.
    InvalidUri {
        /// The string as written.
        value: String,
        /// Why it is not valid.
        reason: String,
    },
    /// Two different schemas declare the same resource URI.
    DuplicateId {
        /// The resource URI.
        uri: String,
        /// JSON Pointer (within its document) of the schema that declared it first.
        first: String,
        /// JSON Pointer (within its document) of the schema that declared it again.
        second: String,
    },
    /// Two different schemas in one resource declare the same anchor.
    DuplicateAnchor {
        /// The resource URI.
        uri: String,
        /// The anchor name.
        anchor: String,
    },
    /// A `$ref` does not resolve to any schema.
    UnresolvableRef {
        /// The `$ref` as written.
        ref_str: String,
        /// The absolute URI it resolved to.
        target: String,
        /// Why nothing is there.
        reason: String,
    },
    /// A `$ref` resolves to a location that is not a schema (e.g. `#/properties` or `#/title`).
    RefTargetNotSchema {
        /// The `$ref` as written.
        ref_str: String,
        /// The absolute URI it resolved to.
        target: String,
    },
    /// A referenced document could not be retrieved.
    Retrieve {
        /// The document's URI.
        uri: String,
        /// Why retrieval failed.
        reason: String,
    },
    /// A referenced document (or a raw value a `$ref` targets) is not a valid schema.
    Parse {
        /// The document's URI, with a fragment for a value inside it.
        uri: String,
        /// The parse error.
        reason: String,
    },
    /// A custom metaschema requires a vocabulary this library does not know, so a schema using
    /// it cannot be processed (the specification requires refusing it).
    UnsupportedVocabulary {
        /// The metaschema URI.
        metaschema: String,
        /// The required vocabulary URI.
        vocabulary: String,
    },
}

impl RegistryError {
    /// The `$ref` this error is about, when it is about one.
    #[must_use]
    pub fn ref_str(&self) -> Option<&str> {
        match self {
            RegistryError::UnresolvableRef { ref_str, .. }
            | RegistryError::RefTargetNotSchema { ref_str, .. } => Some(ref_str),
            RegistryError::InvalidUri { .. }
            | RegistryError::DuplicateId { .. }
            | RegistryError::DuplicateAnchor { .. }
            | RegistryError::Retrieve { .. }
            | RegistryError::Parse { .. }
            | RegistryError::UnsupportedVocabulary { .. } => None,
        }
    }
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::InvalidUri { value, reason } => {
                write!(f, "\"{value}\" is not a valid URI: {reason}")
            }
            RegistryError::DuplicateId { uri, first, second } => write!(
                f,
                "duplicate schema resource \"{uri}\": declared at \"{first}\" and at \"{second}\""
            ),
            RegistryError::DuplicateAnchor { uri, anchor } => {
                write!(
                    f,
                    "duplicate anchor \"{anchor}\" in schema resource \"{uri}\""
                )
            }
            RegistryError::UnresolvableRef {
                ref_str,
                target,
                reason,
            } => write!(
                f,
                "$ref \"{ref_str}\" (\"{target}\") does not resolve: {reason}"
            ),
            RegistryError::RefTargetNotSchema { ref_str, target } => write!(
                f,
                "$ref \"{ref_str}\" (\"{target}\") points at a value that is not a schema"
            ),
            RegistryError::Retrieve { uri, reason } => {
                write!(f, "could not retrieve \"{uri}\": {reason}")
            }
            RegistryError::Parse { uri, reason } => {
                write!(f, "\"{uri}\" is not a valid schema: {reason}")
            }
            RegistryError::UnsupportedVocabulary {
                metaschema,
                vocabulary,
            } => write!(
                f,
                "metaschema \"{metaschema}\" requires the unsupported vocabulary \"{vocabulary}\""
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

/// A schema resource a consumer is evaluating in: the base URI for relative `$ref`s and the
/// dialect that applies. Obtained from [`Registry::root_scope`] and carried through
/// [`Registry::enter`] and [`Registry::resolve`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scope(usize);

/// A schema a `$ref` resolved to, and the scope to evaluate it in (pass both to
/// [`Registry::enter`] before reading it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolved<'r> {
    /// The target schema.
    pub schema: SchemaRef<'r>,
    /// The scope the target sits in (before entering the target's own `$id`, if it has one).
    pub scope: Scope,
    /// The target's location: JSON Pointer segments from its document's root.
    pub pointer: &'r [String],
}

impl Resolved<'_> {
    /// The name of the `$defs` / `definitions` entry the target is, if it is one.
    #[must_use]
    pub fn definition_name(&self) -> Option<&str> {
        match self.pointer {
            [.., container, name] if container == "$defs" || container == "definitions" => {
                Some(name)
            }
            _ => None,
        }
    }
}

/// Why a `$ref` could not be resolved while evaluating (the registry does not know it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownRef {
    /// The `$ref` as written.
    pub ref_str: String,
}

impl fmt::Display for UnknownRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "$ref \"{}\" is not in this schema's registry (the schema is not part of it)",
            self.ref_str
        )
    }
}

/// The root schema of a loaded document.
#[derive(Debug, Clone)]
enum DocumentRoot<'a> {
    Borrowed(SchemaRef<'a>),
    Owned(Arc<Schema>),
}

impl DocumentRoot<'_> {
    fn schema_ref(&self) -> SchemaRef<'_> {
        match self {
            DocumentRoot::Borrowed(schema) => *schema,
            DocumentRoot::Owned(schema) => schema.as_schema_ref(),
        }
    }
}

#[derive(Debug)]
struct Document<'a> {
    root: DocumentRoot<'a>,
    /// The scope of the document's retrieval URI: the parent scope of its root.
    scope: Scope,
}

/// Where a `$ref` target's schema lives.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetSchema {
    /// Reached through the typed model of a document, by `pointer`.
    Typed { document: usize },
    /// A raw value parsed into its own [`Schema`] (index into `Registry::detached`).
    Detached(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    schema: TargetSchema,
    pointer: Vec<String>,
    scope: Scope,
}

#[derive(Debug)]
struct Resource {
    uri: Uri,
    document: usize,
    pointer: Vec<String>,
    dialect: Dialect,
    /// Anchor name → JSON Pointer segments (in `document`).
    anchors: HashMap<String, Vec<String>>,
    /// `$dynamicAnchor` names (also in `anchors`).
    dynamic_anchors: HashMap<String, Vec<String>>,
    /// `$id` as written on a schema directly in this resource → the resource it declares.
    child_ids: HashMap<String, Scope>,
    /// `$ref` as written in this resource → its target.
    refs: HashMap<String, Target>,
}

/// One step of a JSON Pointer while walking, borrowed from the schema.
#[derive(Debug, Clone, Copy)]
enum Segment<'r> {
    Key(&'r str),
    Index(usize),
}

/// A schema node seen while discovering.
#[derive(Debug, Clone, Copy)]
enum Node<'r> {
    Typed(&'r JsonSchema),
    Raw(&'r Map<String, Value>),
}

impl<'r> Node<'r> {
    fn get_str(self, key: &str) -> Option<&'r str> {
        match self {
            Node::Typed(schema) => schema.extensions.get(key).and_then(Value::as_str),
            Node::Raw(obj) => obj.get(key).and_then(Value::as_str),
        }
    }

    /// The node's resource identifier, as written.
    fn id(self, dialect: Dialect) -> Option<&'r str> {
        match self {
            Node::Typed(schema) => schema.id.as_deref().or_else(|| {
                (dialect.id_keyword() == "id")
                    .then(|| self.get_str("id"))
                    .flatten()
            }),
            Node::Raw(obj) => obj
                .get("$id")
                .or_else(|| obj.get(dialect.id_keyword()))
                .and_then(Value::as_str),
        }
    }

    fn ref_(self) -> Option<&'r str> {
        match self {
            Node::Typed(schema) => schema.ref_.as_deref(),
            Node::Raw(_) => self.get_str("$ref"),
        }
    }

    fn schema_uri(self) -> Option<&'r str> {
        match self {
            Node::Typed(schema) => schema.schema.as_deref(),
            Node::Raw(_) => self.get_str("$schema"),
        }
    }

    /// The `$id` that takes effect: none when a draft-4–7 `$ref` sibling makes it ignored.
    fn effective_id(self, dialect: Dialect) -> Option<&'r str> {
        if dialect.ref_siblings == RefSiblings::Ignored && self.ref_().is_some() {
            return None;
        }
        self.id(dialect)
    }
}

/// The node `value` is as a schema, if it is a schema object.
fn raw_node(value: &Value) -> Option<Node<'_>> {
    value.as_object().map(Node::Raw)
}

/// Pushes the subschemas of `value`, the value of a keyword at `position`.
fn push_raw_children<'r>(
    value: &'r Value,
    position: SubschemaPosition,
    out: &mut Vec<(Node<'r>, Vec<Segment<'r>>)>,
) {
    match (position, value) {
        (SubschemaPosition::Single | SubschemaPosition::SingleOrArray, Value::Object(_)) => {
            out.extend(raw_node(value).map(|n| (n, vec![])));
        }
        (SubschemaPosition::Array | SubschemaPosition::SingleOrArray, Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                out.extend(raw_node(item).map(|n| (n, vec![Segment::Index(i)])));
            }
        }
        (SubschemaPosition::Map, Value::Object(map)) => {
            for (key, item) in map {
                out.extend(raw_node(item).map(|n| (n, vec![Segment::Key(key)])));
            }
        }
        _ => {}
    }
}

/// The child schema nodes of `node`, each with the pointer segments from `node` to it.
fn children<'r>(node: Node<'r>, dialect: Dialect) -> Vec<(Node<'r>, Vec<Segment<'r>>)> {
    let mut out: Vec<(Node<'r>, Vec<Segment<'r>>)> = Vec::new();
    let keywords: Vec<(&'r String, &'r Value)> = match node {
        Node::Typed(schema) => {
            let single = |key: &'r str, s: Option<&'r Schema>| {
                s.and_then(Schema::as_object)
                    .map(|o| (Node::Typed(o), vec![Segment::Key(key)]))
            };
            out.extend(single("items", schema.items.as_ref()));
            out.extend(single(
                "additionalProperties",
                schema.additional_properties.as_ref(),
            ));
            let maps: [(&'r str, Option<&'r BTreeMap<String, Schema>>); 3] = [
                ("properties", Some(&schema.properties)),
                ("$defs", schema.defs.as_ref()),
                ("definitions", schema.definitions.as_ref()),
            ];
            for (keyword, map) in maps {
                for (key, s) in map.into_iter().flatten() {
                    if let Some(o) = s.as_object() {
                        out.push((
                            Node::Typed(o),
                            vec![Segment::Key(keyword), Segment::Key(key)],
                        ));
                    }
                }
            }
            let lists: [(&'r str, Option<&'r Vec<Schema>>); 3] = [
                ("allOf", schema.all_of.as_ref()),
                ("anyOf", schema.any_of.as_ref()),
                ("oneOf", schema.one_of.as_ref()),
            ];
            for (keyword, list) in lists {
                for (i, s) in list.into_iter().flatten().enumerate() {
                    if let Some(o) = s.as_object() {
                        out.push((
                            Node::Typed(o),
                            vec![Segment::Key(keyword), Segment::Index(i)],
                        ));
                    }
                }
            }
            schema.extensions.iter().collect()
        }
        Node::Raw(obj) => obj.iter().collect(),
    };
    // Raw values: only the keywords whose value holds subschemas in this dialect.
    let mut raw: Vec<(Node<'r>, Vec<Segment<'r>>)> = Vec::new();
    for (key, value) in keywords {
        if let Some(position) = dialect.keyword(key).and_then(|k| k.subschemas) {
            push_raw_children(value, position, &mut raw);
            for (child, rest) in raw.drain(..) {
                let mut segments: Vec<Segment<'r>> = vec![Segment::Key(key)];
                segments.extend(rest);
                out.push((child, segments));
            }
        }
    }
    out
}

/// What a walk along a JSON Pointer is looking at.
#[derive(Debug, Clone, Copy)]
enum Step<'r> {
    /// A schema in the typed model.
    Typed(SchemaRef<'r>),
    /// A raw value at a schema position.
    RawSchema(&'r Value),
    /// A raw value whose subschema positions follow `position` (one more segment selects one).
    RawContainer(&'r Value),
    /// A raw value that is not at a schema position (data, or an unknown keyword's value).
    RawData(&'r Value),
    /// A typed value that is not a schema (`properties` itself, `title`, `required`, ...).
    NotSchema,
    /// Nothing is there: a map without the key, a list without the index, an absent subschema.
    Missing,
}

/// A builder for a [`Registry`] with documents, a retriever, a base URI, or settings beyond the
/// defaults of [`Registry::new`].
pub struct RegistryBuilder<'b> {
    base_uri: Option<String>,
    documents: HashMap<String, Value>,
    retriever: Box<dyn Retrieve + 'b>,
    settings: JsonSchemaSettings,
}

impl fmt::Debug for RegistryBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegistryBuilder")
            .field("base_uri", &self.base_uri)
            .field("documents", &self.documents.keys().collect::<Vec<_>>())
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Default for RegistryBuilder<'_> {
    fn default() -> Self {
        RegistryBuilder {
            base_uri: None,
            documents: HashMap::new(),
            retriever: Box::new(OfflineRetriever),
            settings: JsonSchemaSettings::default(),
        }
    }
}

impl<'b> RegistryBuilder<'b> {
    /// The root document's retrieval URI: the base for its relative `$id`s and `$ref`s.
    /// Default: [`DEFAULT_BASE_URI`].
    #[must_use]
    pub fn base_uri(mut self, uri: impl Into<String>) -> Self {
        self.base_uri = Some(uri.into());
        self
    }

    /// Adds a document the root may reference, identified by `uri` (absolute; normalised, and a
    /// fragment ignored). It is parsed and discovered only if a `$ref` or `$schema` reaches it.
    /// A `uri` that is not an absolute URI makes [`RegistryBuilder::build`] fail.
    #[must_use]
    pub fn document(mut self, uri: impl Into<String>, document: Value) -> Self {
        self.documents.insert(uri.into(), document);
        self
    }

    /// The retriever for referenced documents that are neither embedded metaschemas nor given
    /// with [`RegistryBuilder::document`]. Default: [`OfflineRetriever`].
    #[must_use]
    pub fn retriever(mut self, retriever: impl Retrieve + 'b) -> Self {
        self.retriever = Box::new(retriever);
        self
    }

    /// The settings referenced documents are parsed with. Their own `$schema` decides their
    /// draft; one without `$schema` takes the draft of the resource that references it.
    #[must_use]
    pub fn settings(mut self, settings: JsonSchemaSettings) -> Self {
        self.settings = settings;
        self
    }

    /// Builds the registry for `root`.
    ///
    /// # Errors
    ///
    /// Returns [`RegistryError`] when a `$ref` does not resolve, a resource or anchor is
    /// declared twice, a referenced document cannot be retrieved or parsed, or a custom
    /// metaschema requires an unknown vocabulary.
    pub fn build<'a>(self, root: impl Into<SchemaRef<'a>>) -> Result<Registry<'a>, RegistryError> {
        Builder::new(self)?.build(DocumentRoot::Borrowed(root.into()))
    }

    /// Builds the registry for an owned `root` (the registry keeps it).
    ///
    /// # Errors
    ///
    /// See [`RegistryBuilder::build`].
    pub fn build_owned(self, root: Schema) -> Result<Registry<'static>, RegistryError> {
        Builder::new(self)?.build(DocumentRoot::Owned(Arc::new(root)))
    }
}

/// Every schema resource reachable from a root schema, with each `$ref` resolved. See the
/// [module documentation](self).
#[derive(Debug)]
pub struct Registry<'a> {
    documents: Vec<Document<'a>>,
    detached: Vec<Schema>,
    resources: Vec<Resource>,
}

/// A `$ref` waiting to be resolved: the scope it appears in and the reference as written.
struct PendingRef {
    scope: Scope,
    ref_str: String,
}

/// Build-time state around the registry being built.
struct Builder<'a, 'b> {
    registry: Registry<'a>,
    by_uri: HashMap<Uri, Scope>,
    root_base: Uri,
    documents: HashMap<String, Value>,
    retriever: Box<dyn Retrieve + 'b>,
    settings: JsonSchemaSettings,
    /// Documents fetched (for a `$ref` or a custom `$schema`), by URI, before or after they
    /// are discovered.
    fetched: HashMap<Uri, Arc<Schema>>,
    /// Detached schemas by `(document, pointer)`.
    detached_at: HashMap<(usize, Vec<String>), usize>,
    pending: Vec<PendingRef>,
}

impl<'a, 'b> Builder<'a, 'b> {
    fn new(options: RegistryBuilder<'b>) -> Result<Self, RegistryError> {
        let invalid = |value: &str, reason: String| RegistryError::InvalidUri {
            value: value.to_string(),
            reason,
        };
        let base_text: &str = options.base_uri.as_deref().unwrap_or(DEFAULT_BASE_URI);
        let root_base: Uri =
            without_fragment(&parse_uri(base_text).map_err(|e| invalid(base_text, e))?);
        let mut documents: HashMap<String, Value> = HashMap::new();
        for (uri, document) in options.documents {
            let parsed: Uri = parse_uri(&uri).map_err(|e| invalid(&uri, e))?;
            documents.insert(without_fragment(&parsed).to_string(), document);
        }
        Ok(Builder {
            registry: Registry {
                documents: Vec::new(),
                detached: Vec::new(),
                resources: Vec::new(),
            },
            by_uri: HashMap::new(),
            root_base,
            documents,
            retriever: options.retriever,
            settings: options.settings,
            fetched: HashMap::new(),
            detached_at: HashMap::new(),
            pending: Vec::new(),
        })
    }

    fn build(mut self, root: DocumentRoot<'a>) -> Result<Registry<'a>, RegistryError> {
        let base: Uri = self.root_base.clone();
        self.add_document(base, root, None)?;
        while let Some(pending) = self.pending.pop() {
            self.resolve_pending(pending)?;
        }
        Ok(self.registry)
    }

    /// Adds a document under its retrieval `uri` and discovers it.
    fn add_document(
        &mut self,
        uri: Uri,
        root: DocumentRoot<'a>,
        inherited: Option<Dialect>,
    ) -> Result<(), RegistryError> {
        let document: usize = self.registry.documents.len();
        let dialect: Dialect = match root.schema_ref() {
            SchemaRef::Object(schema) => self.document_dialect(schema, inherited)?,
            SchemaRef::Bool(_) => inherited.unwrap_or_default(),
        };
        let scope: Scope = self.add_resource(uri, document, Vec::new(), dialect)?;
        let walked: DocumentRoot<'a> = root.clone();
        self.registry.documents.push(Document { root, scope });
        let node: Option<Node<'_>> = walked.schema_ref().as_object().map(Node::Typed);
        if let Some(node) = node {
            self.discover(node, document, &[], scope)?;
        }
        Ok(())
    }

    /// The dialect of a document root: the forced draft when its parse settings force one;
    /// else its `$schema` (a custom metaschema is loaded for its `$vocabulary`); else `inherited`
    /// (the referencing resource's) for a loaded document without `$schema`, or the default draft
    /// for the root document. The document's per-flag overrides apply on top.
    fn document_dialect(
        &mut self,
        schema: &JsonSchema,
        inherited: Option<Dialect>,
    ) -> Result<Dialect, RegistryError> {
        let settings: DialectSettings = schema.dialect_settings.unwrap_or_default();
        if settings.forced_spec_version.is_some() {
            return Ok(schema.dialect());
        }
        let dialect: Dialect = match (schema.schema.as_deref(), inherited) {
            (None, Some(dialect)) => dialect,
            (uri, _) => self.dialect_for_schema_uri(uri, schema.dialect())?,
        };
        Ok(settings.apply_overrides(dialect))
    }

    /// The dialect `$schema: uri` selects; `fallback` when absent or when a custom metaschema
    /// cannot be loaded.
    fn dialect_for_schema_uri(
        &mut self,
        uri: Option<&str>,
        fallback: Dialect,
    ) -> Result<Dialect, RegistryError> {
        let Some(uri) = uri else {
            return Ok(fallback);
        };
        if let Some(version) = SpecVersion::from_schema_uri(uri) {
            return Ok(Dialect::for_spec_version(version));
        }
        let Ok(meta_uri) = parse_uri(uri).map(|u| without_fragment(&u)) else {
            return Ok(fallback);
        };
        let metaschema: Arc<Schema> = match self.fetch(&meta_uri, None) {
            Ok(m) => m,
            Err(e) => {
                log::debug!(
                    "custom metaschema \"{uri}\" could not be loaded ({e}); using {fallback:?}"
                );
                return Ok(fallback);
            }
        };
        let Some(meta) = metaschema.as_object() else {
            return Ok(fallback);
        };
        // The metaschema's own `$schema` decides the base draft (following a chain of custom
        // metaschemas until a standard one).
        let mut base: Option<SpecVersion> = None;
        let mut next: Option<String> = meta.schema.clone();
        let mut seen: Vec<Uri> = vec![meta_uri.clone()];
        while let Some(next_uri) = next.take() {
            if let Some(version) = SpecVersion::from_schema_uri(&next_uri) {
                base = Some(version);
                break;
            }
            let Ok(parsed) = parse_uri(&next_uri).map(|u| without_fragment(&u)) else {
                break;
            };
            if seen.contains(&parsed) {
                break;
            }
            seen.push(parsed.clone());
            if let Ok(doc) = self.fetch(&parsed, None) {
                next = doc.as_object().and_then(|o| o.schema.clone());
            }
        }
        let mut dialect: Dialect =
            Dialect::for_spec_version(base.unwrap_or(SpecVersion::Draft202012));
        if let Some(Value::Object(vocabularies)) = meta.extensions.get("$vocabulary") {
            let mut active = super::dialect::VocabularySet::empty();
            for (vocabulary, required) in vocabularies {
                match VocabularyKind::from_uri(vocabulary) {
                    Some(kind) => active.insert(kind),
                    None if required.as_bool() == Some(true) => {
                        return Err(RegistryError::UnsupportedVocabulary {
                            metaschema: meta_uri.to_string(),
                            vocabulary: vocabulary.clone(),
                        });
                    }
                    None => {}
                }
            }
            active.insert(VocabularyKind::Core);
            dialect.active_vocabularies = active;
            if active.contains(VocabularyKind::FormatAssertion) {
                dialect.format_mode = FormatMode::Assertion;
            }
        }
        Ok(dialect)
    }

    /// Fetches and parses the document at `uri` (embedded metaschema, given document, or
    /// retriever), once. `draft` pins the draft of a document without `$schema`.
    fn fetch(
        &mut self,
        uri: &Uri,
        draft: Option<SpecVersion>,
    ) -> Result<Arc<Schema>, RegistryError> {
        if let Some(schema) = self.fetched.get(uri) {
            return Ok(Arc::clone(schema));
        }
        let value: Value =
            if let Some((_, text)) = METASCHEMAS.iter().find(|(known, _)| *known == uri.as_str()) {
                serde_json::from_str(text).map_err(|e| RegistryError::Parse {
                    uri: uri.to_string(),
                    reason: e.to_string(),
                })?
            } else if let Some(value) = self.documents.get(uri.as_str()) {
                value.clone()
            } else {
                self.retriever
                    .retrieve(uri.as_str())
                    .map_err(|e| RegistryError::Retrieve {
                        uri: uri.to_string(),
                        reason: e.reason,
                    })?
            };
        // A loaded document follows its own `$schema`; without one it takes `draft` (the
        // referencing resource's). Forcing a draft applies to the root document only.
        let mut settings: JsonSchemaSettings = self.settings.clone();
        settings.forced_spec_version = None;
        settings.default_spec_version = draft.or(settings.default_spec_version);
        settings.unknown_schema_uri = super::settings::UnknownSchemaUri::Fallback;
        let schema: Schema =
            Schema::new_from_serde_value(&value, &settings).map_err(|e| RegistryError::Parse {
                uri: uri.to_string(),
                reason: e.to_string(),
            })?;
        let schema: Arc<Schema> = Arc::new(schema);
        self.fetched.insert(uri.clone(), Arc::clone(&schema));
        Ok(schema)
    }

    /// Registers a resource; a URI declared at another location is a duplicate.
    fn add_resource(
        &mut self,
        uri: Uri,
        document: usize,
        pointer: Vec<String>,
        dialect: Dialect,
    ) -> Result<Scope, RegistryError> {
        if let Some(existing) = self.by_uri.get(&uri).copied() {
            let resource: &Resource = &self.registry.resources[existing.0];
            if resource.document != document || resource.pointer != pointer {
                return Err(RegistryError::DuplicateId {
                    uri: uri.to_string(),
                    first: JsonPointer::from(resource.pointer.clone()).to_string(),
                    second: JsonPointer::from(pointer).to_string(),
                });
            }
            // The same location again: a document root whose `$id` is its retrieval URI. The
            // `$id` resource replaces the retrieval one under that URI, so the root's anchors
            // are found there.
        }
        let scope: Scope = Scope(self.registry.resources.len());
        self.registry.resources.push(Resource {
            uri: uri.clone(),
            document,
            pointer,
            dialect,
            anchors: HashMap::new(),
            dynamic_anchors: HashMap::new(),
            child_ids: HashMap::new(),
            refs: HashMap::new(),
        });
        self.by_uri.insert(uri, scope);
        Ok(scope)
    }

    /// Records an anchor in `scope`; the same name at another location is a duplicate.
    fn add_anchor(
        &mut self,
        scope: Scope,
        name: &str,
        pointer: Vec<String>,
        dynamic: bool,
    ) -> Result<(), RegistryError> {
        let resource: &mut Resource = &mut self.registry.resources[scope.0];
        if let Some(existing) = resource.anchors.get(name)
            && *existing != pointer
        {
            return Err(RegistryError::DuplicateAnchor {
                uri: resource.uri.to_string(),
                anchor: name.to_string(),
            });
        }
        if dynamic {
            resource
                .dynamic_anchors
                .insert(name.to_string(), pointer.clone());
        }
        resource.anchors.insert(name.to_string(), pointer);
        Ok(())
    }

    /// Walks every schema position under `node` (at `base_pointer` in `document`, whose parent
    /// scope is `parent_scope`), registering resources and anchors and queueing `$ref`s.
    fn discover(
        &mut self,
        node: Node<'_>,
        document: usize,
        base_pointer: &[String],
        parent_scope: Scope,
    ) -> Result<(), RegistryError> {
        // Path arena: each entry is (parent entry, segment); materialised only when needed.
        let mut paths: Vec<(usize, Segment<'_>)> = Vec::new();
        let mut stack: Vec<(Node<'_>, Option<usize>, Scope)> = vec![(node, None, parent_scope)];
        let materialise = |paths: &Vec<(usize, Segment<'_>)>, at: Option<usize>| -> Vec<String> {
            let mut rev: Vec<String> = Vec::new();
            let mut cursor: Option<usize> = at;
            while let Some(i) = cursor {
                let (parent, segment) = paths[i];
                rev.push(match segment {
                    Segment::Key(k) => k.to_string(),
                    Segment::Index(i) => i.to_string(),
                });
                cursor = (parent != usize::MAX).then_some(parent);
            }
            let mut out: Vec<String> = base_pointer.to_vec();
            out.extend(rev.into_iter().rev());
            out
        };
        while let Some((node, path, parent_scope)) = stack.pop() {
            let dialect: Dialect = self.registry.resources[parent_scope.0].dialect;
            let mut scope: Scope = parent_scope;
            if let Some(id) = node.effective_id(dialect) {
                let (resource_part, fragment): (&str, Option<&str>) = match id.split_once('#') {
                    Some((resource_part, fragment)) => (resource_part, Some(fragment)),
                    None => (id, None),
                };
                if !resource_part.is_empty() {
                    let base: &Uri = &self.registry.resources[parent_scope.0].uri;
                    let uri: Uri = resolve_uri(base, resource_part)
                        .map(|u| without_fragment(&u))
                        .map_err(|reason| RegistryError::InvalidUri {
                            value: id.to_string(),
                            reason,
                        })?;
                    let settings: DialectSettings = self.dialect_settings(document);
                    let node_dialect: Dialect = if settings.forced_spec_version.is_some() {
                        dialect
                    } else {
                        let own: Dialect =
                            self.dialect_for_schema_uri(node.schema_uri(), dialect)?;
                        settings.apply_overrides(own)
                    };
                    let pointer: Vec<String> = materialise(&paths, path);
                    scope = match self.existing_at(&uri, document, &pointer) {
                        Some(existing) => existing,
                        None => self.add_resource(uri, document, pointer, node_dialect)?,
                    };
                    self.registry.resources[parent_scope.0]
                        .child_ids
                        .insert(id.to_string(), scope);
                }
                let older_than_2019: bool = dialect.keyword("$anchor").is_none();
                if let Some(name) = fragment
                    && older_than_2019
                    && !name.is_empty()
                    && !name.starts_with('/')
                {
                    let pointer: Vec<String> = materialise(&paths, path);
                    self.add_anchor(scope, name, pointer, false)?;
                }
            }
            let dialect: Dialect = self.registry.resources[scope.0].dialect;
            if dialect.keyword("$anchor").is_some()
                && let Some(name) = node.get_str("$anchor")
            {
                let pointer: Vec<String> = materialise(&paths, path);
                self.add_anchor(scope, name, pointer, false)?;
            }
            if dialect.keyword("$dynamicAnchor").is_some()
                && let Some(name) = node.get_str("$dynamicAnchor")
            {
                let pointer: Vec<String> = materialise(&paths, path);
                self.add_anchor(scope, name, pointer, true)?;
            }
            if let Some(ref_str) = node.ref_() {
                self.pending.push(PendingRef {
                    scope,
                    ref_str: ref_str.to_string(),
                });
            }
            let mut kids: Vec<(Node<'_>, Vec<Segment<'_>>)> = children(node, dialect);
            // Reverse so children are visited in schema order (deterministic errors).
            kids.reverse();
            for (child, segments) in kids {
                let mut at: usize = path.unwrap_or(usize::MAX);
                for segment in segments {
                    paths.push((at, segment));
                    at = paths.len() - 1;
                }
                stack.push((child, Some(at), scope));
            }
        }
        Ok(())
    }

    /// The resource already registered for `uri` at exactly this location (a second discovery
    /// of the same schema), if any.
    fn existing_at(&self, uri: &Uri, document: usize, pointer: &[String]) -> Option<Scope> {
        self.by_uri.get(uri).copied().filter(|scope| {
            let resource: &Resource = &self.registry.resources[scope.0];
            resource.document == document && resource.pointer == pointer && !pointer.is_empty()
        })
    }

    /// The dialect settings `document`'s root was parsed with: a forced draft covers every
    /// resource of the document, and the per-flag overrides apply to each resource's own draft.
    fn dialect_settings(&self, document: usize) -> DialectSettings {
        self.registry.documents[document]
            .root
            .schema_ref()
            .as_object()
            .and_then(|s| s.dialect_settings)
            .unwrap_or_default()
    }

    fn resolve_pending(&mut self, pending: PendingRef) -> Result<(), RegistryError> {
        if self.registry.resources[pending.scope.0]
            .refs
            .contains_key(&pending.ref_str)
        {
            return Ok(());
        }
        let ref_str: &str = &pending.ref_str;
        let target_uri: Uri = resolve_uri(&self.registry.resources[pending.scope.0].uri, ref_str)
            .map_err(|reason| RegistryError::UnresolvableRef {
            ref_str: ref_str.to_string(),
            target: ref_str.to_string(),
            reason: format!("it is not a valid URI reference: {reason}"),
        })?;
        let resource_uri: Uri = without_fragment(&target_uri);
        let unresolvable = |reason: String| RegistryError::UnresolvableRef {
            ref_str: ref_str.to_string(),
            target: target_uri.to_string(),
            reason,
        };
        let known: Option<Scope> = self.by_uri.get(&resource_uri).copied();
        let resource: Scope = if let Some(scope) = known {
            scope
        } else {
            {
                let referrer: Dialect = self.registry.resources[pending.scope.0].dialect;
                let schema: Arc<Schema> = self
                    .fetch(&resource_uri, Some(referrer.spec_version))
                    .map_err(|e| unresolvable(e.to_string()))?;
                self.add_document(
                    resource_uri.clone(),
                    DocumentRoot::Owned(schema),
                    Some(referrer),
                )?;
                self.by_uri
                    .get(&resource_uri)
                    .copied()
                    .ok_or_else(|| unresolvable("no schema resource has this URI".to_string()))?
            }
        };
        let fragment: String = match target_uri.fragment() {
            Some(fragment) => fragment
                .decode()
                .to_string()
                .map_err(|_| unresolvable("the decoded fragment is not UTF-8".to_string()))?
                .into_owned(),
            None => String::new(),
        };
        let resource_data: &Resource = &self.registry.resources[resource.0];
        let document: usize = resource_data.document;
        let pointer: Vec<String> = if fragment.is_empty() {
            resource_data.pointer.clone()
        } else if fragment.starts_with('/') {
            let parsed: JsonPointer = JsonPointer::try_from(fragment.as_str())
                .map_err(|e| unresolvable(e.to_string()))?;
            let mut pointer: Vec<String> = resource_data.pointer.clone();
            pointer.extend(parsed.segments().map(str::to_string));
            pointer
        } else {
            resource_data
                .anchors
                .get(&fragment)
                .cloned()
                .ok_or_else(|| {
                    unresolvable(format!("no anchor \"{fragment}\" in \"{resource_uri}\""))
                })?
        };
        let target: Target = self.locate(document, pointer, ref_str, &target_uri)?;
        self.registry.resources[pending.scope.0]
            .refs
            .insert(pending.ref_str, target);
        Ok(())
    }

    /// Walks `pointer` from `document`'s root, tracking the scope, and returns the target.
    fn locate(
        &mut self,
        document: usize,
        pointer: Vec<String>,
        ref_str: &str,
        target_uri: &Uri,
    ) -> Result<Target, RegistryError> {
        let root: DocumentRoot<'a> = self.registry.documents[document].root.clone();
        let not_found = |reason: &str| RegistryError::UnresolvableRef {
            ref_str: ref_str.to_string(),
            target: target_uri.to_string(),
            reason: reason.to_string(),
        };
        let not_schema = || RegistryError::RefTargetNotSchema {
            ref_str: ref_str.to_string(),
            target: target_uri.to_string(),
        };
        let mut scope_before: Scope = self.registry.documents[document].scope;
        let mut step: Step<'_> = Step::Typed(root.schema_ref());
        let mut i: usize = 0;
        while i < pointer.len() {
            // Enter the resource of the schema we are at before stepping into it.
            let scope: Scope = match step {
                Step::Typed(SchemaRef::Object(s)) => {
                    self.registry.enter_node(scope_before, Node::Typed(s))
                }
                Step::RawSchema(Value::Object(o)) => {
                    self.registry.enter_node(scope_before, Node::Raw(o))
                }
                _ => scope_before,
            };
            let dialect: Dialect = self.registry.resources[scope.0].dialect;
            let seg: &str = &pointer[i];
            let next_seg: Option<&str> = pointer.get(i + 1).map(String::as_str);
            let (next, consumed): (Step<'_>, usize) = match step {
                Step::Typed(SchemaRef::Bool(_)) | Step::NotSchema | Step::Missing => {
                    return Err(not_found("no such location"));
                }
                Step::Typed(SchemaRef::Object(s)) => typed_step(s, seg, next_seg),
                Step::RawSchema(value) => match (value, dialect.keyword(seg)) {
                    (Value::Object(o), Some(keyword)) => match (o.get(seg), keyword.subschemas) {
                        (Some(v), Some(SubschemaPosition::Single)) => (Step::RawSchema(v), 1),
                        (Some(v @ Value::Array(_)), Some(SubschemaPosition::SingleOrArray)) => {
                            (Step::RawContainer(v), 1)
                        }
                        (Some(v), Some(SubschemaPosition::SingleOrArray)) => {
                            (Step::RawSchema(v), 1)
                        }
                        (Some(v), Some(_)) => (Step::RawContainer(v), 1),
                        (Some(v), None) => (Step::RawData(v), 1),
                        (None, _) => return Err(not_found("no such location")),
                    },
                    _ => match raw_get(value, seg) {
                        Some(v) => (Step::RawData(v), 1),
                        None => return Err(not_found("no such location")),
                    },
                },
                Step::RawContainer(value) => match raw_get(value, seg) {
                    Some(v) => (Step::RawSchema(v), 1),
                    None => return Err(not_found("no such location")),
                },
                Step::RawData(value) => match raw_get(value, seg) {
                    Some(v) => (Step::RawData(v), 1),
                    None => return Err(not_found("no such location")),
                },
            };
            if matches!(next, Step::NotSchema) && i + consumed < pointer.len() {
                return Err(not_found("no such location"));
            }
            scope_before = scope;
            step = next;
            i += consumed;
        }
        match step {
            Step::Typed(_) => Ok(Target {
                schema: TargetSchema::Typed { document },
                pointer,
                scope: scope_before,
            }),
            Step::RawSchema(value) | Step::RawData(value)
                if matches!(value, Value::Object(_) | Value::Bool(_)) =>
            {
                let index: usize =
                    self.detach(document, &pointer, value, scope_before, target_uri)?;
                Ok(Target {
                    schema: TargetSchema::Detached(index),
                    pointer,
                    scope: scope_before,
                })
            }
            Step::Missing => Err(not_found("no such location")),
            Step::RawSchema(_) | Step::RawData(_) | Step::RawContainer(_) | Step::NotSchema => {
                Err(not_schema())
            }
        }
    }

    /// Parses the raw `value` at `pointer` into a detached schema (once) and discovers it.
    fn detach(
        &mut self,
        document: usize,
        pointer: &[String],
        value: &Value,
        scope: Scope,
        target_uri: &Uri,
    ) -> Result<usize, RegistryError> {
        let key: (usize, Vec<String>) = (document, pointer.to_vec());
        if let Some(index) = self.detached_at.get(&key) {
            return Ok(*index);
        }
        let dialect: Dialect = self.registry.resources[scope.0].dialect;
        let mut settings: JsonSchemaSettings = self.settings.clone();
        settings.disallow_unknown_fields = false;
        settings.forced_spec_version = Some(dialect.spec_version);
        let schema: Schema =
            Schema::new_from_serde_value(value, &settings).map_err(|e| RegistryError::Parse {
                uri: target_uri.to_string(),
                reason: e.to_string(),
            })?;
        let index: usize = self.registry.detached.len();
        self.registry.detached.push(schema);
        self.detached_at.insert(key, index);
        // Discover it so the `$ref`s inside a value no schema position reached are resolved.
        if let Value::Object(obj) = value {
            self.discover(Node::Raw(obj), document, pointer, scope)?;
        }
        Ok(index)
    }
}

/// One step of a JSON Pointer into a typed schema: the value `seg` (and, for maps and arrays,
/// `next`) selects, and how many segments it consumed.
fn typed_step<'r>(s: &'r JsonSchema, seg: &str, next: Option<&str>) -> (Step<'r>, usize) {
    let in_map = |map: Option<&'r BTreeMap<String, Schema>>| match (map, next) {
        (Some(map), Some(key)) => map
            .get(key)
            .map_or((Step::Missing, 2), |s| (Step::Typed(s.as_schema_ref()), 2)),
        _ => (Step::NotSchema, 1),
    };
    let in_list = |list: Option<&'r Vec<Schema>>| match (list, next.and_then(|n| n.parse().ok())) {
        (Some(list), Some(index)) => list
            .get::<usize>(index)
            .map_or((Step::Missing, 2), |s| (Step::Typed(s.as_schema_ref()), 2)),
        _ => (Step::NotSchema, 1),
    };
    let in_values =
        |values: Option<&'r Vec<Value>>| match (values, next.and_then(|n| n.parse().ok())) {
            (Some(values), Some(index)) => values
                .get::<usize>(index)
                .map_or((Step::Missing, 2), |v| (Step::RawData(v), 2)),
            _ => (Step::NotSchema, 1),
        };
    let single = |schema: Option<&'r Schema>| {
        schema.map_or((Step::Missing, 1), |s| (Step::Typed(s.as_schema_ref()), 1))
    };
    match seg {
        "properties" => in_map(Some(&s.properties)),
        "$defs" => in_map(s.defs.as_ref()),
        "definitions" => in_map(s.definitions.as_ref()),
        "allOf" => in_list(s.all_of.as_ref()),
        "anyOf" => in_list(s.any_of.as_ref()),
        "oneOf" => in_list(s.one_of.as_ref()),
        "items" => single(s.items.as_ref()),
        "additionalProperties" => single(s.additional_properties.as_ref()),
        "enum" => in_values(s.enum_values.as_ref()),
        "examples" => in_values(s.examples.as_ref()),
        "const" => s
            .const_value
            .as_ref()
            .map_or((Step::NotSchema, 1), |v| (Step::RawData(v), 1)),
        "default" => s
            .default_value
            .as_ref()
            .map_or((Step::NotSchema, 1), |v| (Step::RawData(v), 1)),
        _ => match s.extensions.get(seg) {
            Some(v) => (Step::RawSchema(v), 1),
            None => (Step::NotSchema, 1),
        },
    }
}

/// `value[seg]` for an object key or an array index.
fn raw_get<'r>(value: &'r Value, seg: &str) -> Option<&'r Value> {
    match value {
        Value::Object(o) => o.get(seg),
        Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    }
}

impl<'a> Registry<'a> {
    /// Builds the registry for `root` with the defaults: base URI [`DEFAULT_BASE_URI`], no
    /// documents but the embedded standard metaschemas, no retrieval.
    ///
    /// # Errors
    ///
    /// See [`RegistryBuilder::build`].
    pub fn new(root: impl Into<SchemaRef<'a>>) -> Result<Registry<'a>, RegistryError> {
        RegistryBuilder::default().build(root)
    }

    /// A builder for a registry with documents, a retriever, a base URI or settings.
    #[must_use]
    pub fn builder<'b>() -> RegistryBuilder<'b> {
        RegistryBuilder::default()
    }

    /// The root schema.
    #[must_use]
    pub fn root(&self) -> SchemaRef<'_> {
        self.documents[0].root.schema_ref()
    }

    /// The scope to evaluate the root schema in (pass it to [`Registry::enter`] with the root).
    #[must_use]
    pub fn root_scope(&self) -> Scope {
        self.documents[0].scope
    }

    /// The scope inside `schema`, which sits in `scope`: the resource `schema` declares with its
    /// `$id`, if it declares one, otherwise `scope`.
    #[must_use]
    pub fn enter(&self, scope: Scope, schema: &JsonSchema) -> Scope {
        self.enter_node(scope, Node::Typed(schema))
    }

    fn enter_node(&self, scope: Scope, node: Node<'_>) -> Scope {
        let resource: &Resource = &self.resources[scope.0];
        match node.effective_id(resource.dialect) {
            Some(id) => resource.child_ids.get(id).copied().unwrap_or(scope),
            None => scope,
        }
    }

    /// The dialect of the resource `scope` is in.
    #[must_use]
    pub fn dialect(&self, scope: Scope) -> Dialect {
        self.resources[scope.0].dialect
    }

    /// The base URI of the resource `scope` is in.
    #[must_use]
    pub fn base_uri(&self, scope: Scope) -> &str {
        self.resources[scope.0].uri.as_str()
    }

    /// The target of `ref_str`, a `$ref` written in a schema whose scope (after
    /// [`Registry::enter`]) is `scope`.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownRef`] when the registry has no such `$ref` in that scope: the schema
    /// being evaluated is not one the registry was built from.
    pub fn resolve(&self, scope: Scope, ref_str: &str) -> Result<Resolved<'_>, UnknownRef> {
        let target: &Target =
            self.resources[scope.0]
                .refs
                .get(ref_str)
                .ok_or_else(|| UnknownRef {
                    ref_str: ref_str.to_string(),
                })?;
        Ok(self.resolved(target))
    }

    /// Resolves `ref_str` in the root's scope, or, when it does not appear there, in the first
    /// resource (in discovery order) where it appears. For consumers that do not track scopes:
    /// codegen walks copies and merges of subschemas, so it resolves every `$ref` this way.
    pub(crate) fn resolve_anywhere(&self, ref_str: &str) -> Result<Resolved<'_>, UnknownRef> {
        let root: Scope = match self.root().as_object() {
            Some(root) => self.enter(self.root_scope(), root),
            None => self.root_scope(),
        };
        self.resources[root.0]
            .refs
            .get(ref_str)
            .or_else(|| self.resources.iter().find_map(|r| r.refs.get(ref_str)))
            .map(|target| self.resolved(target))
            .ok_or_else(|| UnknownRef {
                ref_str: ref_str.to_string(),
            })
    }

    fn resolved<'s>(&'s self, target: &'s Target) -> Resolved<'s> {
        let schema: SchemaRef<'_> = match target.schema {
            TargetSchema::Detached(index) => self.detached[index].as_schema_ref(),
            TargetSchema::Typed { document } => {
                let mut current: SchemaRef<'_> = self.documents[document].root.schema_ref();
                let mut i: usize = 0;
                while i < target.pointer.len() {
                    let SchemaRef::Object(s) = current else {
                        break;
                    };
                    let (next, consumed) = typed_step(
                        s,
                        &target.pointer[i],
                        target.pointer.get(i + 1).map(String::as_str),
                    );
                    match next {
                        Step::Typed(next) => current = next,
                        // Build-time `locate` only stores typed targets reached by typed steps.
                        _ => break,
                    }
                    i += consumed;
                }
                current
            }
        };
        Resolved {
            schema,
            scope: target.scope,
            pointer: &target.pointer,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Registry, RegistryError, Resolved, Scope, UnknownRef};
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::dialect::{Dialect, FormatMode, VocabularyKind, VocabularySet};
    use crate::json_schema::json_schema::{JsonSchema, Schema, SchemaRef};
    use crate::json_schema::settings::JsonSchemaSettings;
    use crate::json_schema::spec_version::SpecVersion;
    use crate::validator::{ValidationError, ValidationResult, validate_with_registry};
    use serde_json::{Value, json};
    use std::collections::HashMap;

    /// Parses `value` with default settings.
    fn parse(value: &Value) -> JsonSchema {
        JsonSchema::try_from(value).expect("parse schema")
    }

    /// The build error of a registry for `schema` with the defaults.
    fn build_error(schema: &JsonSchema) -> Result<(), RegistryError> {
        Registry::new(schema).map(|_| ())
    }

    /// Validates `instance` against `schema` through a registry with the defaults.
    fn validate_default(schema: &JsonSchema, instance: &Value) -> ValidationResult {
        let registry: Registry<'_> = Registry::new(schema).expect("build registry");
        validate_with_registry(&registry, instance)
    }

    /// The scope inside the root of `registry` (after its own `$id`, if any).
    fn root_inner_scope(registry: &Registry<'_>, schema: &JsonSchema) -> Scope {
        registry.enter(registry.root_scope(), schema)
    }

    // --- Discovery errors ---

    #[test]
    fn duplicate_id_is_a_build_error() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {
                "a": {"$id": "https://example.com/s.json"},
                "b": {"$id": "https://example.com/s.json"}
            }
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::DuplicateId {
            uri: "https://example.com/s.json".to_string(),
            first: "/$defs/a".to_string(),
            second: "/$defs/b".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn duplicate_anchor_in_one_resource_is_a_build_error() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {
                "a": {"$anchor": "x"},
                "b": {"$anchor": "x"}
            }
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::DuplicateAnchor {
            uri: "json-schema:///".to_string(),
            anchor: "x".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn same_anchor_in_two_resources_is_not_a_duplicate() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {
                "a": {"$id": "a.json", "$anchor": "x"},
                "b": {"$id": "b.json", "$anchor": "x"}
            }
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_base_uri_is_a_build_error() {
        let schema: JsonSchema = parse(&json!({}));
        let actual: Result<(), RegistryError> = Registry::builder()
            .base_uri("relative/schema.json")
            .build(&schema)
            .map(|_| ());
        let expected: Result<(), RegistryError> = Err(RegistryError::InvalidUri {
            value: "relative/schema.json".to_string(),
            reason: "it has no scheme (it is a relative reference)".to_string(),
        });
        assert_eq!(expected, actual);
    }

    // --- Resolution errors ---

    #[test]
    fn ref_to_missing_entry_of_present_defs_is_unresolvable() {
        let schema: JsonSchema =
            parse(&json!({"$defs": {"present": {}}, "$ref": "#/$defs/missing"}));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "#/$defs/missing".to_string(),
            target: "json-schema:///#/$defs/missing".to_string(),
            reason: "no such location".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_into_absent_defs_is_unresolvable() {
        let schema: JsonSchema = parse(&json!({"$ref": "#/$defs/missing"}));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "#/$defs/missing".to_string(),
            target: "json-schema:///#/$defs/missing".to_string(),
            reason: "no such location".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_unknown_anchor_is_unresolvable() {
        let schema: JsonSchema = parse(&json!({"$ref": "#nope"}));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "#nope".to_string(),
            target: "json-schema:///#nope".to_string(),
            reason: "no anchor \"nope\" in \"json-schema:///\"".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_required_is_not_a_schema() {
        let schema: JsonSchema = parse(&json!({
            "required": ["a"],
            "properties": {"p": {"$ref": "#/required"}}
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::RefTargetNotSchema {
            ref_str: "#/required".to_string(),
            target: "json-schema:///#/required".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_properties_map_is_not_a_schema() {
        let schema: JsonSchema = parse(&json!({
            "properties": {"p": {"$ref": "#/properties"}}
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::RefTargetNotSchema {
            ref_str: "#/properties".to_string(),
            target: "json-schema:///#/properties".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_raw_number_is_not_a_schema() {
        let schema: JsonSchema = parse(&json!({
            "x-defs": {"n": 5},
            "properties": {"p": {"$ref": "#/x-defs/n"}}
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::RefTargetNotSchema {
            ref_str: "#/x-defs/n".to_string(),
            target: "json-schema:///#/x-defs/n".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_with_malformed_percent_escape_is_unresolvable() {
        let schema: JsonSchema = parse(&json!({"$ref": "#/$defs/a%zz"}));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "#/$defs/a%zz".to_string(),
            target: "#/$defs/a%zz".to_string(),
            reason: "it is not a valid URI reference: invalid percent-encoded octet at index 9"
                .to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_percent_encoded_def_name_resolves() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {"a b": {"type": "string"}},
            "$ref": "#/$defs/a%20b"
        }));
        let actual: ValidationResult = validate_default(&schema, &json!(1));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    // --- Targets outside the typed model ---

    #[test]
    fn ref_into_unknown_keyword_resolves_leniently() {
        let schema: JsonSchema = parse(&json!({
            "x-defs": {"a": {"type": "string", "x-note": "kept"}},
            "properties": {"p": {"$ref": "#/x-defs/a"}}
        }));
        let actual: ValidationResult = validate_default(&schema, &json!({"p": 1}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root().push("p"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_inside_unknown_keyword_target_is_resolved() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {"s": {"type": "string"}},
            "x-defs": {"a": {"$ref": "#/$defs/s"}},
            "properties": {"p": {"$ref": "#/x-defs/a"}}
        }));
        let actual: ValidationResult = validate_default(&schema, &json!({"p": 1}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root().push("p"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_to_examples_entry_resolves() {
        let schema: JsonSchema = parse(&json!({
            "examples": [{"type": "string"}],
            "properties": {"p": {"$ref": "#/examples/0"}}
        }));
        let actual: ValidationResult = validate_default(&schema, &json!({"p": 1}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root().push("p"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn id_inside_enum_is_not_a_resource() {
        let schema: JsonSchema = parse(&json!({
            "enum": [{"$id": "https://example.com/e.json"}],
            "properties": {"p": {"$ref": "https://example.com/e.json"}}
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "https://example.com/e.json".to_string(),
            target: "https://example.com/e.json".to_string(),
            reason: RegistryError::Retrieve {
                uri: "https://example.com/e.json".to_string(),
                reason: "retrieval is disabled; no document was given for \"https://example.com/e.json\"".to_string(),
            }
            .to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn id_inside_const_is_not_a_resource() {
        let schema: JsonSchema = parse(&json!({
            "const": {"$id": "https://example.com/c.json"},
            "properties": {"p": {"$ref": "https://example.com/c.json"}}
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "https://example.com/c.json".to_string(),
            target: "https://example.com/c.json".to_string(),
            reason: RegistryError::Retrieve {
                uri: "https://example.com/c.json".to_string(),
                reason: "retrieval is disabled; no document was given for \"https://example.com/c.json\"".to_string(),
            }
            .to_string(),
        });
        assert_eq!(expected, actual);
    }

    // --- Documents, retrievers, base URIs ---

    /// Doc A (the root, at `https://example.com/a.json`) refs into doc B, which refs back.
    fn mutual_refs_root() -> JsonSchema {
        parse(&json!({
            "$defs": {"y": {"type": "integer"}},
            "properties": {"b": {"$ref": "b.json#/$defs/x"}}
        }))
    }

    fn mutual_refs_registry(root: &JsonSchema) -> Registry<'_> {
        Registry::builder()
            .base_uri("https://example.com/a.json")
            .document(
                "https://example.com/b.json",
                json!({
                    "$defs": {
                        "x": {
                            "type": "object",
                            "properties": {"a": {"$ref": "a.json#/$defs/y"}}
                        }
                    }
                }),
            )
            .build(root)
            .expect("build registry")
    }

    #[test]
    fn mutual_refs_across_documents_reject_through_both_hops() {
        let root: JsonSchema = mutual_refs_root();
        let registry: Registry<'_> = mutual_refs_registry(&root);
        let actual: ValidationResult = validate_with_registry(&registry, &json!({"b": {"a": "s"}}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("b").push("a"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn mutual_refs_across_documents_accept_through_both_hops() {
        let root: JsonSchema = mutual_refs_root();
        let registry: Registry<'_> = mutual_refs_registry(&root);
        let actual: ValidationResult = validate_with_registry(&registry, &json!({"b": {"a": 1}}));
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn mutual_refs_first_hop_rejects_wrong_container() {
        let root: JsonSchema = mutual_refs_root();
        let registry: Registry<'_> = mutual_refs_registry(&root);
        let actual: ValidationResult = validate_with_registry(&registry, &json!({"b": 1}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedObject {
            instance_path: JsonPointer::root().push("b"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn hash_map_retriever_supplies_remote_document() {
        let root: JsonSchema = parse(&json!({"$ref": "https://example.com/remote.json"}));
        let documents: HashMap<String, Value> = HashMap::from([(
            "https://example.com/remote.json".to_string(),
            json!({"type": "string"}),
        )]);
        let registry: Registry<'_> = Registry::builder()
            .retriever(documents)
            .build(&root)
            .expect("build registry");
        let actual: ValidationResult = validate_with_registry(&registry, &json!(1));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn differently_spelled_ref_reaches_normalised_resource() {
        let root: JsonSchema = parse(&json!({
            "$defs": {"a": {"$id": "https://example.com/a.json", "type": "string"}},
            "$ref": "HTTPS://EXAMPLE.com:443/./a.json"
        }));
        let registry: Registry<'_> = Registry::new(&root).expect("build registry");
        let actual: ValidationResult = validate_with_registry(&registry, &json!(1));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn document_uri_is_normalised_when_given_to_builder() {
        let root: JsonSchema = parse(&json!({"$ref": "https://example.com/remote.json"}));
        let registry: Registry<'_> = Registry::builder()
            .document("HTTPS://Example.com/remote.json", json!({"type": "string"}))
            .build(&root)
            .expect("build registry");
        let actual: ValidationResult = validate_with_registry(&registry, &json!(1));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedString {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_path_ref_against_urn_base_is_unresolvable() {
        let root: JsonSchema = parse(&json!({"$id": "urn:example:root", "$ref": "other.json"}));
        let actual: Result<(), RegistryError> = build_error(&root);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "other.json".to_string(),
            target: "other.json".to_string(),
            reason: "it is not a valid URI reference: when base has a rootless path and no \
                     authority, reference should either have scheme, be empty or start with '#'"
                .to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn id_with_invalid_syntax_is_a_build_error() {
        let root: JsonSchema = parse(&json!({"$defs": {"a": {"$id": "a b.json"}}}));
        let actual: Result<(), RegistryError> = build_error(&root);
        let expected: Result<(), RegistryError> = Err(RegistryError::InvalidUri {
            value: "a b.json".to_string(),
            reason: "unexpected character at index 1".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn base_uri_is_normalised() {
        let root: JsonSchema = parse(&json!({}));
        let registry: Registry<'_> = Registry::builder()
            .base_uri("HTTP://Example.COM:80/a/../root.json")
            .build(&root)
            .expect("build registry");
        let expected: &str = "http://example.com/root.json";
        let actual: &str = registry.base_uri(registry.root_scope());
        assert_eq!(expected, actual);
    }

    #[test]
    fn hash_map_retriever_without_document_is_unresolvable() {
        let root: JsonSchema = parse(&json!({"$ref": "https://example.com/remote.json"}));
        let documents: HashMap<String, Value> = HashMap::new();
        let actual: Result<(), RegistryError> = Registry::builder()
            .retriever(documents)
            .build(&root)
            .map(|_| ());
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "https://example.com/remote.json".to_string(),
            target: "https://example.com/remote.json".to_string(),
            reason: "could not retrieve \"https://example.com/remote.json\": no in-memory document for \"https://example.com/remote.json\"".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn offline_retriever_makes_remote_ref_unresolvable() {
        let root: JsonSchema = parse(&json!({"$ref": "https://example.com/remote.json#/$defs/a"}));
        let actual: Result<(), RegistryError> = build_error(&root);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "https://example.com/remote.json#/$defs/a".to_string(),
            target: "https://example.com/remote.json#/$defs/a".to_string(),
            reason: "could not retrieve \"https://example.com/remote.json\": retrieval is disabled; no document was given for \"https://example.com/remote.json\"".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn document_that_is_not_a_schema_is_unresolvable() {
        let root: JsonSchema = parse(&json!({"$ref": "https://example.com/bad.json"}));
        let actual: Result<(), RegistryError> = Registry::builder()
            .document("https://example.com/bad.json", json!(5))
            .build(&root)
            .map(|_| ());
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "https://example.com/bad.json".to_string(),
            target: "https://example.com/bad.json".to_string(),
            reason: "\"https://example.com/bad.json\" is not a valid schema: invalid JSON Schema: invalid type: integer `5`, expected a schema (an object or a boolean)".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn nested_relative_id_changes_base_for_relative_ref() {
        let schema: JsonSchema = parse(&json!({
            "$id": "https://example.com/root.json",
            "$defs": {
                "folder": {"$id": "folder/", "$ref": "item.json"},
                "item": {"$id": "folder/item.json", "type": "integer"}
            },
            "properties": {"p": {"$ref": "folder/"}}
        }));
        let actual: ValidationResult = validate_default(&schema, &json!({"p": "s"}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("p"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn entering_nested_id_moves_base_uri() {
        let schema: JsonSchema = parse(&json!({
            "$id": "https://example.com/root.json",
            "$defs": {"inner": {"$id": "folder/inner.json"}}
        }));
        let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
        let root_scope: Scope = root_inner_scope(&registry, &schema);
        let inner: &JsonSchema = schema.defs.as_ref().expect("$defs")["inner"]
            .as_object()
            .expect("object");
        let scope: Scope = registry.enter(root_scope, inner);
        let actual: String = registry.base_uri(scope).to_string();
        let expected: String = "https://example.com/folder/inner.json".to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_ref_sibling_id_is_ignored_for_base_uri() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_serde_value(
            &json!({
                "$id": "https://example.com/root.json",
                "definitions": {
                    "a": {"$id": "other/", "$ref": "b.json"},
                    "b": {"$id": "b.json", "type": "integer"}
                },
                "properties": {"p": {"$ref": "#/definitions/a"}}
            }),
            &settings,
        )
        .expect("parse schema");
        let actual: ValidationResult = validate_default(&schema, &json!({"p": "s"}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("p"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft2020_ref_sibling_id_changes_base_uri() {
        let schema: JsonSchema = parse(&json!({
            "$id": "https://example.com/root.json",
            "$defs": {
                "a": {"$id": "other/", "$ref": "b.json"},
                "b": {"$id": "b.json", "type": "integer"}
            }
        }));
        let actual: Result<(), RegistryError> = build_error(&schema);
        let expected: Result<(), RegistryError> = Err(RegistryError::UnresolvableRef {
            ref_str: "b.json".to_string(),
            target: "https://example.com/other/b.json".to_string(),
            reason: "could not retrieve \"https://example.com/other/b.json\": retrieval is disabled; no document was given for \"https://example.com/other/b.json\"".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft7_plain_name_fragment_id_is_an_anchor() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_serde_value(
            &json!({
                "definitions": {"a": {"$id": "#foo", "type": "integer"}},
                "properties": {"p": {"$ref": "#foo"}}
            }),
            &settings,
        )
        .expect("parse schema");
        let actual: ValidationResult = validate_default(&schema, &json!({"p": "s"}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("p"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn anchor_resolves_to_its_schema() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {"a": {"$anchor": "foo", "type": "integer"}},
            "properties": {"p": {"$ref": "#foo"}}
        }));
        let actual: ValidationResult = validate_default(&schema, &json!({"p": "s"}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("p"),
            got: "string".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    // --- Metaschemas and dialects ---

    fn embedded_metaschema_ref() -> JsonSchema {
        parse(&json!({"$ref": "https://json-schema.org/draft/2020-12/schema"}))
    }

    #[test]
    fn embedded_metaschema_rejects_negative_min_length() {
        let schema: JsonSchema = embedded_metaschema_ref();
        let actual: ValidationResult = validate_default(&schema, &json!({"minLength": -1}));
        let expected: ValidationResult = Err(vec![ValidationError::BelowMinimum {
            instance_path: JsonPointer::root().push("minLength"),
            minimum: 0.into(),
            actual: (-1).into(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn embedded_metaschema_accepts_positive_min_length() {
        let schema: JsonSchema = embedded_metaschema_ref();
        let actual: ValidationResult = validate_default(&schema, &json!({"minLength": 1}));
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    /// A 2020-12 metaschema at `https://example.com/meta` with the given `$vocabulary`.
    fn custom_metaschema(vocabulary: &Value) -> Value {
        json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/meta",
            "$vocabulary": vocabulary
        })
    }

    fn core_applicator_vocabulary() -> Value {
        json!({
            "https://json-schema.org/draft/2020-12/vocab/core": true,
            "https://json-schema.org/draft/2020-12/vocab/applicator": true
        })
    }

    fn custom_meta_root() -> JsonSchema {
        parse(&json!({"$schema": "https://example.com/meta", "minimum": 5}))
    }

    #[test]
    fn custom_metaschema_vocabulary_sets_root_dialect() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&core_applicator_vocabulary()),
            )
            .build(&root)
            .expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect {
            active_vocabularies: [VocabularyKind::Core, VocabularyKind::Applicator]
                .into_iter()
                .collect::<VocabularySet>(),
            ..Dialect::for_spec_version(SpecVersion::Draft202012)
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_without_validation_vocabulary_ignores_minimum() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&core_applicator_vocabulary()),
            )
            .build(&root)
            .expect("build registry");
        let actual: ValidationResult = validate_with_registry(&registry, &json!(1));
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_vocabulary_always_includes_core() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&json!({
                    "https://json-schema.org/draft/2020-12/vocab/validation": true
                })),
            )
            .build(&root)
            .expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect {
            active_vocabularies: [VocabularyKind::Core, VocabularyKind::Validation]
                .into_iter()
                .collect::<VocabularySet>(),
            ..Dialect::for_spec_version(SpecVersion::Draft202012)
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_format_assertion_vocabulary_asserts_format() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&json!({
                    "https://json-schema.org/draft/2020-12/vocab/core": true,
                    "https://json-schema.org/draft/2020-12/vocab/format-assertion": true
                })),
            )
            .build(&root)
            .expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect {
            active_vocabularies: [VocabularyKind::Core, VocabularyKind::FormatAssertion]
                .into_iter()
                .collect::<VocabularySet>(),
            format_mode: FormatMode::Assertion,
            ..Dialect::for_spec_version(SpecVersion::Draft202012)
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_requiring_unknown_vocabulary_is_a_build_error() {
        let root: JsonSchema = custom_meta_root();
        let actual: Result<(), RegistryError> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&json!({
                    "https://json-schema.org/draft/2020-12/vocab/core": true,
                    "https://example.com/vocab/x": true
                })),
            )
            .build(&root)
            .map(|_| ());
        let expected: Result<(), RegistryError> = Err(RegistryError::UnsupportedVocabulary {
            metaschema: "https://example.com/meta".to_string(),
            vocabulary: "https://example.com/vocab/x".to_string(),
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_optional_unknown_vocabulary_is_ignored() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                custom_metaschema(&json!({
                    "https://json-schema.org/draft/2020-12/vocab/core": true,
                    "https://json-schema.org/draft/2020-12/vocab/applicator": true,
                    "https://example.com/vocab/x": false
                })),
            )
            .build(&root)
            .expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect {
            active_vocabularies: [VocabularyKind::Core, VocabularyKind::Applicator]
                .into_iter()
                .collect::<VocabularySet>(),
            ..Dialect::for_spec_version(SpecVersion::Draft202012)
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn custom_metaschema_chain_takes_base_draft_from_standard_ancestor() {
        let root: JsonSchema = parse(&json!({"$schema": "https://example.com/meta"}));
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/meta",
                json!({
                    "$schema": "https://example.com/meta-base",
                    "$vocabulary": {
                        "https://json-schema.org/draft/2019-09/vocab/core": true,
                        "https://json-schema.org/draft/2019-09/vocab/validation": true
                    }
                }),
            )
            .document(
                "https://example.com/meta-base",
                json!({"$schema": "https://json-schema.org/draft/2019-09/schema"}),
            )
            .build(&root)
            .expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect {
            active_vocabularies: [VocabularyKind::Core, VocabularyKind::Validation]
                .into_iter()
                .collect::<VocabularySet>(),
            ..Dialect::for_spec_version(SpecVersion::Draft201909)
        };
        assert_eq!(expected, actual);
    }

    #[test]
    fn unloadable_custom_metaschema_falls_back_to_default_dialect() {
        let root: JsonSchema = custom_meta_root();
        let registry: Registry<'_> = Registry::new(&root).expect("build registry");
        let actual: Dialect = registry.dialect(registry.root_scope());
        let expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft202012);
        assert_eq!(expected, actual);
    }

    /// A 2020-12 root with a draft-4 resource and a 2020-12 sibling, both `"type": "integer"`.
    fn mixed_dialect_root() -> JsonSchema {
        parse(&json!({
            "$defs": {
                "old": {
                    "$id": "old.json",
                    "$schema": "http://json-schema.org/draft-04/schema#",
                    "type": "integer"
                },
                "new": {"type": "integer"}
            },
            "properties": {
                "old": {"$ref": "old.json"},
                "new": {"$ref": "#/$defs/new"}
            }
        }))
    }

    #[test]
    fn draft4_resource_in_2020_12_root_rejects_zero_fraction_float() {
        let schema: JsonSchema = mixed_dialect_root();
        let actual: ValidationResult = validate_default(&schema, &json!({"old": 1.0}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("old"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft2020_sibling_of_draft4_resource_accepts_zero_fraction_float() {
        let schema: JsonSchema = mixed_dialect_root();
        let actual: ValidationResult = validate_default(&schema, &json!({"new": 1.0}));
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    /// [`mixed_dialect_root`]'s schema text parsed with `settings`.
    fn mixed_dialect_root_with(settings: &JsonSchemaSettings) -> JsonSchema {
        let value: Value = serde_json::to_value(mixed_dialect_root()).expect("serialize");
        JsonSchema::new_from_serde_value(&value, settings).expect("parse schema")
    }

    #[test]
    fn forced_spec_version_overrides_embedded_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft202012)
            .build();
        let schema: JsonSchema = mixed_dialect_root_with(&settings);
        let actual: ValidationResult = validate_default(&schema, &json!({"old": 1.0}));
        let expected: ValidationResult = Ok(());
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_spec_version_yields_to_embedded_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .default_spec_version(SpecVersion::Draft07)
            .build();
        let schema: JsonSchema = mixed_dialect_root_with(&settings);
        let actual: ValidationResult = validate_default(&schema, &json!({"old": 1.0}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("old"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_spec_version_applies_to_root_without_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .default_spec_version(SpecVersion::Draft04)
            .build();
        let schema: JsonSchema = mixed_dialect_root_with(&settings);
        let actual: ValidationResult = validate_default(&schema, &json!({"new": 1.0}));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root().push("new"),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_mode_override_applies_to_embedded_resource_with_own_schema_keyword() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .format_mode(FormatMode::Annotation)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_serde_value(
            &json!({
                "$defs": {"inner": {
                    "$id": "inner.json",
                    "$schema": "http://json-schema.org/draft-07/schema#"
                }},
                "$ref": "inner.json"
            }),
            &settings,
        )
        .expect("parse schema");
        let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
        let resolved: Resolved<'_> = registry
            .resolve(root_inner_scope(&registry, &schema), "inner.json")
            .expect("resolve");
        let inner: Scope = registry.enter(
            resolved.scope,
            resolved.schema.as_object().expect("schema object"),
        );
        let mut expected: Dialect = Dialect::for_spec_version(SpecVersion::Draft07);
        expected.format_mode = FormatMode::Annotation;
        let actual: Dialect = registry.dialect(inner);
        assert_eq!(expected, actual);
    }

    #[test]
    fn forced_spec_version_does_not_apply_to_loaded_document() {
        let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
            .forced_spec_version(SpecVersion::Draft202012)
            .build();
        let schema: JsonSchema = JsonSchema::new_from_serde_value(
            &json!({"$ref": "https://example.com/old.json"}),
            &settings,
        )
        .expect("parse schema");
        let registry: Registry<'_> = Registry::builder()
            .document(
                "https://example.com/old.json",
                json!({"$schema": "http://json-schema.org/draft-04/schema#", "type": "integer"}),
            )
            .build(&schema)
            .expect("build registry");
        let actual: ValidationResult = validate_with_registry(&registry, &json!(1.0));
        let expected: ValidationResult = Err(vec![ValidationError::ExpectedInteger {
            instance_path: JsonPointer::root(),
            got: "number".to_string(),
        }]);
        assert_eq!(expected, actual);
    }

    // --- Registry::resolve and Resolved ---

    #[test]
    fn resolve_returns_target_scope_and_pointer() {
        let schema: JsonSchema = parse(&json!({
            "$defs": {"a": {"type": "string"}},
            "$ref": "#/$defs/a"
        }));
        let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
        let scope: Scope = root_inner_scope(&registry, &schema);
        let target: &Schema = &schema.defs.as_ref().expect("$defs")["a"];
        let pointer: Vec<String> = vec!["$defs".to_string(), "a".to_string()];
        let actual: Result<Resolved<'_>, UnknownRef> = registry.resolve(scope, "#/$defs/a");
        let expected: Result<Resolved<'_>, UnknownRef> = Ok(Resolved {
            schema: SchemaRef::from(target),
            scope: registry.root_scope(),
            pointer: &pointer,
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn resolve_unknown_ref_is_an_error() {
        let schema: JsonSchema = parse(&json!({}));
        let registry: Registry<'_> = Registry::new(&schema).expect("build registry");
        let scope: Scope = root_inner_scope(&registry, &schema);
        let actual: Result<Resolved<'_>, UnknownRef> = registry.resolve(scope, "#/$defs/a");
        let expected: Result<Resolved<'_>, UnknownRef> = Err(UnknownRef {
            ref_str: "#/$defs/a".to_string(),
        });
        assert_eq!(expected, actual);
    }

    /// The definition name of the target of `ref_str`, a `$ref` on the root of `schema`.
    fn definition_name_of(schema: &JsonSchema, ref_str: &str) -> Option<String> {
        let registry: Registry<'_> = Registry::new(schema).expect("build registry");
        let scope: Scope = root_inner_scope(&registry, schema);
        registry
            .resolve(scope, ref_str)
            .expect("resolve")
            .definition_name()
            .map(str::to_string)
    }

    #[test]
    fn definition_name_of_defs_entry() {
        let schema: JsonSchema = parse(&json!({"$defs": {"Name": {}}, "$ref": "#/$defs/Name"}));
        let actual: Option<String> = definition_name_of(&schema, "#/$defs/Name");
        let expected: Option<String> = Some("Name".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn definition_name_of_definitions_entry() {
        let schema: JsonSchema =
            parse(&json!({"definitions": {"Name": {}}, "$ref": "#/definitions/Name"}));
        let actual: Option<String> = definition_name_of(&schema, "#/definitions/Name");
        let expected: Option<String> = Some("Name".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn definition_name_of_document_root_is_none() {
        let schema: JsonSchema = parse(&json!({"$ref": "#"}));
        let actual: Option<String> = definition_name_of(&schema, "#");
        let expected: Option<String> = None;
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_owned_keeps_root() {
        let schema: JsonSchema = parse(&json!({"type": "string"}));
        let registry: Registry<'static> = Registry::builder()
            .build_owned(Schema::from(schema.clone()))
            .expect("build registry");
        let actual: SchemaRef<'_> = registry.root();
        let expected: SchemaRef<'_> = SchemaRef::from(&schema);
        assert_eq!(expected, actual);
    }
}
