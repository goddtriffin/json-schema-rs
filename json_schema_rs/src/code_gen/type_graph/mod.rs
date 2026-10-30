//! The language-neutral type graph codegen builds from JSON Schema, before lowering to a target
//! language (see design.md, *Codegen intermediate representation*).
//!
//! A [`TypeGraph`] is an arena of [`TypeNode`]s addressed by [`TypeId`]. Children are ids, never
//! pointers, so a recursive schema is a graph with a cycle and needs no special shape. The graph is
//! built by [`build`] (one walk of every input schema through its `$ref` registry), then the
//! passes in [`passes`] run in a fixed order: structural dedup, cycle breaking, placement.

mod build;
pub(crate) mod passes;

pub(crate) use build::{Conditionals, build};

use crate::json_schema::JsonType;
use serde_json::Value;

/// Index of a node in a [`TypeGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TypeId(pub(crate) u32);

impl TypeId {
    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

/// The width of an integer type, chosen from the schema's exact bounds (narrowest that holds
/// every allowed integer; `I64` when a side is unbounded).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum IntWidth {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

/// The width of a floating-point type: `F32` when both bounds lie within `f32`'s range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum FloatWidth {
    F32,
    F64,
}

/// A string `format` that maps to its own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum StringFormat {
    /// Any string.
    Plain,
    /// `format: "uuid"` (a UUID type when the `uuid` feature is on, else a plain string).
    Uuid,
}

/// String keywords carried to the generated code as metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct StringConstraints {
    pub(crate) min_length: Option<u64>,
    pub(crate) max_length: Option<u64>,
    pub(crate) pattern: Option<String>,
}

/// Array keywords carried to the generated code as metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ArrayConstraints {
    pub(crate) min_items: Option<u64>,
    pub(crate) max_items: Option<u64>,
}

/// Annotations that document a named type. With [`DedupeMode::Full`](crate::DedupeMode::Full)
/// every one takes part in deduplication; with `Functional` only `title` does.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Annotations {
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) comment: Option<String>,
    pub(crate) examples: Option<Vec<Value>>,
    pub(crate) deprecated: Option<bool>,
    pub(crate) id: Option<String>,
}

/// One field of an [`ObjectNode`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Field {
    /// The property name in JSON.
    pub(crate) json_key: String,
    /// The value's type. A nullable value is an [`TypeNode::Option`].
    pub(crate) ty: TypeId,
    /// Whether the property must be present (`required`).
    pub(crate) required: bool,
    /// The property's `default`.
    pub(crate) default: Option<Value>,
    /// The property's `description` (from the property schema itself, not its `$ref` target).
    pub(crate) description: Option<String>,
    /// The property's `deprecated`.
    pub(crate) deprecated: Option<bool>,
}

/// An object with named properties.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ObjectNode {
    /// Fields in JSON key order.
    pub(crate) fields: Vec<Field>,
    /// `additionalProperties: false`.
    pub(crate) deny_unknown: bool,
    /// The value type of `additionalProperties` when it is a schema (other than `true`).
    pub(crate) additional: Option<TypeId>,
    pub(crate) annotations: Annotations,
}

/// Which keyword a union came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum UnionKind {
    AnyOf,
    OneOf,
    /// A `type` array with two or more non-null types.
    TypeArray,
}

/// One alternative of a [`UnionNode`].
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Variant {
    pub(crate) ty: TypeId,
    /// The branch's `title`, when it is an inline schema (not a `$ref`) that has one.
    pub(crate) title: Option<String>,
    /// Whether the branch is a `$ref` (its variant is named after the target's type).
    pub(crate) is_ref: bool,
}

/// `oneOf` / `anyOf` (or a multi-type `type` array): exactly one / at least one alternative.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnionNode {
    pub(crate) kind: UnionKind,
    /// Alternatives in schema order (integers before numbers).
    pub(crate) variants: Vec<Variant>,
    pub(crate) annotations: Annotations,
}

/// `enum` / `const`: a closed set of JSON values.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EnumNode {
    /// The allowed values, deduplicated by JSON equality, in a fixed order: strings
    /// alphabetically when every value is a string, otherwise by their JSON text.
    pub(crate) values: Vec<Value>,
    pub(crate) annotations: Annotations,
}

impl EnumNode {
    /// Whether every value is a string.
    pub(crate) fn is_string_enum(&self) -> bool {
        self.values.iter().all(Value::is_string)
    }
}

/// One node of the type graph.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TypeNode {
    /// Not yet built (reserved id); never left in a finished graph.
    Pending,
    /// Any JSON value (`{}`, `true`, a schema with no type information).
    Any,
    /// Only `null`.
    Null,
    Boolean,
    Integer(IntWidth),
    Number(FloatWidth),
    String {
        format: StringFormat,
        constraints: StringConstraints,
    },
    Array {
        items: TypeId,
        unique: bool,
        constraints: ArrayConstraints,
    },
    /// An object with no named properties: string keys to `values`.
    Map {
        values: TypeId,
    },
    /// `null` or the inner type.
    Option(TypeId),
    /// The inner type behind a pointer: placed on back-edges by cycle breaking.
    Box(TypeId),
    Object(ObjectNode),
    Enum(EnumNode),
    Union(UnionNode),
}

impl TypeNode {
    /// Whether this node becomes a named item (struct, enum) in the generated code.
    pub(crate) fn is_named(&self) -> bool {
        matches!(
            self,
            TypeNode::Object(_) | TypeNode::Enum(_) | TypeNode::Union(_)
        )
    }

    /// The node's children in a fixed order.
    pub(crate) fn children(&self) -> Vec<TypeId> {
        match self {
            TypeNode::Pending
            | TypeNode::Any
            | TypeNode::Null
            | TypeNode::Boolean
            | TypeNode::Integer(_)
            | TypeNode::Number(_)
            | TypeNode::String { .. }
            | TypeNode::Enum(_) => Vec::new(),
            TypeNode::Array { items, .. } => vec![*items],
            TypeNode::Map { values } => vec![*values],
            TypeNode::Option(inner) | TypeNode::Box(inner) => vec![*inner],
            TypeNode::Object(o) => o.fields.iter().map(|f| f.ty).chain(o.additional).collect(),
            TypeNode::Union(u) => u.variants.iter().map(|v| v.ty).collect(),
        }
    }

    /// Mutable references to the node's children, in the order of [`TypeNode::children`].
    pub(crate) fn children_mut(&mut self) -> Vec<&mut TypeId> {
        match self {
            TypeNode::Pending
            | TypeNode::Any
            | TypeNode::Null
            | TypeNode::Boolean
            | TypeNode::Integer(_)
            | TypeNode::Number(_)
            | TypeNode::String { .. }
            | TypeNode::Enum(_) => Vec::new(),
            TypeNode::Array { items, .. } => vec![items],
            TypeNode::Map { values } => vec![values],
            TypeNode::Option(inner) | TypeNode::Box(inner) => vec![inner],
            TypeNode::Object(o) => o
                .fields
                .iter_mut()
                .map(|f| &mut f.ty)
                .chain(o.additional.as_mut())
                .collect(),
            TypeNode::Union(u) => u.variants.iter_mut().map(|v| &mut v.ty).collect(),
        }
    }
}

/// A name a node could take, with where it came from. The naming pass picks the best one per
/// node: lowest [`NameTier`], then lowest origin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NameProposal {
    pub(crate) tier: NameTier,
    /// Where the proposal was made: (schema index, JSON Pointer), compared in that order.
    pub(crate) origin: (usize, String),
    /// The unsanitized name text.
    pub(crate) text: String,
}

/// How strongly a name is wanted (lower wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NameTier {
    /// The key of a `$defs` / `definitions` entry.
    Definition,
    /// The schema's `title` (or the property key, with `ModelNameSource::PropertyKeyFirst`).
    Preferred,
    /// `Root`, for a schema's root without a title: it outranks the key of a property that
    /// refers back to the root (`{"children": {"items": {"$ref": "#"}}}`).
    Root,
    /// The property key (or the `title`, with `ModelNameSource::PropertyKeyFirst`).
    Secondary,
    /// A fallback.
    Fallback,
}

/// Where a schema's roots sit in the graph.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SchemaRoots {
    /// The root schema's type.
    pub(crate) root: TypeId,
    /// Every `$defs` / `definitions` entry of the root, by key (`$defs` first).
    pub(crate) definitions: Vec<(String, TypeId)>,
}

/// The arena of type nodes plus, per node, the names proposed for it.
#[derive(Debug, Clone, Default)]
pub(crate) struct TypeGraph {
    pub(crate) nodes: Vec<TypeNode>,
    pub(crate) proposals: Vec<Vec<NameProposal>>,
    /// Per input schema, the root and its definitions.
    pub(crate) schemas: Vec<SchemaRoots>,
}

impl TypeGraph {
    pub(crate) fn node(&self, id: TypeId) -> &TypeNode {
        &self.nodes[id.index()]
    }

    /// Adds `node` and returns its id.
    pub(crate) fn push(&mut self, node: TypeNode) -> TypeId {
        let id: TypeId = TypeId(u32::try_from(self.nodes.len()).expect("fewer than 2^32 nodes"));
        self.nodes.push(node);
        self.proposals.push(Vec::new());
        id
    }

    /// `id` with every [`TypeNode::Option`] and [`TypeNode::Box`] around it removed.
    pub(crate) fn peel(&self, mut id: TypeId) -> TypeId {
        while let TypeNode::Option(inner) | TypeNode::Box(inner) = self.node(id) {
            id = *inner;
        }
        id
    }
}

/// The JSON types a node of each scalar kind holds, for ordering union variants.
pub(crate) fn scalar_json_type(node: &TypeNode) -> Option<JsonType> {
    match node {
        TypeNode::Null => Some(JsonType::Null),
        TypeNode::Boolean => Some(JsonType::Boolean),
        TypeNode::Integer(_) => Some(JsonType::Integer),
        TypeNode::Number(_) => Some(JsonType::Number),
        TypeNode::String { .. } => Some(JsonType::String),
        TypeNode::Array { .. } => Some(JsonType::Array),
        TypeNode::Map { .. } | TypeNode::Object(_) => Some(JsonType::Object),
        TypeNode::Pending
        | TypeNode::Any
        | TypeNode::Option(_)
        | TypeNode::Box(_)
        | TypeNode::Enum(_)
        | TypeNode::Union(_) => None,
    }
}
