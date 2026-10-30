//! The compiled form of a schema: an arena of [`Node`]s linked by [`NodeId`], built once from a
//! [`Registry`] so validation never re-reads the schema model.
//!
//! Each node holds the keywords of one schema object as a closed enum ([`Keyword`]), ordered so
//! the cheap assertions run before the applicators and `unevaluated*` runs last. Every `$ref` is
//! resolved to a [`NodeId`] at compile time; a schema reached twice (two `$ref`s to one
//! definition, a recursive schema) is compiled once. Regular expressions are compiled once per
//! distinct pattern.

use super::format::Format;
use crate::error_list::ErrorList;
use crate::json_pointer::JsonPointer;
use crate::json_schema::{
    Dialect, FormatMode, IntegerSemantics, JsonSchema, Keyword as DialectKeyword, RefSiblings,
    Registry, Schema, SchemaRef, Scope, TypeSet, VocabularyKind,
};
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use std::collections::{BTreeSet, HashMap};
use std::fmt;

/// A problem that keeps a schema from compiling, with its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// A `pattern` or `patternProperties` key that is not a valid ECMA-262 regular expression.
    InvalidPattern {
        /// JSON Pointer to the schema object holding the pattern, within its document.
        location: String,
        /// The pattern as written.
        pattern: String,
        /// Why the regular expression engine refused it.
        reason: String,
    },
    /// A keyword the model keeps raw whose value has the wrong shape (`"minContains": "x"`).
    InvalidKeywordValue {
        /// JSON Pointer to the schema object holding the keyword, within its document.
        location: String,
        /// The keyword.
        keyword: String,
        /// The shape the keyword needs.
        expected: &'static str,
    },
    /// A `$ref` the registry does not know: the schema was not part of the registry it was
    /// compiled from.
    UnknownRef {
        /// JSON Pointer to the schema object holding the reference, within its document.
        location: String,
        /// The reference as written.
        ref_str: String,
    },
}

/// Every problem found while compiling a schema (see [`CompiledSchema::new`]).
pub type CompileErrors = ErrorList<CompileError>;

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |location: &str| -> String {
            if location.is_empty() {
                "root".to_string()
            } else {
                location.to_string()
            }
        };
        match self {
            CompileError::InvalidPattern {
                location,
                pattern,
                reason,
            } => write!(
                f,
                "{}: invalid regular expression \"{pattern}\": {reason}",
                at(location)
            ),
            CompileError::InvalidKeywordValue {
                location,
                keyword,
                expected,
            } => write!(
                f,
                "{}: the value of \"{keyword}\" must be {expected}",
                at(location)
            ),
            CompileError::UnknownRef { location, ref_str } => write!(
                f,
                "{}: $ref \"{ref_str}\" is not in the schema's registry",
                at(location)
            ),
        }
    }
}

impl std::error::Error for CompileError {}

/// The index of a [`Node`] in a [`CompiledSchema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct NodeId(u32);

impl NodeId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// The index of an interned regular expression in a [`CompiledSchema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegexId(u32);

/// A compiled regular expression and its source.
#[derive(Debug)]
pub(crate) struct Pattern {
    pub(crate) source: String,
    pub(crate) regex: regress::Regex,
}

/// One schema object: its keywords and the resource it sits in.
#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) keywords: Box<[Keyword]>,
    /// The schema resource the keywords are evaluated in (its dynamic scope entry).
    pub(crate) scope: Scope,
    /// Whether the node has `unevaluatedProperties` / `unevaluatedItems`, so its keywords'
    /// annotations must be collected.
    pub(crate) has_unevaluated: bool,
}

/// How a property or item applicator sees an object's members: the names `properties` lists
/// and the patterns `patternProperties` lists, which `additionalProperties` skips.
#[derive(Debug)]
pub(crate) struct KnownProperties {
    /// Sorted, for binary search.
    pub(crate) names: Box<[Box<str>]>,
    pub(crate) patterns: Box<[RegexId]>,
}

/// A `dependentRequired` entry: a property and the names it requires when present.
pub(crate) type Dependency = (Box<str>, Box<[Box<str>]>);

/// One keyword, compiled.
#[derive(Debug)]
pub(crate) enum Keyword {
    /// The boolean schema `false`.
    False,
    Type {
        types: TypeSet,
        integer_semantics: IntegerSemantics,
    },
    Const(Value),
    Enum(Box<[Value]>),
    Minimum(Number),
    Maximum(Number),
    ExclusiveMinimum(Number),
    ExclusiveMaximum(Number),
    MultipleOf(Number),
    MinLength(u64),
    MaxLength(u64),
    Pattern(RegexId),
    Format(Format),
    MinItems(u64),
    MaxItems(u64),
    UniqueItems,
    Required(Box<[Box<str>]>),
    MinProperties(u64),
    MaxProperties(u64),
    /// `dependentRequired` and the array form of `dependencies`: (property, names it requires).
    DependentRequired(Box<[Dependency]>),
    /// `additionalProperties: false`: every member outside `known` is an error of its own.
    NoAdditionalProperties(KnownProperties),
    /// `$ref`: evaluated in place, with the in-place loop guard.
    Ref {
        target: NodeId,
        ref_str: Box<str>,
    },
    /// `$dynamicRef` whose initial target declares the matching `$dynamicAnchor`: the outermost
    /// resource in the dynamic scope that declares `anchor` wins, else `fallback`.
    DynamicRef {
        fallback: NodeId,
        anchor: Box<str>,
        ref_str: Box<str>,
    },
    /// `$recursiveRef` whose initial target has `$recursiveAnchor: true`: the outermost resource
    /// in the dynamic scope whose root has it wins, else `fallback`.
    RecursiveRef {
        fallback: NodeId,
        ref_str: Box<str>,
    },
    /// Sorted by name, for binary search.
    Properties(Box<[(Box<str>, NodeId)]>),
    PatternProperties(Box<[(RegexId, NodeId)]>),
    AdditionalProperties {
        node: NodeId,
        known: KnownProperties,
    },
    PropertyNames(NodeId),
    /// `dependentSchemas` and the schema form of `dependencies`.
    DependentSchemas(Box<[(Box<str>, NodeId)]>),
    /// `prefixItems` + `items` (2020-12), or array-form `items` + `additionalItems` and
    /// single-schema `items` (earlier drafts), normalised to one tuple node.
    Items {
        prefix: Box<[NodeId]>,
        rest: Option<NodeId>,
    },
    Contains {
        node: NodeId,
        min: u64,
        max: Option<u64>,
        /// Whether matched items count as evaluated for `unevaluatedItems` (2020-12).
        annotates: bool,
    },
    AllOf(Box<[NodeId]>),
    AnyOf(Box<[NodeId]>),
    OneOf(Box<[NodeId]>),
    Not(NodeId),
    IfThenElse {
        condition: NodeId,
        then: Option<NodeId>,
        otherwise: Option<NodeId>,
    },
    UnevaluatedProperties(NodeId),
    UnevaluatedItems(NodeId),
}

/// A schema compiled for validation. Build it once with [`CompiledSchema::new`] and validate
/// any number of instances with [`CompiledSchema::validate`] or [`CompiledSchema::is_valid`].
/// It owns everything it needs: the registry it was compiled from can be dropped.
#[derive(Debug)]
pub struct CompiledSchema {
    pub(crate) nodes: Vec<Node>,
    pub(crate) root: NodeId,
    pub(crate) patterns: Vec<Pattern>,
    /// `$dynamicAnchor` targets: resource → (anchor name, node) for every anchor name a dynamic
    /// `$dynamicRef` uses.
    pub(crate) dynamic_anchors: HashMap<Scope, Vec<(Box<str>, NodeId)>>,
    /// Resource roots with `$recursiveAnchor: true`: resource → node.
    pub(crate) recursive_anchors: HashMap<Scope, NodeId>,
    /// Whether any dynamic reference needs the dynamic scope tracked while evaluating.
    pub(crate) uses_dynamic_scope: bool,
}

impl CompiledSchema {
    /// Compiles the root schema of `registry` (and everything it references).
    ///
    /// # Errors
    ///
    /// Returns every [`CompileError`] found: an invalid regular expression, a keyword the model
    /// keeps raw with a value of the wrong shape, or a `$ref` the registry does not know.
    pub fn new(registry: &Registry<'_>) -> Result<CompiledSchema, CompileErrors> {
        Compiler::new(registry).compile()
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub(crate) fn pattern(&self, id: RegexId) -> &Pattern {
        &self.patterns[id.0 as usize]
    }
}

/// A schema position seen while compiling: a boolean schema, a schema object of the typed model,
/// or a raw schema object the model keeps in `extensions` (under `not`, `if`, ...).
#[derive(Debug, Clone, Copy)]
enum Source<'r> {
    Bool(bool),
    Typed(&'r JsonSchema),
    Raw(&'r Map<String, Value>),
}

impl<'r> Source<'r> {
    fn from_schema_ref(schema: SchemaRef<'r>) -> Source<'r> {
        match schema {
            SchemaRef::Bool(b) => Source::Bool(b),
            SchemaRef::Object(o) => Source::Typed(o),
        }
    }

    fn from_schema(schema: &'r Schema) -> Source<'r> {
        Source::from_schema_ref(schema.as_schema_ref())
    }

    fn from_value(value: &'r Value) -> Option<Source<'r>> {
        match value {
            Value::Bool(b) => Some(Source::Bool(*b)),
            Value::Object(o) => Some(Source::Raw(o)),
            _ => None,
        }
    }

    /// The key compiled nodes are shared by: the schema object's address (a boolean schema
    /// gets a fixed key).
    fn key(self) -> usize {
        match self {
            Source::Bool(b) => usize::from(b),
            Source::Typed(s) => std::ptr::from_ref(s) as usize,
            Source::Raw(o) => std::ptr::from_ref(o) as usize,
        }
    }

    /// A keyword the typed model does not hold (or any keyword of a raw object).
    fn raw(self, key: &str) -> Option<&'r Value> {
        match self {
            Source::Bool(_) => None,
            Source::Typed(s) => s.extensions.get(key),
            Source::Raw(o) => o.get(key),
        }
    }
}

/// One step of a schema location, for error messages.
#[derive(Debug, Clone, Copy)]
enum Step<'r> {
    Key(&'r str),
    Index(usize),
}

/// A schema waiting to be compiled into the node `id`.
struct Pending<'r> {
    id: NodeId,
    source: Source<'r>,
    /// The scope the schema sits in, before entering its own `$id`.
    scope: Scope,
    /// Its location: an entry in `Compiler::locations`.
    location: usize,
}

/// Build-time state.
struct Compiler<'r, 'a> {
    registry: &'r Registry<'a>,
    nodes: Vec<Option<Node>>,
    by_source: HashMap<(usize, Scope), NodeId>,
    queue: Vec<Pending<'r>>,
    /// Location arena: (parent entry, step); entry 0 is a document root.
    locations: Vec<(usize, Option<Step<'r>>)>,
    /// The pointer segments of `$ref` targets, by location entry, so their locations start at
    /// the target rather than at the referrer.
    location_bases: HashMap<usize, &'r [String]>,
    patterns: Vec<Pattern>,
    pattern_ids: HashMap<&'r str, RegexId>,
    dynamic_names: BTreeSet<&'r str>,
    uses_recursive_ref: bool,
    uses_dynamic_scope: bool,
    errors: Vec<CompileError>,
}

/// The schema object's keywords, read from the typed model and its `extensions` (or from a raw
/// object), each only when the dialect defines it.
#[derive(Default)]
struct Parts<'r> {
    ref_: Option<&'r str>,
    dynamic_ref: Option<&'r str>,
    recursive_ref: Option<&'r str>,
    types: Option<TypeSet>,
    const_value: Option<&'r Value>,
    enum_values: Option<&'r [Value]>,
    minimum: Option<&'r Number>,
    maximum: Option<&'r Number>,
    exclusive_minimum: Option<&'r Number>,
    exclusive_maximum: Option<&'r Number>,
    multiple_of: Option<&'r Number>,
    min_length: Option<u64>,
    max_length: Option<u64>,
    pattern: Option<&'r str>,
    format: Option<&'r str>,
    min_items: Option<u64>,
    max_items: Option<u64>,
    unique_items: bool,
    required: Vec<&'r str>,
    min_properties: Option<u64>,
    max_properties: Option<u64>,
    dependent_required: Vec<(&'r str, Vec<&'r str>)>,
    properties: Vec<(&'r str, Source<'r>)>,
    pattern_properties: Vec<(&'r str, Source<'r>)>,
    additional_properties: Option<Source<'r>>,
    property_names: Option<Source<'r>>,
    dependent_schemas: Vec<(&'r str, Source<'r>)>,
    prefix_items: Vec<Source<'r>>,
    items: Option<Source<'r>>,
    contains: Option<Source<'r>>,
    min_contains: Option<u64>,
    max_contains: Option<u64>,
    all_of: Option<Vec<Source<'r>>>,
    any_of: Option<Vec<Source<'r>>>,
    one_of: Option<Vec<Source<'r>>>,
    not: Option<Source<'r>>,
    if_: Option<Source<'r>>,
    then: Option<Source<'r>>,
    else_: Option<Source<'r>>,
    unevaluated_properties: Option<Source<'r>>,
    unevaluated_items: Option<Source<'r>>,
}

/// A numeric keyword's value and how to lower it.
type NumericKeyword<'n> = (Option<&'n Number>, fn(Number) -> Keyword);

/// A schema-list keyword: its name, its subschemas, and how to lower it.
type ListKeyword<'p, 'r> = (
    &'static str,
    &'p Option<Vec<Source<'r>>>,
    fn(Box<[NodeId]>) -> Keyword,
);

/// Where a subschema found in [`Parts`] sits, for its location: the keyword and, for maps and
/// arrays, the key or index.
type Child<'r> = (Source<'r>, &'r str, Option<Step<'r>>);

/// 2^64: every `f64` below it converts to a `u64` exactly.
const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;

/// A non-negative integer written as an integer or as a number with a zero fraction.
fn count(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| *f >= 0.0 && f.fract() == 0.0 && *f < U64_LIMIT)
                .map(|f| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "checked non-negative, integral and in range above"
                    )]
                    let n: u64 = f as u64;
                    n
                })
        }),
        _ => None,
    }
}

impl<'r, 'a> Compiler<'r, 'a> {
    fn new(registry: &'r Registry<'a>) -> Self {
        Compiler {
            registry,
            nodes: Vec::new(),
            by_source: HashMap::new(),
            queue: Vec::new(),
            locations: vec![(usize::MAX, None)],
            location_bases: HashMap::new(),
            patterns: Vec::new(),
            pattern_ids: HashMap::new(),
            dynamic_names: BTreeSet::new(),
            uses_recursive_ref: false,
            uses_dynamic_scope: false,
            errors: Vec::new(),
        }
    }

    fn compile(mut self) -> Result<CompiledSchema, CompileErrors> {
        let root_source: Source<'r> = Source::from_schema_ref(self.registry.root());
        let root: NodeId = self.node_for(root_source, self.registry.root_scope(), 0);
        let mut dynamic_anchors: HashMap<Scope, Vec<(Box<str>, NodeId)>> = HashMap::new();
        let mut recursive_anchors: HashMap<Scope, NodeId> = HashMap::new();
        loop {
            while let Some(pending) = self.queue.pop() {
                let node: Node = self.compile_node(&pending);
                self.nodes[pending.id.index()] = Some(node);
            }
            // Dynamic targets in every resource the compiled nodes sit in; compiling them can
            // reach more resources, so repeat until nothing is added.
            let added: bool =
                self.add_dynamic_targets(&mut dynamic_anchors, &mut recursive_anchors);
            if !added {
                break;
            }
        }
        if let Some(errors) = ErrorList::from_vec(std::mem::take(&mut self.errors)) {
            return Err(errors);
        }
        let nodes: Vec<Node> = self.nodes.into_iter().flatten().collect();
        Ok(CompiledSchema {
            nodes,
            root,
            patterns: self.patterns,
            dynamic_anchors,
            recursive_anchors,
            uses_dynamic_scope: self.uses_dynamic_scope,
        })
    }

    /// Queues the dynamic-anchor and recursive-anchor targets of every resource compiled so far
    /// that are not known yet. Returns whether any was queued.
    fn add_dynamic_targets(
        &mut self,
        dynamic_anchors: &mut HashMap<Scope, Vec<(Box<str>, NodeId)>>,
        recursive_anchors: &mut HashMap<Scope, NodeId>,
    ) -> bool {
        if !self.uses_dynamic_scope {
            return false;
        }
        let scopes: BTreeSet<Scope> = self.nodes.iter().flatten().map(|n| n.scope).collect();
        let names: Vec<&'r str> = self.dynamic_names.iter().copied().collect();
        let mut added: bool = false;
        for scope in scopes {
            for name in &names {
                let known: bool = dynamic_anchors
                    .get(&scope)
                    .is_some_and(|anchors| anchors.iter().any(|(n, _)| **n == **name));
                if known || !self.registry.has_dynamic_anchor(scope, name) {
                    continue;
                }
                if let Ok(resolved) = self.registry.resolve(scope, &format!("#{name}")) {
                    let location: usize = self.target_location(resolved.pointer);
                    let id: NodeId = self.node_for(
                        Source::from_schema_ref(resolved.schema),
                        resolved.scope,
                        location,
                    );
                    dynamic_anchors
                        .entry(scope)
                        .or_default()
                        .push((Box::from(*name), id));
                    added = true;
                }
            }
            if self.uses_recursive_ref
                && !recursive_anchors.contains_key(&scope)
                && let Ok(resolved) = self.registry.resolve(scope, "#")
                && let SchemaRef::Object(root) = resolved.schema
                && root.extensions.get("$recursiveAnchor") == Some(&Value::Bool(true))
            {
                let location: usize = self.target_location(resolved.pointer);
                let id: NodeId = self.node_for(Source::Typed(root), resolved.scope, location);
                recursive_anchors.insert(scope, id);
                added = true;
            }
        }
        added
    }

    /// A location entry for a `$ref` target at `pointer` in its document.
    fn target_location(&mut self, pointer: &'r [String]) -> usize {
        let entry: usize = self.locations.len();
        self.locations.push((usize::MAX, None));
        self.location_bases.insert(entry, pointer);
        entry
    }

    fn child_location(&mut self, parent: usize, step: Step<'r>) -> usize {
        self.locations.push((parent, Some(step)));
        self.locations.len() - 1
    }

    /// The JSON Pointer of a location entry.
    fn location_string(&self, mut entry: usize) -> String {
        let mut steps: Vec<String> = Vec::new();
        let mut base: &[String] = &[];
        while entry != usize::MAX {
            let (parent, step) = self.locations[entry];
            match step {
                Some(Step::Key(key)) => steps.push(key.to_string()),
                Some(Step::Index(index)) => steps.push(index.to_string()),
                None => {
                    if let Some(pointer) = self.location_bases.get(&entry) {
                        base = pointer;
                    }
                }
            }
            entry = parent;
        }
        let mut segments: Vec<String> = base.to_vec();
        segments.extend(steps.into_iter().rev());
        JsonPointer::from(segments).to_string()
    }

    /// The node for `source` in `scope`, queueing it when it is new.
    fn node_for(&mut self, source: Source<'r>, scope: Scope, location: usize) -> NodeId {
        let key: (usize, Scope) = (source.key(), scope);
        if let Some(id) = self.by_source.get(&key) {
            return *id;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a schema with more than u32::MAX nodes cannot be held in memory"
        )]
        let id: NodeId = NodeId(self.nodes.len() as u32);
        self.nodes.push(None);
        self.by_source.insert(key, id);
        self.queue.push(Pending {
            id,
            source,
            scope,
            location,
        });
        id
    }

    fn compile_node(&mut self, pending: &Pending<'r>) -> Node {
        let scope: Scope = match pending.source {
            Source::Bool(_) => pending.scope,
            Source::Typed(s) => self.registry.enter(pending.scope, s),
            Source::Raw(o) => self.registry.enter_raw(pending.scope, o),
        };
        if let Source::Bool(b) = pending.source {
            let keywords: Box<[Keyword]> = if b {
                Box::new([])
            } else {
                Box::new([Keyword::False])
            };
            return Node {
                keywords,
                scope,
                has_unevaluated: false,
            };
        }
        let dialect: Dialect = self.registry.dialect(scope);
        let mut parts: Parts<'r> = self.read_parts(pending.source, dialect, pending.location);
        if parts.ref_.is_some() && dialect.ref_siblings == RefSiblings::Ignored {
            parts = Parts {
                ref_: parts.ref_,
                ..Parts::default()
            };
        }
        let keywords: Vec<Keyword> = self.lower(&parts, dialect, scope, pending.location);
        let has_unevaluated: bool = keywords.iter().any(|k| {
            matches!(
                k,
                Keyword::UnevaluatedProperties(_) | Keyword::UnevaluatedItems(_)
            )
        });
        Node {
            keywords: keywords.into_boxed_slice(),
            scope,
            has_unevaluated,
        }
    }

    fn invalid(&mut self, location: usize, keyword: &str, expected: &'static str) {
        let location: String = self.location_string(location);
        self.errors.push(CompileError::InvalidKeywordValue {
            location,
            keyword: keyword.to_string(),
            expected,
        });
    }

    /// Reads every keyword of `source` that `dialect` defines.
    #[expect(
        clippy::too_many_lines,
        reason = "one statement per keyword; splitting would scatter the keyword list"
    )]
    fn read_parts(&mut self, source: Source<'r>, dialect: Dialect, location: usize) -> Parts<'r> {
        let defined = |name: &str| -> Option<&'static DialectKeyword> { dialect.keyword(name) };
        let mut parts: Parts<'r> = Parts::default();
        let applicators: bool = dialect.vocabulary_active(VocabularyKind::Applicator);
        let assertions: bool = dialect.vocabulary_active(VocabularyKind::Validation);
        let unevaluated: bool = dialect.vocabulary_active(VocabularyKind::Unevaluated)
            || (applicators
                && !dialect
                    .vocabularies()
                    .iter()
                    .any(|v| VocabularyKind::from_uri(v.uri) == Some(VocabularyKind::Unevaluated)));

        // Keywords the typed model holds.
        if let Source::Typed(s) = source {
            parts.ref_ = s.ref_.as_deref();
            if assertions {
                parts.types = s.type_.filter(|_| defined("type").is_some());
                parts.const_value = s
                    .const_value
                    .as_ref()
                    .filter(|_| defined("const").is_some());
                parts.enum_values = s
                    .enum_values
                    .as_deref()
                    .filter(|_| defined("enum").is_some());
                parts.minimum = s.minimum.as_ref();
                parts.maximum = s.maximum.as_ref();
                parts.exclusive_minimum = s.exclusive_minimum.as_ref();
                parts.exclusive_maximum = s.exclusive_maximum.as_ref();
                parts.multiple_of = s.multiple_of.as_ref();
                parts.min_length = s.min_length;
                parts.max_length = s.max_length;
                parts.pattern = s.pattern.as_deref();
                parts.min_items = s.min_items;
                parts.max_items = s.max_items;
                parts.unique_items = s.unique_items == Some(true);
                parts.required = s.required.iter().flatten().map(String::as_str).collect();
            }
            parts.format = s.format.as_deref();
            if applicators {
                parts.properties = s
                    .properties
                    .iter()
                    .map(|(k, v)| (k.as_str(), Source::from_schema(v)))
                    .collect();
                parts.additional_properties =
                    s.additional_properties.as_ref().map(Source::from_schema);
                parts.items = s.items.as_ref().map(Source::from_schema);
                let list = |l: &'r Option<Vec<Schema>>| {
                    l.as_ref()
                        .map(|l| l.iter().map(Source::from_schema).collect::<Vec<_>>())
                };
                parts.all_of = list(&s.all_of);
                parts.any_of = list(&s.any_of);
                parts.one_of = list(&s.one_of);
            }
        }

        // Keywords read raw: every keyword of a raw object, the rest of a typed one.
        let is_raw: bool = matches!(source, Source::Raw(_));
        let get = |name: &str| -> Option<&'r Value> { defined(name).and(source.raw(name)) };
        let mut errors: Vec<(&'static str, &'static str)> = Vec::new();
        let schema_at = |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
            get(name).and_then(|v| {
                let s: Option<Source<'r>> = Source::from_value(v);
                if s.is_none() {
                    errors.push((name, "a schema (an object or a boolean)"));
                }
                s
            })
        };
        let count_at = |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
            get(name).and_then(|v| {
                let n: Option<u64> = count(v);
                if n.is_none() {
                    errors.push((name, "a non-negative integer"));
                }
                n
            })
        };
        let map_at = |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
            get(name).map_or_else(Vec::new, |v| {
                let Value::Object(map) = v else {
                    errors.push((name, "an object whose values are schemas"));
                    return Vec::new();
                };
                map.iter()
                    .filter_map(|(k, v)| {
                        let s: Option<Source<'r>> = Source::from_value(v);
                        if s.is_none() {
                            errors.push((name, "an object whose values are schemas"));
                        }
                        s.map(|s| (k.as_str(), s))
                    })
                    .collect()
            })
        };
        let list_at = |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
            get(name).map(|v| {
                let Value::Array(items) = v else {
                    errors.push((name, "an array of schemas"));
                    return Vec::new();
                };
                items
                    .iter()
                    .filter_map(|item| {
                        let s: Option<Source<'r>> = Source::from_value(item);
                        if s.is_none() {
                            errors.push((name, "an array of schemas"));
                        }
                        s
                    })
                    .collect()
            })
        };

        parts.dynamic_ref = get("$dynamicRef").and_then(Value::as_str);
        parts.recursive_ref = get("$recursiveRef").and_then(Value::as_str);
        if is_raw {
            parts.ref_ = source.raw("$ref").and_then(Value::as_str);
            parts.format = source.raw("format").and_then(Value::as_str);
        }
        if assertions {
            parts.min_properties = count_at("minProperties", &mut errors);
            parts.max_properties = count_at("maxProperties", &mut errors);
            parts.min_contains = count_at("minContains", &mut errors);
            parts.max_contains = count_at("maxContains", &mut errors);
            if let Some(v) = get("dependentRequired") {
                match dependent_required(v) {
                    Some(list) => parts.dependent_required.extend(list),
                    None => errors.push((
                        "dependentRequired",
                        "an object whose values are arrays of strings",
                    )),
                }
            }
            if is_raw {
                match get("type").map(TypeSet::deserialize) {
                    Some(Ok(types)) => parts.types = Some(types),
                    Some(Err(_)) => errors.push(("type", "a type name or an array of type names")),
                    None => {}
                }
                parts.const_value = get("const");
                match get("enum") {
                    Some(Value::Array(values)) => parts.enum_values = Some(values),
                    Some(_) => errors.push(("enum", "an array")),
                    None => {}
                }
                let number =
                    |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
                        get(name).and_then(|v| {
                            let n: Option<&'r Number> = v.as_number();
                            if n.is_none() {
                                errors.push((name, "a number"));
                            }
                            n
                        })
                    };
                parts.minimum = number("minimum", &mut errors);
                parts.maximum = number("maximum", &mut errors);
                parts.exclusive_minimum = number("exclusiveMinimum", &mut errors);
                parts.exclusive_maximum = number("exclusiveMaximum", &mut errors);
                parts.multiple_of = number("multipleOf", &mut errors);
                parts.min_length = count_at("minLength", &mut errors);
                parts.max_length = count_at("maxLength", &mut errors);
                parts.min_items = count_at("minItems", &mut errors);
                parts.max_items = count_at("maxItems", &mut errors);
                parts.pattern = get("pattern").and_then(Value::as_str);
                parts.unique_items = get("uniqueItems") == Some(&Value::Bool(true));
                match get("required") {
                    Some(Value::Array(names)) => {
                        parts.required = names.iter().filter_map(Value::as_str).collect();
                    }
                    Some(_) => errors.push(("required", "an array of strings")),
                    None => {}
                }
            }
        }
        if applicators {
            parts.pattern_properties = map_at("patternProperties", &mut errors);
            parts.property_names = schema_at("propertyNames", &mut errors);
            parts.dependent_schemas = map_at("dependentSchemas", &mut errors);
            if let Some(Value::Object(map)) = get("dependencies") {
                for (name, value) in map {
                    match value {
                        Value::Array(names) if assertions => parts.dependent_required.push((
                            name.as_str(),
                            names.iter().filter_map(Value::as_str).collect(),
                        )),
                        Value::Array(_) => {}
                        other => match Source::from_value(other) {
                            Some(s) => parts.dependent_schemas.push((name.as_str(), s)),
                            None => errors.push((
                                "dependencies",
                                "an object whose values are schemas or arrays of strings",
                            )),
                        },
                    }
                }
            }
            parts.contains = schema_at("contains", &mut errors);
            parts.not = schema_at("not", &mut errors);
            parts.if_ = schema_at("if", &mut errors);
            parts.then = schema_at("then", &mut errors);
            parts.else_ = schema_at("else", &mut errors);
            if is_raw {
                parts.properties = map_at("properties", &mut errors);
                parts.additional_properties = schema_at("additionalProperties", &mut errors);
                parts.all_of = list_at("allOf", &mut errors);
                parts.any_of = list_at("anyOf", &mut errors);
                parts.one_of = list_at("oneOf", &mut errors);
            }
            // `items`: 2020-12 splits the tuple form out as `prefixItems`; earlier drafts take
            // an array (with `additionalItems` for the rest) or one schema.
            let tuple_items: bool = defined("prefixItems").is_none();
            let items_value: Option<&'r Value> = get("items");
            if tuple_items && let Some(Value::Array(_)) = items_value {
                parts.prefix_items = list_at("items", &mut errors).unwrap_or_default();
                parts.items = schema_at("additionalItems", &mut errors);
            } else {
                if is_raw {
                    parts.items = schema_at("items", &mut errors);
                } else if items_value.is_some() {
                    // A 2020-12 `items` array: not a schema.
                    errors.push(("items", "a schema (an object or a boolean)"));
                }
                if !tuple_items {
                    parts.prefix_items = list_at("prefixItems", &mut errors).unwrap_or_default();
                }
            }
        }
        if unevaluated {
            parts.unevaluated_properties = schema_at("unevaluatedProperties", &mut errors);
            parts.unevaluated_items = schema_at("unevaluatedItems", &mut errors);
        }
        for (keyword, expected) in errors {
            self.invalid(location, keyword, expected);
        }
        parts
    }

    fn intern_pattern(&mut self, pattern: &'r str, location: usize) -> Option<RegexId> {
        if let Some(id) = self.pattern_ids.get(pattern) {
            return Some(*id);
        }
        match regress::Regex::with_flags(pattern, "u") {
            Ok(regex) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "fewer than u32::MAX distinct patterns fit in memory"
                )]
                let id: RegexId = RegexId(self.patterns.len() as u32);
                self.patterns.push(Pattern {
                    source: pattern.to_string(),
                    regex,
                });
                self.pattern_ids.insert(pattern, id);
                Some(id)
            }
            Err(error) => {
                let location: String = self.location_string(location);
                self.errors.push(CompileError::InvalidPattern {
                    location,
                    pattern: pattern.to_string(),
                    reason: error.to_string(),
                });
                None
            }
        }
    }

    /// Queues a subschema found under `keyword` (and `step`) of the schema at `location`.
    fn child(&mut self, child: Child<'r>, scope: Scope, location: usize) -> NodeId {
        let (source, keyword, step) = child;
        let mut entry: usize = self.child_location(location, Step::Key(keyword));
        if let Some(step) = step {
            entry = self.child_location(entry, step);
        }
        self.node_for(source, scope, entry)
    }

    /// Resolves a reference written in `scope` to its node.
    fn reference(&mut self, ref_str: &'r str, scope: Scope, location: usize) -> Option<NodeId> {
        let Ok(resolved) = self.registry.resolve(scope, ref_str) else {
            let location: String = self.location_string(location);
            self.errors.push(CompileError::UnknownRef {
                location,
                ref_str: ref_str.to_string(),
            });
            return None;
        };
        let entry: usize = self.target_location(resolved.pointer);
        Some(self.node_for(
            Source::from_schema_ref(resolved.schema),
            resolved.scope,
            entry,
        ))
    }

    /// Lowers `parts` to keywords: assertions first (cheapest first), then the applicators,
    /// then `unevaluated*`, which reads the others' annotations.
    #[expect(
        clippy::too_many_lines,
        reason = "one statement per keyword, in evaluation order"
    )]
    fn lower(
        &mut self,
        parts: &Parts<'r>,
        dialect: Dialect,
        scope: Scope,
        location: usize,
    ) -> Vec<Keyword> {
        let mut out: Vec<Keyword> = Vec::new();
        if let Some(types) = parts.types {
            out.push(Keyword::Type {
                types,
                integer_semantics: dialect.integer_semantics,
            });
        }
        if let Some(value) = parts.const_value {
            out.push(Keyword::Const(value.clone()));
        }
        if let Some(values) = parts.enum_values {
            out.push(Keyword::Enum(values.to_vec().into_boxed_slice()));
        }
        let numbers: [NumericKeyword<'_>; 5] = [
            (parts.minimum, Keyword::Minimum),
            (parts.maximum, Keyword::Maximum),
            (parts.exclusive_minimum, Keyword::ExclusiveMinimum),
            (parts.exclusive_maximum, Keyword::ExclusiveMaximum),
            (parts.multiple_of, Keyword::MultipleOf),
        ];
        for (number, keyword) in numbers {
            if let Some(n) = number {
                out.push(keyword(n.clone()));
            }
        }
        out.extend(parts.min_length.map(Keyword::MinLength));
        out.extend(parts.max_length.map(Keyword::MaxLength));
        if let Some(pattern) = parts.pattern
            && let Some(id) = self.intern_pattern(pattern, location)
        {
            out.push(Keyword::Pattern(id));
        }
        if dialect.format_mode == FormatMode::Assertion
            && let Some(format) = parts.format.and_then(Format::from_name)
        {
            out.push(Keyword::Format(format));
        }
        out.extend(parts.min_items.map(Keyword::MinItems));
        out.extend(parts.max_items.map(Keyword::MaxItems));
        if parts.unique_items {
            out.push(Keyword::UniqueItems);
        }
        if !parts.required.is_empty() {
            out.push(Keyword::Required(
                parts.required.iter().map(|n| Box::from(*n)).collect(),
            ));
        }
        out.extend(parts.min_properties.map(Keyword::MinProperties));
        out.extend(parts.max_properties.map(Keyword::MaxProperties));
        if !parts.dependent_required.is_empty() {
            out.push(Keyword::DependentRequired(
                parts
                    .dependent_required
                    .iter()
                    .map(|(name, required)| {
                        (
                            Box::from(*name),
                            required.iter().map(|n| Box::from(*n)).collect(),
                        )
                    })
                    .collect(),
            ));
        }

        // References.
        if let Some(ref_str) = parts.ref_
            && let Some(target) = self.reference(ref_str, scope, location)
        {
            out.push(Keyword::Ref {
                target,
                ref_str: Box::from(ref_str),
            });
        }
        if let Some(ref_str) = parts.dynamic_ref {
            out.extend(self.dynamic_ref(ref_str, scope, location));
        }
        if let Some(ref_str) = parts.recursive_ref
            && let Some(fallback) = self.reference(ref_str, scope, location)
        {
            let bookended: bool = matches!(
                self.registry.resolve(scope, ref_str).map(|r| r.schema),
                Ok(SchemaRef::Object(target))
                    if target.extensions.get("$recursiveAnchor") == Some(&Value::Bool(true))
            );
            if bookended {
                self.uses_recursive_ref = true;
                self.uses_dynamic_scope = true;
                out.push(Keyword::RecursiveRef {
                    fallback,
                    ref_str: Box::from(ref_str),
                });
            } else {
                out.push(Keyword::Ref {
                    target: fallback,
                    ref_str: Box::from(ref_str),
                });
            }
        }

        // Object applicators.
        let known_names: Box<[Box<str>]> = {
            let mut names: Vec<Box<str>> = parts
                .properties
                .iter()
                .map(|(n, _)| Box::from(*n))
                .collect();
            names.sort();
            names.dedup();
            names.into_boxed_slice()
        };
        let mut known_patterns: Vec<RegexId> = Vec::new();
        let mut pattern_properties: Vec<(RegexId, NodeId)> = Vec::new();
        for (pattern, source) in &parts.pattern_properties {
            let id: Option<RegexId> = self.intern_pattern(pattern, location);
            let node: NodeId = self.child(
                (*source, "patternProperties", Some(Step::Key(pattern))),
                scope,
                location,
            );
            if let Some(id) = id {
                known_patterns.push(id);
                pattern_properties.push((id, node));
            }
        }
        if let Some(Source::Bool(false)) = parts.additional_properties {
            out.push(Keyword::NoAdditionalProperties(KnownProperties {
                names: known_names.clone(),
                patterns: known_patterns.clone().into_boxed_slice(),
            }));
        }
        if !parts.properties.is_empty() {
            let mut properties: Vec<(Box<str>, NodeId)> = Vec::new();
            for (name, source) in &parts.properties {
                let node: NodeId = self.child(
                    (*source, "properties", Some(Step::Key(name))),
                    scope,
                    location,
                );
                properties.push((Box::from(*name), node));
            }
            properties.sort_by(|a, b| a.0.cmp(&b.0));
            properties.dedup_by(|a, b| a.0 == b.0);
            out.push(Keyword::Properties(properties.into_boxed_slice()));
        }
        if !pattern_properties.is_empty() {
            out.push(Keyword::PatternProperties(
                pattern_properties.into_boxed_slice(),
            ));
        }
        match parts.additional_properties {
            None | Some(Source::Bool(false)) => {}
            Some(source) => {
                let node: NodeId =
                    self.child((source, "additionalProperties", None), scope, location);
                out.push(Keyword::AdditionalProperties {
                    node,
                    known: KnownProperties {
                        names: known_names,
                        patterns: known_patterns.into_boxed_slice(),
                    },
                });
            }
        }
        if let Some(source) = parts.property_names {
            let node: NodeId = self.child((source, "propertyNames", None), scope, location);
            out.push(Keyword::PropertyNames(node));
        }
        if !parts.dependent_schemas.is_empty() {
            let mut dependent: Vec<(Box<str>, NodeId)> = Vec::new();
            for (name, source) in &parts.dependent_schemas {
                let keyword: &'static str = if dialect.keyword("dependentSchemas").is_some() {
                    "dependentSchemas"
                } else {
                    "dependencies"
                };
                let node: NodeId =
                    self.child((*source, keyword, Some(Step::Key(name))), scope, location);
                dependent.push((Box::from(*name), node));
            }
            out.push(Keyword::DependentSchemas(dependent.into_boxed_slice()));
        }

        // Array applicators.
        let tuple_items: bool = dialect.keyword("prefixItems").is_none();
        let (prefix_keyword, rest_keyword): (&'static str, &'static str) =
            if tuple_items && !parts.prefix_items.is_empty() {
                ("items", "additionalItems")
            } else {
                ("prefixItems", "items")
            };
        let prefix: Vec<NodeId> = parts
            .prefix_items
            .iter()
            .enumerate()
            .map(|(i, source)| {
                self.child(
                    (*source, prefix_keyword, Some(Step::Index(i))),
                    scope,
                    location,
                )
            })
            .collect();
        let rest: Option<NodeId> = parts
            .items
            .map(|source| self.child((source, rest_keyword, None), scope, location));
        if !prefix.is_empty() || rest.is_some() {
            out.push(Keyword::Items {
                prefix: prefix.into_boxed_slice(),
                rest,
            });
        }
        if let Some(source) = parts.contains {
            let node: NodeId = self.child((source, "contains", None), scope, location);
            out.push(Keyword::Contains {
                node,
                min: parts.min_contains.unwrap_or(1),
                max: parts.max_contains,
                annotates: !tuple_items,
            });
        }

        // In-place applicators.
        let lists: [ListKeyword<'_, 'r>; 3] = [
            ("allOf", &parts.all_of, Keyword::AllOf),
            ("anyOf", &parts.any_of, Keyword::AnyOf),
            ("oneOf", &parts.one_of, Keyword::OneOf),
        ];
        for (keyword, list, make) in lists {
            if let Some(list) = list {
                let nodes: Vec<NodeId> = list
                    .iter()
                    .enumerate()
                    .map(|(i, source)| {
                        self.child((*source, keyword, Some(Step::Index(i))), scope, location)
                    })
                    .collect();
                out.push(make(nodes.into_boxed_slice()));
            }
        }
        if let Some(source) = parts.not {
            let node: NodeId = self.child((source, "not", None), scope, location);
            out.push(Keyword::Not(node));
        }
        if let Some(source) = parts.if_ {
            let condition: NodeId = self.child((source, "if", None), scope, location);
            let then: Option<NodeId> = parts
                .then
                .map(|s| self.child((s, "then", None), scope, location));
            let otherwise: Option<NodeId> = parts
                .else_
                .map(|s| self.child((s, "else", None), scope, location));
            out.push(Keyword::IfThenElse {
                condition,
                then,
                otherwise,
            });
        }

        // Unevaluated, last: it reads the annotations of everything above.
        if let Some(source) = parts.unevaluated_properties {
            let node: NodeId = self.child((source, "unevaluatedProperties", None), scope, location);
            out.push(Keyword::UnevaluatedProperties(node));
        }
        if let Some(source) = parts.unevaluated_items {
            let node: NodeId = self.child((source, "unevaluatedItems", None), scope, location);
            out.push(Keyword::UnevaluatedItems(node));
        }
        out
    }

    /// Compiles a `$dynamicRef`: dynamic when its initial target declares the `$dynamicAnchor`
    /// its fragment names, otherwise an ordinary `$ref`.
    fn dynamic_ref(&mut self, ref_str: &'r str, scope: Scope, location: usize) -> Option<Keyword> {
        let fallback: NodeId = self.reference(ref_str, scope, location)?;
        let anchor: Option<&'r str> = ref_str
            .rsplit_once('#')
            .map(|(_, fragment)| fragment)
            .filter(|f| !f.is_empty() && !f.starts_with('/'));
        let bookended: bool = anchor.is_some_and(|anchor| {
            matches!(
                self.registry.resolve(scope, ref_str).map(|r| r.schema),
                Ok(SchemaRef::Object(target))
                    if target.extensions.get("$dynamicAnchor").and_then(Value::as_str)
                        == Some(anchor)
            )
        });
        match anchor {
            Some(anchor) if bookended => {
                self.dynamic_names.insert(anchor);
                self.uses_dynamic_scope = true;
                Some(Keyword::DynamicRef {
                    fallback,
                    anchor: Box::from(anchor),
                    ref_str: Box::from(ref_str),
                })
            }
            _ => Some(Keyword::Ref {
                target: fallback,
                ref_str: Box::from(ref_str),
            }),
        }
    }
}

/// Reads `dependentRequired`: an object whose values are arrays of strings.
fn dependent_required(value: &Value) -> Option<Vec<(&str, Vec<&str>)>> {
    let Value::Object(map) = value else {
        return None;
    };
    map.iter()
        .map(|(name, required)| match required {
            Value::Array(names) => names
                .iter()
                .map(Value::as_str)
                .collect::<Option<Vec<&str>>>()
                .map(|names| (name.as_str(), names)),
            _ => None,
        })
        .collect()
}
