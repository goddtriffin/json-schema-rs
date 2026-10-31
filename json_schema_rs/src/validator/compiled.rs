//! The compiled form of a schema: an arena of [`Node`]s linked by [`NodeId`], built once from a
//! [`Registry`] so validation never re-reads the schema model.
//!
//! Each node holds the keywords of one schema object as a closed enum ([`Keyword`]), ordered so
//! the cheap assertions run before the applicators and `unevaluated*` runs last. Every `$ref` is
//! resolved to a [`NodeId`] at compile time; a schema reached twice (two `$ref`s to one
//! definition, a recursive schema) is compiled once. Regular expressions are compiled once per
//! distinct pattern.

use super::ecma_regex;
use super::format::Format;
use super::simple_pattern::SimplePattern;
use crate::error_list::ErrorList;
use crate::json_pointer::JsonPointer;
use crate::json_schema::{
    Dialect, FormatMode, IntegerSemantics, JsonSchema, Keyword as DialectKeyword, RefSiblings,
    Registry, Schema, SchemaRef, Scope, TypeSet, VocabularyKind,
};
use crate::number;
use serde::Deserialize;
use serde_json::{Map, Number, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

/// A problem that keeps a schema from compiling, with its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// A `pattern` or `patternProperties` key that is not a valid ECMA-262 regular expression.
    InvalidPattern {
        /// JSON Pointer to the pattern within its document: the `pattern` keyword
        /// (`/properties/a/pattern`), or the `patternProperties` key (`/patternProperties/(`).
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
    /// A `format` this library does not know, under the `format-assertion` vocabulary, which
    /// requires failing on unknown formats.
    UnknownFormat {
        /// JSON Pointer to the schema object holding the format, within its document.
        location: String,
        /// The format name.
        format: String,
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
            CompileError::UnknownFormat { location, format } => write!(
                f,
                "{}: unknown format \"{format}\" under the format-assertion vocabulary",
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

/// The index of an interned `enum` list in a [`CompiledSchema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EnumId(u32);

/// The index of a `properties` list in a [`CompiledSchema`], shared by the node's
/// `additionalProperties`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PropertiesId(u32);

/// The index of a user-defined format in a [`CompiledSchema`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UserFormatId(u32);

/// A keyword's name, for the keyword locations of errors (one byte, so the evaluator's location
/// frames stay small).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kw {
    Type,
    Const,
    Enum,
    Minimum,
    Maximum,
    ExclusiveMinimum,
    ExclusiveMaximum,
    MultipleOf,
    MinLength,
    MaxLength,
    Pattern,
    Format,
    MinItems,
    MaxItems,
    UniqueItems,
    Required,
    MinProperties,
    MaxProperties,
    DependentRequired,
    Dependencies,
    DependentSchemas,
    Properties,
    PatternProperties,
    AdditionalProperties,
    PropertyNames,
    UnevaluatedProperties,
    Items,
    PrefixItems,
    AdditionalItems,
    Contains,
    MinContains,
    MaxContains,
    UnevaluatedItems,
    AllOf,
    AnyOf,
    OneOf,
    Not,
    If,
    Then,
    Else,
    Ref,
    DynamicRef,
    RecursiveRef,
}

impl Kw {
    /// The keyword as written in a schema.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Kw::Type => "type",
            Kw::Const => "const",
            Kw::Enum => "enum",
            Kw::Minimum => "minimum",
            Kw::Maximum => "maximum",
            Kw::ExclusiveMinimum => "exclusiveMinimum",
            Kw::ExclusiveMaximum => "exclusiveMaximum",
            Kw::MultipleOf => "multipleOf",
            Kw::MinLength => "minLength",
            Kw::MaxLength => "maxLength",
            Kw::Pattern => "pattern",
            Kw::Format => "format",
            Kw::MinItems => "minItems",
            Kw::MaxItems => "maxItems",
            Kw::UniqueItems => "uniqueItems",
            Kw::Required => "required",
            Kw::MinProperties => "minProperties",
            Kw::MaxProperties => "maxProperties",
            Kw::DependentRequired => "dependentRequired",
            Kw::Dependencies => "dependencies",
            Kw::DependentSchemas => "dependentSchemas",
            Kw::Properties => "properties",
            Kw::PatternProperties => "patternProperties",
            Kw::AdditionalProperties => "additionalProperties",
            Kw::PropertyNames => "propertyNames",
            Kw::UnevaluatedProperties => "unevaluatedProperties",
            Kw::Items => "items",
            Kw::PrefixItems => "prefixItems",
            Kw::AdditionalItems => "additionalItems",
            Kw::Contains => "contains",
            Kw::MinContains => "minContains",
            Kw::MaxContains => "maxContains",
            Kw::UnevaluatedItems => "unevaluatedItems",
            Kw::AllOf => "allOf",
            Kw::AnyOf => "anyOf",
            Kw::OneOf => "oneOf",
            Kw::Not => "not",
            Kw::If => "if",
            Kw::Then => "then",
            Kw::Else => "else",
            Kw::Ref => "$ref",
            Kw::DynamicRef => "$dynamicRef",
            Kw::RecursiveRef => "$recursiveRef",
        }
    }
}

/// A user-defined format check: answers whether a string is a member of the format.
pub type FormatCheck = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// A format the user registered with [`CompiledSchemaBuilder::format`].
#[derive(Clone)]
pub(crate) struct UserFormat {
    pub(crate) name: Box<str>,
    pub(crate) check: FormatCheck,
}

impl fmt::Debug for UserFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserFormat")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// A compiled regular expression and its source.
#[derive(Debug)]
pub(crate) struct Pattern {
    pub(crate) source: String,
    pub(crate) regex: regress::Regex,
    /// The greedy matcher, when the pattern is in the subset it decides exactly.
    pub(crate) simple: Option<SimplePattern>,
}

impl Pattern {
    /// Whether the pattern matches somewhere in `text` (ECMA-262 search semantics).
    pub(crate) fn is_match(&self, text: &str) -> bool {
        match self
            .simple
            .as_ref()
            .and_then(|simple| simple.is_match(text))
        {
            Some(matched) => matched,
            None => self.regex.find(text).is_some(),
        }
    }
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
    /// The node's `properties` list (sorted by name), when it has one.
    pub(crate) names: Option<PropertiesId>,
    pub(crate) patterns: Box<[RegexId]>,
}

/// A `properties` list: each name and its schema, sorted by name.
pub(crate) type PropertyList = Box<[(Box<str>, NodeId)]>;

/// A `dependentRequired` entry: a property and the names it requires when present.
pub(crate) type Dependency = (Box<str>, Box<[Box<str>]>);

/// The names `required` lists.
#[derive(Debug)]
pub(crate) struct Required {
    /// In schema order, duplicates kept: one error per name, in that order.
    pub(crate) names: Box<[Box<str>]>,
    /// Sorted and deduplicated, to check against an object's sorted keys in one pass.
    pub(crate) sorted: Box<[Box<str>]>,
}

/// A `$dynamicRef` whose initial target declares the matching `$dynamicAnchor`.
#[derive(Debug)]
pub(crate) struct DynamicRef {
    pub(crate) fallback: NodeId,
    pub(crate) anchor: Box<str>,
    pub(crate) ref_str: Box<str>,
}

/// Item schemas by position, then one for every later item.
#[derive(Debug)]
pub(crate) struct PrefixItems {
    pub(crate) prefix: Box<[NodeId]>,
    pub(crate) rest: Option<NodeId>,
    /// How the prefix and the rest are spelled: `prefixItems` / `items` (2020-12), or `items` /
    /// `additionalItems` (earlier drafts).
    pub(crate) prefix_keyword: Kw,
    pub(crate) rest_keyword: Kw,
}

/// `contains` with its bounds.
#[derive(Debug)]
pub(crate) struct Contains {
    pub(crate) node: NodeId,
    pub(crate) min: u64,
    pub(crate) max: Option<u64>,
    /// Whether matched items count as evaluated for `unevaluatedItems` (2020-12).
    pub(crate) annotates: bool,
    /// The keyword too few matches fail: `minContains` when the schema has it, else `contains`.
    pub(crate) min_keyword: Kw,
}

/// One keyword, compiled. Rare or large payloads are boxed (or kept in a side table of the
/// [`CompiledSchema`]), so the common keywords stay small.
#[derive(Debug)]
pub(crate) enum Keyword {
    /// The boolean schema `false`.
    False,
    Type {
        types: TypeSet,
        integer_semantics: IntegerSemantics,
    },
    Const(Box<Value>),
    Enum(EnumId),
    Minimum(Number),
    Maximum(Number),
    ExclusiveMinimum(Number),
    ExclusiveMaximum(Number),
    MultipleOf(Number),
    MinLength(u64),
    MaxLength(u64),
    Pattern(RegexId),
    Format(Format),
    /// A format the user registered.
    UserFormat(UserFormatId),
    MinItems(u64),
    MaxItems(u64),
    UniqueItems,
    Required(Box<Required>),
    MinProperties(u64),
    MaxProperties(u64),
    /// `dependentRequired` and the array form of `dependencies` (`keyword` says which):
    /// (property, names it requires).
    DependentRequired {
        dependencies: Box<[Dependency]>,
        keyword: Kw,
    },
    /// `additionalProperties: false`: every member outside `known` is an error of its own.
    NoAdditionalProperties(Box<KnownProperties>),
    /// `$ref` (or a `$dynamicRef` / `$recursiveRef` that resolves statically, `keyword` says
    /// which): evaluated in place, with the in-place loop guard.
    Ref {
        target: NodeId,
        keyword: Kw,
        ref_str: Box<str>,
    },
    /// `$dynamicRef` whose initial target declares the matching `$dynamicAnchor`: the outermost
    /// resource in the dynamic scope that declares `anchor` wins, else `fallback`.
    DynamicRef(Box<DynamicRef>),
    /// `$recursiveRef` whose initial target has `$recursiveAnchor: true`: the outermost resource
    /// in the dynamic scope whose root has it wins, else `fallback`.
    RecursiveRef {
        fallback: NodeId,
        ref_str: Box<str>,
    },
    /// The `properties` list, sorted by name.
    Properties(PropertiesId),
    PatternProperties(Box<[(RegexId, NodeId)]>),
    AdditionalProperties {
        node: NodeId,
        known: Box<KnownProperties>,
    },
    PropertyNames(NodeId),
    /// `dependentSchemas` and the schema form of `dependencies` (`keyword` says which).
    DependentSchemas {
        dependencies: Box<[(Box<str>, NodeId)]>,
        keyword: Kw,
    },
    /// One schema for every item: single-schema `items` (every draft), 2020-12 `items`
    /// without `prefixItems`, or `additionalItems` after an empty tuple (`keyword` says which).
    Items {
        node: NodeId,
        keyword: Kw,
    },
    /// `prefixItems` + `items` (2020-12), or array-form `items` + `additionalItems` (earlier
    /// drafts), normalised to one tuple.
    PrefixItems(Box<PrefixItems>),
    Contains(Box<Contains>),
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
    pub(crate) nodes: Box<[Node]>,
    pub(crate) root: NodeId,
    pub(crate) patterns: Vec<Pattern>,
    /// Distinct `enum` lists.
    pub(crate) enums: Box<[Box<[Value]>]>,
    /// `properties` lists, each sorted by name.
    pub(crate) properties: Box<[PropertyList]>,
    /// `$dynamicAnchor` targets: resource → (anchor name, node) for every anchor name a dynamic
    /// `$dynamicRef` uses.
    pub(crate) dynamic_anchors: HashMap<Scope, Vec<(Box<str>, NodeId)>>,
    /// Resource roots with `$recursiveAnchor: true`: resource → node.
    pub(crate) recursive_anchors: HashMap<Scope, NodeId>,
    /// Whether any dynamic reference needs the dynamic scope tracked while evaluating.
    pub(crate) uses_dynamic_scope: bool,
    /// Whether an object's members iterate in key order (see [`maps_are_sorted`]), so the
    /// property keywords can walk them together with their sorted name lists.
    pub(crate) sorted_maps: bool,
    /// The canonical URI of every reference target that has one (see
    /// [`Registry::canonical_uri`]): where the absolute keyword locations of errors start. Cold:
    /// read only when an error is built.
    pub(crate) target_uris: IntMap<NodeId, Box<str>>,
    /// The user-defined formats, by [`UserFormatId`].
    pub(crate) user_formats: Box<[UserFormat]>,
}

/// Whether [`serde_json::Map`] iterates its keys in sorted (byte) order: it does unless some
/// crate in the build enables `serde_json`'s `preserve_order` feature, which keeps insertion
/// order.
pub(crate) fn maps_are_sorted() -> bool {
    let mut map: Map<String, Value> = Map::new();
    map.insert("b".to_string(), Value::Null);
    map.insert("a".to_string(), Value::Null);
    map.keys().next().map(String::as_str) == Some("a")
}

/// Options for compiling a schema: [`CompiledSchema::builder`], then
/// [`CompiledSchemaBuilder::build`].
///
/// # Example
///
/// ```
/// use json_schema_rs::{CompiledSchema, FormatMode, JsonSchema, JsonSchemaSettings, Registry};
/// use serde_json::json;
///
/// // Formats are asserted only when the schema is parsed with assertion on (2020-12 records
/// // them as annotations by default).
/// let settings: JsonSchemaSettings = JsonSchemaSettings::builder()
///     .format_mode(FormatMode::Assertion)
///     .build();
/// let schema: JsonSchema = JsonSchema::new_from_str(r#"{"format":"even-length"}"#, &settings)
///     .unwrap();
/// let registry: Registry<'_> = Registry::new(&schema).unwrap();
/// let compiled: CompiledSchema = CompiledSchema::builder()
///     .format("even-length", |s: &str| s.chars().count().is_multiple_of(2))
///     .build(&registry)
///     .unwrap();
/// assert!(compiled.is_valid(&json!("ab")));
/// assert!(!compiled.is_valid(&json!("abc")));
/// ```
#[derive(Debug, Clone, Default)]
pub struct CompiledSchemaBuilder {
    formats: Vec<UserFormat>,
}

impl CompiledSchemaBuilder {
    /// Registers the format `name`, checked by `check` (which answers whether a string is a
    /// member of the format). Like the built-in formats it applies to strings only, and only
    /// where formats are asserted (`FormatMode::Assertion`, the default in drafts 4 to 7, or
    /// the `format-assertion` vocabulary); it is known in every draft, so the
    /// `format-assertion` vocabulary does not reject it. A user format replaces a built-in one
    /// of the same name, and registering a name twice keeps the last check.
    #[must_use]
    pub fn format(
        mut self,
        name: impl Into<String>,
        check: impl Fn(&str) -> bool + Send + Sync + 'static,
    ) -> Self {
        let name: Box<str> = name.into().into_boxed_str();
        self.formats.retain(|format| format.name != name);
        self.formats.push(UserFormat {
            name,
            check: Arc::new(check),
        });
        self
    }

    /// Compiles the root schema of `registry` (and everything it references) with these
    /// options.
    ///
    /// # Errors
    ///
    /// Returns every [`CompileError`] found (see [`CompiledSchema::new`]).
    pub fn build(&self, registry: &Registry<'_>) -> Result<CompiledSchema, CompileErrors> {
        let mut compiler: Compiler<'_, '_> = Compiler::new(registry);
        compiler.user_formats.clone_from(&self.formats);
        compiler.compile()
    }
}

impl CompiledSchema {
    /// Compiles the root schema of `registry` (and everything it references) with the default
    /// options.
    ///
    /// # Errors
    ///
    /// Returns every [`CompileError`] found: an invalid regular expression, a keyword the model
    /// keeps raw with a value of the wrong shape, or a `$ref` the registry does not know.
    pub fn new(registry: &Registry<'_>) -> Result<CompiledSchema, CompileErrors> {
        Compiler::new(registry).compile()
    }

    /// Options for compiling a schema (user-defined formats).
    #[must_use]
    pub fn builder() -> CompiledSchemaBuilder {
        CompiledSchemaBuilder::default()
    }

    pub(crate) fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub(crate) fn pattern(&self, id: RegexId) -> &Pattern {
        &self.patterns[id.0 as usize]
    }

    pub(crate) fn enum_values(&self, id: EnumId) -> &[Value] {
        &self.enums[id.0 as usize]
    }

    pub(crate) fn property_list(&self, id: PropertiesId) -> &[(Box<str>, NodeId)] {
        &self.properties[id.0 as usize]
    }

    pub(crate) fn user_format(&self, id: UserFormatId) -> &UserFormat {
        &self.user_formats[id.0 as usize]
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

/// A multiplicative hasher for the compiler's maps keyed by integers (schema addresses, scopes,
/// location entries, precomputed hashes). Their keys come from the process itself, never from
/// input an attacker chooses, so the flooding resistance of the default `SipHash` buys nothing and
/// its cost shows on large schemas.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct IntHasher(u64);

impl Hasher for IntHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.write_u64(u64::from(*byte));
        }
    }

    fn write_u64(&mut self, n: u64) {
        // FxHash's step: mix the word in, then multiply by a large odd constant.
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
}

/// A hash map keyed by integers, hashed with [`IntHasher`].
pub(crate) type IntMap<K, V> = HashMap<K, V, BuildHasherDefault<IntHasher>>;

/// A hash set of integers, hashed with [`IntHasher`].
type IntSet<K> = HashSet<K, BuildHasherDefault<IntHasher>>;

/// What compiling a node needs to know about its dialect, worked out once per dialect.
#[derive(Debug, Clone, Copy)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent yes/no facts, each read on its own"
)]
struct DialectFacts {
    applicators: bool,
    assertions: bool,
    unevaluated: bool,
    /// Whether `items` takes the tuple form (no `prefixItems`, drafts before 2020-12).
    tuple_items: bool,
}

impl DialectFacts {
    fn of(dialect: Dialect) -> DialectFacts {
        let applicators: bool = dialect.vocabulary_active(VocabularyKind::Applicator);
        // Drafts without vocabularies keep `unevaluated*` with the applicators.
        let unevaluated: bool = dialect.vocabulary_active(VocabularyKind::Unevaluated)
            || (applicators
                && !dialect
                    .vocabularies()
                    .iter()
                    .any(|v| VocabularyKind::from_uri(v.uri) == Some(VocabularyKind::Unevaluated)));
        DialectFacts {
            applicators,
            assertions: dialect.vocabulary_active(VocabularyKind::Validation),
            unevaluated,
            tuple_items: dialect.keyword("prefixItems").is_none(),
        }
    }
}

/// Build-time state.
struct Compiler<'r, 'a> {
    registry: &'r Registry<'a>,
    nodes: Vec<Option<Node>>,
    /// Nodes that may be reached more than once (boolean schemas and `$ref` targets), by
    /// source address and scope.
    by_source: IntMap<(usize, Scope), NodeId>,
    /// Every `$ref` target of the registry, by address and scope: the subschemas that can be
    /// reached other than from their parent.
    ref_targets: IntSet<(usize, Scope)>,
    queue: Vec<Pending<'r>>,
    /// Location arena: (parent entry, step); entry 0 is a document root.
    locations: Vec<(usize, Option<Step<'r>>)>,
    /// The pointer segments of `$ref` targets, by location entry, so their locations start at
    /// the target rather than at the referrer.
    location_bases: IntMap<usize, &'r [String]>,
    patterns: Vec<Pattern>,
    pattern_ids: HashMap<&'r str, RegexId>,
    enums: Vec<Box<[Value]>>,
    /// Interned `enum` lists by the hash of their values.
    enum_ids: IntMap<u64, Vec<EnumId>>,
    properties: Vec<PropertyList>,
    /// [`DialectFacts`] of each dialect met so far.
    facts: Vec<(Dialect, DialectFacts)>,
    /// See [`maps_are_sorted`].
    sorted_maps: bool,
    dynamic_names: BTreeSet<&'r str>,
    uses_recursive_ref: bool,
    uses_dynamic_scope: bool,
    /// See [`CompiledSchema::target_uris`].
    target_uris: IntMap<NodeId, Box<str>>,
    /// The user-defined formats (see [`CompiledSchemaBuilder::format`]).
    user_formats: Vec<UserFormat>,
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
    dependent_required: DependencyNames<'r>,
    /// The array form of `dependencies` (kept apart: its errors name `dependencies`).
    dependencies_required: DependencyNames<'r>,
    properties: Vec<(&'r str, Source<'r>)>,
    pattern_properties: Vec<(&'r str, Source<'r>)>,
    additional_properties: Option<Source<'r>>,
    property_names: Option<Source<'r>>,
    dependent_schemas: Vec<(&'r str, Source<'r>)>,
    /// The schema form of `dependencies` (kept apart: its locations name `dependencies`).
    dependencies_schemas: Vec<(&'r str, Source<'r>)>,
    prefix_items: Vec<Source<'r>>,
    /// Whether `items` took the tuple form (an array, drafts before 2020-12), so the rest is
    /// `additionalItems`.
    tuple_form: bool,
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

/// `dependentRequired` as read: each property and the names it requires.
type DependencyNames<'r> = Vec<(&'r str, Vec<&'r str>)>;

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
        let ref_targets: IntSet<(usize, Scope)> = registry
            .ref_targets()
            .map(|target| (Source::from_schema_ref(target.schema).key(), target.scope))
            .collect();
        Compiler {
            registry,
            nodes: Vec::new(),
            by_source: IntMap::default(),
            ref_targets,
            queue: Vec::new(),
            locations: vec![(usize::MAX, None)],
            location_bases: IntMap::default(),
            patterns: Vec::new(),
            pattern_ids: HashMap::new(),
            enums: Vec::new(),
            enum_ids: IntMap::default(),
            properties: Vec::new(),
            facts: Vec::new(),
            sorted_maps: maps_are_sorted(),
            dynamic_names: BTreeSet::new(),
            uses_recursive_ref: false,
            uses_dynamic_scope: false,
            target_uris: IntMap::default(),
            user_formats: Vec::new(),
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
        let nodes: Box<[Node]> = self.nodes.into_iter().flatten().collect();
        Ok(CompiledSchema {
            nodes,
            root,
            patterns: self.patterns,
            enums: self.enums.into_boxed_slice(),
            properties: self.properties.into_boxed_slice(),
            dynamic_anchors,
            recursive_anchors,
            uses_dynamic_scope: self.uses_dynamic_scope,
            sorted_maps: self.sorted_maps,
            target_uris: self.target_uris,
            user_formats: self.user_formats.into_boxed_slice(),
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
                    let source: Source<'r> = Source::from_schema_ref(resolved.schema);
                    let id: NodeId = self.node_for(source, resolved.scope, location);
                    self.record_target_uri(id, source, resolved.scope, resolved.pointer);
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
                self.record_target_uri(id, Source::Typed(root), resolved.scope, resolved.pointer);
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
        let id: NodeId = self.new_node(source, scope, location);
        self.by_source.insert(key, id);
        id
    }

    /// Queues `source` in `scope` as a new node.
    fn new_node(&mut self, source: Source<'r>, scope: Scope, location: usize) -> NodeId {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a schema with more than u32::MAX nodes cannot be held in memory"
        )]
        let id: NodeId = NodeId(self.nodes.len() as u32);
        self.nodes.push(None);
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

    fn facts(&mut self, dialect: Dialect) -> DialectFacts {
        if let Some((_, facts)) = self.facts.iter().find(|(known, _)| *known == dialect) {
            return *facts;
        }
        let facts: DialectFacts = DialectFacts::of(dialect);
        self.facts.push((dialect, facts));
        facts
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
        let DialectFacts {
            applicators,
            assertions,
            unevaluated,
            tuple_items,
            ..
        } = self.facts(dialect);

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
        // The schema's own keys first: on a typed schema they are usually absent, which skips
        // the keyword-table lookup.
        let get = |name: &str| -> Option<&'r Value> {
            source.raw(name).filter(|_| defined(name).is_some())
        };
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

        let string_at = |name: &'static str, errors: &mut Vec<(&'static str, &'static str)>| {
            get(name).and_then(|v| {
                let s: Option<&'r str> = v.as_str();
                if s.is_none() {
                    errors.push((name, "a string"));
                }
                s
            })
        };

        parts.dynamic_ref = string_at("$dynamicRef", &mut errors);
        parts.recursive_ref = string_at("$recursiveRef", &mut errors);
        if is_raw {
            parts.ref_ = string_at("$ref", &mut errors);
            parts.format = string_at("format", &mut errors);
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
                parts.pattern = string_at("pattern", &mut errors);
                match get("uniqueItems") {
                    Some(Value::Bool(unique)) => parts.unique_items = *unique,
                    Some(_) => errors.push(("uniqueItems", "a boolean")),
                    None => {}
                }
                if let Some(v) = get("required") {
                    match strings(v) {
                        Some(names) => parts.required = names,
                        None => errors.push(("required", "an array of strings")),
                    }
                }
            }
        }
        if applicators {
            parts.pattern_properties = map_at("patternProperties", &mut errors);
            parts.property_names = schema_at("propertyNames", &mut errors);
            parts.dependent_schemas = map_at("dependentSchemas", &mut errors);
            let dependencies_shape: &'static str =
                "an object whose values are schemas or arrays of strings";
            match get("dependencies") {
                Some(Value::Object(map)) => {
                    for (name, value) in map {
                        if let Some(s) = Source::from_value(value) {
                            parts.dependencies_schemas.push((name.as_str(), s));
                        } else if let Some(names) = strings(value) {
                            if assertions {
                                parts.dependencies_required.push((name.as_str(), names));
                            }
                        } else {
                            errors.push(("dependencies", dependencies_shape));
                        }
                    }
                }
                Some(_) => errors.push(("dependencies", dependencies_shape)),
                None => {}
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
            let items_value: Option<&'r Value> = get("items");
            if tuple_items && let Some(Value::Array(_)) = items_value {
                parts.tuple_form = true;
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

    /// Compiles `pattern`, written under `keyword` of the schema at `location` (`pattern`, or
    /// `patternProperties` with the pattern as the key), interning it.
    fn intern_pattern(
        &mut self,
        pattern: &'r str,
        keyword: &'static str,
        location: usize,
    ) -> Option<RegexId> {
        if let Some(id) = self.pattern_ids.get(pattern) {
            return Some(*id);
        }
        match ecma_regex::compile(pattern) {
            Ok(regex) => {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "fewer than u32::MAX distinct patterns fit in memory"
                )]
                let id: RegexId = RegexId(self.patterns.len() as u32);
                self.patterns.push(Pattern {
                    source: pattern.to_string(),
                    regex,
                    simple: SimplePattern::parse(pattern),
                });
                self.pattern_ids.insert(pattern, id);
                Some(id)
            }
            Err(error) => {
                let mut entry: usize = self.child_location(location, Step::Key(keyword));
                if keyword == "patternProperties" {
                    entry = self.child_location(entry, Step::Key(pattern));
                }
                let location: String = self.location_string(entry);
                self.errors.push(CompileError::InvalidPattern {
                    location,
                    pattern: pattern.to_string(),
                    reason: error.to_string(),
                });
                None
            }
        }
    }

    /// The id of the `enum` list `values`, interning it: schemas often repeat one list (a
    /// shared vocabulary of names in every definition), and it is kept once. Lists are shared
    /// only when equal value for value as written (`1` and `1.0` are not), so error messages
    /// show each list exactly as its schema wrote it.
    fn intern_enum(&mut self, values: &[Value]) -> EnumId {
        let mut hasher: DefaultHasher = DefaultHasher::new();
        hasher.write_usize(values.len());
        for value in values {
            number::json_hash(value, &mut hasher);
        }
        let hash: u64 = hasher.finish();
        let candidates: &mut Vec<EnumId> = self.enum_ids.entry(hash).or_default();
        // Equal objects can be written in different key orders, which unsorted maps keep.
        let written = |list: &[Value]| serde_json::to_string(list).ok();
        if let Some(id) = candidates.iter().find(|id| {
            let interned: &[Value] = &self.enums[id.0 as usize];
            *interned == *values && (self.sorted_maps || written(interned) == written(values))
        }) {
            return *id;
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "fewer than u32::MAX distinct lists fit in memory"
        )]
        let id: EnumId = EnumId(self.enums.len() as u32);
        candidates.push(id);
        self.enums.push(values.into());
        id
    }

    /// Queues a subschema found under `keyword` (and `step`) of the schema at `location`.
    fn child(&mut self, child: Child<'r>, scope: Scope, location: usize) -> NodeId {
        let (source, keyword, step) = child;
        let mut entry: usize = self.child_location(location, Step::Key(keyword));
        if let Some(step) = step {
            entry = self.child_location(entry, step);
        }
        // A subschema no reference targets is reached only from its parent, which is compiled
        // once: it needs no entry in `by_source`.
        let key: (usize, Scope) = (source.key(), scope);
        if matches!(source, Source::Bool(_)) || self.ref_targets.contains(&key) {
            self.node_for(source, scope, entry)
        } else {
            self.new_node(source, scope, entry)
        }
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
        let source: Source<'r> = Source::from_schema_ref(resolved.schema);
        let id: NodeId = self.node_for(source, resolved.scope, entry);
        self.record_target_uri(id, source, resolved.scope, resolved.pointer);
        Some(id)
    }

    /// Records the canonical URI of the reference target `id` (see
    /// [`CompiledSchema::target_uris`]), once per target.
    fn record_target_uri(
        &mut self,
        id: NodeId,
        source: Source<'r>,
        scope: Scope,
        pointer: &'r [String],
    ) {
        if self.target_uris.contains_key(&id) {
            return;
        }
        // The target's own `$id`, if it has one, makes it the root of its resource.
        let scope: Scope = match source {
            Source::Bool(_) => scope,
            Source::Typed(s) => self.registry.enter(scope, s),
            Source::Raw(o) => self.registry.enter_raw(scope, o),
        };
        if let Some(uri) = self.registry.canonical_uri(scope, pointer) {
            self.target_uris.insert(id, uri.into_boxed_str());
        }
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
            out.push(Keyword::Const(Box::new(value.clone())));
        }
        if let Some(values) = parts.enum_values {
            out.push(Keyword::Enum(self.intern_enum(values)));
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
            && let Some(id) = self.intern_pattern(pattern, "pattern", location)
        {
            out.push(Keyword::Pattern(id));
        }
        if let Some(name) = parts.format {
            self.lower_format(name, dialect, location, &mut out);
        }
        out.extend(parts.min_items.map(Keyword::MinItems));
        out.extend(parts.max_items.map(Keyword::MaxItems));
        if parts.unique_items {
            out.push(Keyword::UniqueItems);
        }
        if !parts.required.is_empty() {
            let mut sorted: Vec<&str> = parts.required.clone();
            sorted.sort_unstable();
            sorted.dedup();
            out.push(Keyword::Required(Box::new(Required {
                names: parts.required.iter().map(|n| Box::from(*n)).collect(),
                sorted: sorted.into_iter().map(Box::from).collect(),
            })));
        }
        out.extend(parts.min_properties.map(Keyword::MinProperties));
        out.extend(parts.max_properties.map(Keyword::MaxProperties));
        let facts: DialectFacts = self.facts(dialect);
        let required_lists: [(&DependencyNames<'r>, Kw); 2] = [
            (&parts.dependent_required, Kw::DependentRequired),
            (&parts.dependencies_required, Kw::Dependencies),
        ];
        for (list, keyword) in required_lists {
            if !list.is_empty() {
                out.push(Keyword::DependentRequired {
                    dependencies: list
                        .iter()
                        .map(|(name, required)| {
                            (
                                Box::from(*name),
                                required.iter().map(|n| Box::from(*n)).collect(),
                            )
                        })
                        .collect(),
                    keyword,
                });
            }
        }

        // References.
        if let Some(ref_str) = parts.ref_
            && let Some(target) = self.reference(ref_str, scope, location)
        {
            out.push(Keyword::Ref {
                target,
                keyword: Kw::Ref,
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
                    keyword: Kw::RecursiveRef,
                    ref_str: Box::from(ref_str),
                });
            }
        }

        // Object applicators.
        let mut known_patterns: Vec<RegexId> = Vec::new();
        let mut pattern_properties: Vec<(RegexId, NodeId)> = Vec::new();
        for (pattern, source) in &parts.pattern_properties {
            let id: Option<RegexId> = self.intern_pattern(pattern, "patternProperties", location);
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
        let mut properties: Vec<(Box<str>, NodeId)> = Vec::new();
        for (name, source) in &parts.properties {
            let node: NodeId = self.child(
                (*source, "properties", Some(Step::Key(name))),
                scope,
                location,
            );
            properties.push((Box::from(*name), node));
        }
        // The list is kept once and shared with `additionalProperties`, which skips its names.
        let properties: Option<PropertiesId> = (!properties.is_empty()).then(|| {
            properties.sort_by(|a, b| a.0.cmp(&b.0));
            properties.dedup_by(|a, b| a.0 == b.0);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "fewer than u32::MAX lists fit in memory"
            )]
            let id: PropertiesId = PropertiesId(self.properties.len() as u32);
            self.properties.push(properties.into_boxed_slice());
            id
        });
        if let Some(Source::Bool(false)) = parts.additional_properties {
            out.push(Keyword::NoAdditionalProperties(Box::new(KnownProperties {
                names: properties,
                patterns: known_patterns.clone().into_boxed_slice(),
            })));
        }
        out.extend(properties.map(Keyword::Properties));
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
                    known: Box::new(KnownProperties {
                        names: properties,
                        patterns: known_patterns.into_boxed_slice(),
                    }),
                });
            }
        }
        if let Some(source) = parts.property_names {
            let node: NodeId = self.child((source, "propertyNames", None), scope, location);
            out.push(Keyword::PropertyNames(node));
        }
        let schema_lists: [(&Vec<(&'r str, Source<'r>)>, Kw); 2] = [
            (&parts.dependent_schemas, Kw::DependentSchemas),
            (&parts.dependencies_schemas, Kw::Dependencies),
        ];
        for (list, keyword) in schema_lists {
            if list.is_empty() {
                continue;
            }
            let mut dependent: Vec<(Box<str>, NodeId)> = Vec::new();
            for (name, source) in list {
                let node: NodeId = self.child(
                    (*source, keyword.as_str(), Some(Step::Key(name))),
                    scope,
                    location,
                );
                dependent.push((Box::from(*name), node));
            }
            out.push(Keyword::DependentSchemas {
                dependencies: dependent.into_boxed_slice(),
                keyword,
            });
        }

        // Array applicators.
        let tuple_items: bool = facts.tuple_items;
        let (prefix_keyword, rest_keyword): (Kw, Kw) = if parts.tuple_form {
            (Kw::Items, Kw::AdditionalItems)
        } else {
            (Kw::PrefixItems, Kw::Items)
        };
        let prefix: Vec<NodeId> = parts
            .prefix_items
            .iter()
            .enumerate()
            .map(|(i, source)| {
                self.child(
                    (*source, prefix_keyword.as_str(), Some(Step::Index(i))),
                    scope,
                    location,
                )
            })
            .collect();
        let rest: Option<NodeId> = parts
            .items
            .map(|source| self.child((source, rest_keyword.as_str(), None), scope, location));
        match (prefix.is_empty(), rest) {
            (true, None) => {}
            (true, Some(rest)) => out.push(Keyword::Items {
                node: rest,
                keyword: rest_keyword,
            }),
            (false, rest) => out.push(Keyword::PrefixItems(Box::new(PrefixItems {
                prefix: prefix.into_boxed_slice(),
                rest,
                prefix_keyword,
                rest_keyword,
            }))),
        }
        if let Some(source) = parts.contains {
            let node: NodeId = self.child((source, "contains", None), scope, location);
            out.push(Keyword::Contains(Box::new(Contains {
                node,
                min: parts.min_contains.unwrap_or(1),
                max: parts.max_contains,
                annotates: !tuple_items,
                min_keyword: if parts.min_contains.is_some() {
                    Kw::MinContains
                } else {
                    Kw::Contains
                },
            })));
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

    /// Compiles `format`: a check when the dialect asserts formats and defines this one. Under
    /// the `format-assertion` vocabulary an unknown format is an error (2020-12 validation
    /// §7.2.3); otherwise it is ignored.
    fn lower_format(
        &mut self,
        name: &str,
        dialect: Dialect,
        location: usize,
        out: &mut Vec<Keyword>,
    ) {
        if let Some(index) = self.user_formats.iter().position(|f| *f.name == *name) {
            if dialect.format_mode == FormatMode::Assertion {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "fewer than u32::MAX formats fit in memory"
                )]
                out.push(Keyword::UserFormat(UserFormatId(index as u32)));
            }
            return;
        }
        let format: Option<Format> = dialect
            .defines_format(name)
            .then(|| Format::from_name(name))
            .flatten();
        match format {
            Some(format) if dialect.format_mode == FormatMode::Assertion => {
                out.push(Keyword::Format(format));
            }
            None if dialect.vocabulary_active(VocabularyKind::FormatAssertion) => {
                let location: String = self.location_string(location);
                self.errors.push(CompileError::UnknownFormat {
                    location,
                    format: name.to_string(),
                });
            }
            Some(_) | None => {}
        }
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
                Some(Keyword::DynamicRef(Box::new(DynamicRef {
                    fallback,
                    anchor: Box::from(anchor),
                    ref_str: Box::from(ref_str),
                })))
            }
            _ => Some(Keyword::Ref {
                target: fallback,
                keyword: Kw::DynamicRef,
                ref_str: Box::from(ref_str),
            }),
        }
    }
}

/// An array of strings, or `None` when `value` is anything else.
fn strings(value: &Value) -> Option<Vec<&str>> {
    match value {
        Value::Array(items) => items.iter().map(Value::as_str).collect(),
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::{CompiledSchema, Compiler, EnumId, Keyword, Node};
    use crate::json_schema::{Registry, Schema};
    use serde_json::{Value, json};

    fn compile(schema: &Value) -> CompiledSchema {
        let schema: Schema = serde_json::from_value(schema.clone()).expect("test schema parses");
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .expect("test registry builds");
        CompiledSchema::new(&registry).expect("test schema compiles")
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn keyword_and_node_stay_small() {
        let expected: (usize, usize) = (24, 32);
        let actual: (usize, usize) = (size_of::<Keyword>(), size_of::<Node>());
        assert_eq!(expected, actual);
    }

    #[test]
    fn reference_targets_compile_once() {
        let compiled: CompiledSchema = compile(&json!({
            "$defs": {"name": {"type": "string"}},
            "properties": {
                "a": {"$ref": "#/$defs/name"},
                "b": {"$ref": "#/$defs/name"},
                "c": {"minLength": 1},
                "d": {"$ref": "#/properties/c"}
            }
        }));
        // The root, `a` to `d`, and `name`: `c` is a child and a target, and is one node.
        let expected: usize = 6;
        let actual: usize = compiled.nodes.len();
        assert_eq!(expected, actual);
    }

    /// Every interned `enum` list, serialised and sorted.
    fn interned_enums(compiled: &CompiledSchema) -> Vec<Vec<String>> {
        let mut lists: Vec<Vec<String>> = compiled
            .enums
            .iter()
            .map(|list| list.iter().map(Value::to_string).collect())
            .collect();
        lists.sort();
        lists
    }

    #[test]
    fn equal_enum_lists_are_interned_once() {
        let compiled: CompiledSchema = compile(&json!({
            "properties": {
                "a": {"enum": ["x", {"k": [1, null]}]},
                "b": {"enum": ["x", {"k": [1, null]}]},
                "c": {"items": {"enum": ["x", {"k": [1, null]}]}}
            }
        }));
        let expected: Vec<Vec<String>> =
            vec![vec!["\"x\"".to_string(), "{\"k\":[1,null]}".to_string()]];
        let actual: Vec<Vec<String>> = interned_enums(&compiled);
        assert_eq!(expected, actual);
    }

    #[test]
    fn equal_enum_lists_are_interned_once_with_unsorted_maps() {
        let schema: Schema = serde_json::from_value(json!(true)).expect("test schema parses");
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .expect("test registry builds");
        let mut compiler: Compiler<'_, '_> = Compiler::new(&registry);
        compiler.sorted_maps = false;
        let first: Vec<Value> = vec![json!({"b": 1, "a": [2]}), json!("x")];
        let second: Vec<Value> = vec![json!({"a": [2], "b": 1}), json!("x")];
        let expected: (EnumId, EnumId, EnumId) = (EnumId(0), EnumId(0), EnumId(1));
        let actual: (EnumId, EnumId, EnumId) = (
            compiler.intern_enum(&first),
            compiler.intern_enum(&second),
            compiler.intern_enum(&[json!("x")]),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_lists_differing_as_written_are_kept_apart() {
        let compiled: CompiledSchema = compile(&json!({
            "properties": {
                "a": {"enum": [1]},
                "b": {"enum": [1.0]},
                "c": {"enum": [1, 2]},
                "d": {"enum": [2, 1]}
            }
        }));
        let expected: Vec<Vec<String>> = vec![
            vec!["1".to_string()],
            vec!["1".to_string(), "2".to_string()],
            vec!["1.0".to_string()],
            vec!["2".to_string(), "1".to_string()],
        ];
        let actual: Vec<Vec<String>> = interned_enums(&compiled);
        assert_eq!(expected, actual);
    }

    #[test]
    fn properties_list_is_shared_with_additional_properties() {
        let compiled: CompiledSchema = compile(&json!({
            "properties": {"b": true, "a": true},
            "additionalProperties": false
        }));
        let expected: Vec<Vec<String>> = vec![vec!["a".to_string(), "b".to_string()]];
        let actual: Vec<Vec<String>> = compiled
            .properties
            .iter()
            .map(|list| list.iter().map(|(name, _)| name.to_string()).collect())
            .collect();
        assert_eq!(expected, actual);
    }
}
