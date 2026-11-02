//! Self-validating generated types ([`CodeGenSettings::self_validating`]): the embedded
//! documents, the `TypeValidators` statics and the per-type validation code the Rust backend
//! emits (see design.md, *Self-validating types*).
//!
//! Each output buffer gets one `TypeValidators` static per input schema whose registry its
//! types validate through: the documents of that registry (the input, the inputs and documents
//! its `$ref`s reached) as JSON text, and one entry per type, the schema locations
//! ([`ValidationTarget`]) the type must satisfy. The static compiles everything once, on first
//! use; codegen compiles the same data before emitting it, so the generated code never meets a
//! schema error.
//!
//! [`CodeGenSettings::self_validating`]: super::CodeGenSettings::self_validating

use super::type_graph::{TypeGraph, TypeId, TypeNode, UnionKind, ValidationTarget};
use super::{CodeGenError, CodeGenResult};
use crate::json_schema::{
    DialectSettings, JsonSchemaSettings, Registry, RegistryDocumentKind, SchemaRef,
};
use crate::validator::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

/// One embedded document, owned (the generated code borrows it as `&'static str`).
#[derive(Debug, Clone)]
struct OwnedDocument {
    uri: String,
    json: String,
    kind: EmbeddedDocumentKind,
}

/// One input's registry as generated code embeds it.
#[derive(Debug, Clone)]
pub(crate) struct EmbeddedSet {
    /// The dialect settings retrieved documents are parsed with (`None`: the defaults).
    settings: Option<DialectSettings>,
    /// The registry's documents, the input first.
    documents: Vec<OwnedDocument>,
}

/// The embedded documents of every input's registry.
pub(crate) fn embedded_sets(
    registries: &[Registry<'_>],
    retrieval_settings: &JsonSchemaSettings,
) -> CodeGenResult<Vec<EmbeddedSet>> {
    let settings: Option<DialectSettings> = non_default(retrieval_settings.dialect_settings());
    registries
        .iter()
        .map(|registry| {
            let documents: Vec<OwnedDocument> = registry
                .embeddable_documents()
                .into_iter()
                .map(|document| {
                    let serialized: Result<String, serde_json::Error> = match document.schema {
                        SchemaRef::Bool(b) => Ok(b.to_string()),
                        SchemaRef::Object(o) => serde_json::to_string(o),
                    };
                    let json: String = serialized.map_err(|e| {
                        CodeGenError::Io(std::io::Error::other(format!(
                            "could not serialize the schema document {}: {e}",
                            document.uri
                        )))
                    })?;
                    let kind: EmbeddedDocumentKind = match document.kind {
                        RegistryDocumentKind::Root | RegistryDocumentKind::Parsed => {
                            EmbeddedDocumentKind::Parsed(
                                document
                                    .schema
                                    .as_object()
                                    .and_then(|s| s.dialect_settings)
                                    .and_then(non_default),
                            )
                        }
                        RegistryDocumentKind::Loaded => EmbeddedDocumentKind::Loaded,
                    };
                    Ok(OwnedDocument {
                        uri: document.uri.to_string(),
                        json,
                        kind,
                    })
                })
                .collect::<CodeGenResult<Vec<OwnedDocument>>>()?;
            Ok(EmbeddedSet {
                settings,
                documents,
            })
        })
        .collect()
}

fn non_default(settings: DialectSettings) -> Option<DialectSettings> {
    (settings != DialectSettings::default()).then_some(settings)
}

/// Where a type's validator lives in the generated code: its static and entry index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) validators: String,
    pub(crate) index: usize,
}

/// The validation code of one union: its own entry and one per variant, in variant order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnionEntries {
    pub(crate) validators: String,
    pub(crate) union: Option<usize>,
    pub(crate) variants: Vec<usize>,
}

/// The validators of one output buffer: per input schema, the entries its types use.
#[derive(Debug, Default)]
pub(crate) struct BufferValidators {
    /// Input schema → entries (URI lists) in first-use order.
    sets: BTreeMap<usize, Vec<Vec<String>>>,
    index: HashMap<(usize, Vec<String>), usize>,
    /// Named item → its entry (input schema, index).
    items: HashMap<TypeId, (usize, usize)>,
    /// `anyOf` / `oneOf` union → (input schema, union entry, variant entries).
    unions: HashMap<TypeId, (usize, Option<usize>, Vec<usize>)>,
}

impl BufferValidators {
    /// Registers the validators of `items` (the named items of one buffer).
    pub(crate) fn new(graph: &TypeGraph, items: &[TypeId]) -> BufferValidators {
        let mut out: BufferValidators = BufferValidators::default();
        for &id in items {
            let target: Option<&ValidationTarget> = graph.targets[id.index()].as_ref();
            if let Some(target) = target {
                let index: usize = out.register(target.schema, &target.uris);
                out.items.insert(id, (target.schema, index));
            }
            let TypeNode::Union(union) = graph.node(id) else {
                continue;
            };
            if union.kind == UnionKind::TypeArray {
                continue;
            }
            // Every variant was built in the union's walk, so the union's registry resolves
            // their locations; a union without a target uses its first variant's registry.
            let schema: Option<usize> = target.map(|t| t.schema).or_else(|| {
                union
                    .variants
                    .iter()
                    .find_map(|v| graph.targets[graph.peel(v.ty).index()].as_ref())
                    .map(|t| t.schema)
            });
            let Some(schema) = schema else {
                continue;
            };
            let union_index: Option<usize> = target.map(|_| out.items[&id].1);
            let variants: Vec<usize> = union
                .variants
                .iter()
                .map(|v| {
                    // A variant with no target accepts any value (a `true` branch): the empty
                    // conjunction.
                    let uris: &[String] = graph.targets[graph.peel(v.ty).index()]
                        .as_ref()
                        .map_or(&[], |t| t.uris.as_slice());
                    out.register(schema, uris)
                })
                .collect();
            out.unions.insert(id, (schema, union_index, variants));
        }
        out
    }

    fn register(&mut self, schema: usize, uris: &[String]) -> usize {
        let key: (usize, Vec<String>) = (schema, uris.to_vec());
        if let Some(&index) = self.index.get(&key) {
            return index;
        }
        let entries: &mut Vec<Vec<String>> = self.sets.entry(schema).or_default();
        let index: usize = entries.len();
        entries.push(uris.to_vec());
        self.index.insert(key, index);
        index
    }

    /// The name of input `schema`'s static: `TYPE_VALIDATORS`, or with a numeric suffix on
    /// every one when the buffer has several.
    fn static_name(&self, schema: usize) -> String {
        if self.sets.len() == 1 {
            return "TYPE_VALIDATORS".to_string();
        }
        let position: usize = self
            .sets
            .keys()
            .position(|&s| s == schema)
            .expect("a registered schema");
        format!("TYPE_VALIDATORS{position}")
    }

    pub(crate) fn item(&self, id: TypeId) -> Option<Entry> {
        self.items.get(&id).map(|&(schema, index)| Entry {
            validators: self.static_name(schema),
            index,
        })
    }

    pub(crate) fn union(&self, id: TypeId) -> Option<UnionEntries> {
        self.unions
            .get(&id)
            .map(|(schema, union, variants)| UnionEntries {
                validators: self.static_name(*schema),
                union: *union,
                variants: variants.clone(),
            })
    }

    /// The statics, after compiling each one as the generated code will (an error here is a
    /// schema the validator cannot use, reported at generation time).
    pub(crate) fn render(&self, sets: &[EmbeddedSet]) -> CodeGenResult<String> {
        let mut out: String = String::new();
        for (&schema, entries) in &self.sets {
            let set: &EmbeddedSet = &sets[schema];
            check(set, entries).map_err(|e| CodeGenError::Batch {
                index: schema,
                source: Box::new(e),
            })?;
            let _ = writeln!(
                out,
                "static {}: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(",
                self.static_name(schema)
            );
            let _ = writeln!(out, "    {},", dialect_settings_tokens(set.settings));
            out.push_str("    &[\n");
            for document in &set.documents {
                out.push_str("        json_schema_rs::EmbeddedDocument {\n");
                let _ = writeln!(out, "            uri: {},", string_literal(&document.uri));
                let _ = writeln!(out, "            json: {},", raw_string(&document.json));
                let kind: String = match document.kind {
                    EmbeddedDocumentKind::Parsed(settings) => format!(
                        "json_schema_rs::EmbeddedDocumentKind::Parsed({})",
                        dialect_settings_tokens(settings)
                    ),
                    EmbeddedDocumentKind::Loaded => {
                        "json_schema_rs::EmbeddedDocumentKind::Loaded".to_string()
                    }
                };
                let _ = writeln!(out, "            kind: {kind},");
                out.push_str("        },\n");
            }
            out.push_str("    ],\n    &[\n");
            for uris in entries {
                let list: Vec<String> = uris.iter().map(|u| string_literal(u)).collect();
                let _ = writeln!(out, "        &[{}],", list.join(", "));
            }
            out.push_str("    ],\n);\n\n");
        }
        Ok(out)
    }
}

/// Compiles `entries` against `set` exactly as the generated static will.
fn check(set: &EmbeddedSet, entries: &[Vec<String>]) -> CodeGenResult<()> {
    let documents: Vec<EmbeddedDocument<'_>> = set
        .documents
        .iter()
        .map(|d| EmbeddedDocument {
            uri: &d.uri,
            json: &d.json,
            kind: d.kind,
        })
        .collect();
    let uris: Vec<Vec<&str>> = entries
        .iter()
        .map(|e| e.iter().map(String::as_str).collect())
        .collect();
    let types: Vec<&[&str]> = uris.iter().map(Vec::as_slice).collect();
    TypeValidators::new(set.settings, &documents, &types)
        .check()
        .map_err(CodeGenError::SelfValidation)
}

/// `validate` and `TryFrom<serde_json::Value>` for the item `name`.
pub(crate) fn render_item_impls(name: &str, entry: &Entry) -> String {
    let Entry { validators, index } = entry;
    format!(
        "impl {name} {{
    /// Validates this value against its schema: every error the schema's validator reports.
    pub fn validate(&self) -> Result<(), json_schema_rs::TypeValidationError> {{
        {validators}.validate_serialized({index}, self)
    }}
}}

impl TryFrom<serde_json::Value> for {name} {{
    type Error = json_schema_rs::TypeValidationError;

    /// Validates `value` against the schema, then deserializes it.
    fn try_from(value: serde_json::Value) -> Result<Self, Self::Error> {{
        {validators}.deserialize({index}, value)
    }}
}}

"
    )
}

/// The `Deserialize` of an `anyOf` / `oneOf` union: buffer the value once, then take the first
/// variant whose branch the value satisfies (for `oneOf`, the one) and that deserializes.
/// `variants` holds each variant's name and Rust type.
pub(crate) fn render_union_deserialize(
    name: &str,
    kind: UnionKind,
    entries: &UnionEntries,
    variants: &[(String, String)],
) -> String {
    let mode: &str = match kind {
        UnionKind::OneOf => "OneOf",
        UnionKind::AnyOf | UnionKind::TypeArray => "AnyOf",
    };
    let union: String = entries
        .union
        .map_or_else(|| "None".to_string(), |i| format!("Some({i})"));
    let indexes: Vec<String> = entries.variants.iter().map(usize::to_string).collect();
    let mut out: String = String::new();
    let _ = writeln!(out, "impl<'de> Deserialize<'de> for {name} {{");
    out.push_str(
        "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {\n",
    );
    out.push_str(
        "        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;\n",
    );
    let _ = writeln!(
        out,
        "        let candidates: Vec<usize> = {}
            .union_candidates(json_schema_rs::UnionMatch::{mode}, {union}, &[{}], &value)
            .map_err(serde::de::Error::custom)?;",
        entries.validators,
        indexes.join(", ")
    );
    out.push_str("        for candidate in candidates {\n");
    out.push_str("            let variant: Result<Self, serde_json::Error> = match candidate {\n");
    for (i, (variant, ty)) in variants.iter().enumerate() {
        let _ = writeln!(
            out,
            "                {i} => <{ty} as Deserialize>::deserialize(&value).map(Self::{variant}),"
        );
    }
    out.push_str("                _ => continue,\n            };\n");
    out.push_str("            if let Ok(variant) = variant {\n                return Ok(variant);\n            }\n        }\n");
    let _ = writeln!(
        out,
        "        Err(serde::de::Error::custom(format!(\"{{value}} does not deserialize into a {name} variant whose schema it satisfies\")))\n    }}\n}}\n"
    );
    out
}

/// A Rust string literal for `s`.
fn string_literal(s: &str) -> String {
    format!("{s:?}")
}

/// A raw string literal for `s`, with as many `#` as it needs.
fn raw_string(s: &str) -> String {
    let mut hashes: usize = 0;
    loop {
        let closing: String = format!("\"{}", "#".repeat(hashes));
        if !s.contains(&closing) {
            break;
        }
        hashes += 1;
    }
    let fence: String = "#".repeat(hashes);
    format!("r{fence}\"{s}\"{fence}")
}

/// `None`, or the `DialectSettings` struct literal.
fn dialect_settings_tokens(settings: Option<DialectSettings>) -> String {
    let Some(s) = settings else {
        return "None".to_string();
    };
    let field = |value: Option<String>| -> String {
        value.map_or_else(|| "None".to_string(), |v| format!("Some({v})"))
    };
    format!(
        "Some(json_schema_rs::DialectSettings {{ forced_spec_version: {}, default_spec_version: {}, format_mode: {}, integer_semantics: {}, exclusive_bounds: {}, ref_siblings: {} }})",
        field(
            s.forced_spec_version
                .map(|v| format!("json_schema_rs::SpecVersion::{v:?}"))
        ),
        field(
            s.default_spec_version
                .map(|v| format!("json_schema_rs::SpecVersion::{v:?}"))
        ),
        field(
            s.format_mode
                .map(|v| format!("json_schema_rs::FormatMode::{v:?}"))
        ),
        field(
            s.integer_semantics
                .map(|v| format!("json_schema_rs::IntegerSemantics::{v:?}"))
        ),
        field(
            s.exclusive_bounds
                .map(|v| format!("json_schema_rs::ExclusiveBounds::{v:?}"))
        ),
        field(
            s.ref_siblings
                .map(|v| format!("json_schema_rs::RefSiblings::{v:?}"))
        ),
    )
}
