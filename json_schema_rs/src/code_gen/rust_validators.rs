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
//! What a static embeds does not depend on the machine: a document read from a file is
//! identified by its path relative to the deepest directory holding every file document
//! (`file:///schemas/pet.json`), never by its absolute path. Each static's documents are pruned
//! to what its entries' validation reads ([`embedded_prune`](super::embedded_prune)); codegen
//! compiles the pruned documents and embeds them only when they compile to the same validator
//! as the documents it read, else the unpruned ones.
//!
//! [`CodeGenSettings::self_validating`]: super::CodeGenSettings::self_validating

use super::embedded_prune::prune;
use super::type_graph::{TargetSources, TypeGraph, TypeId, TypeNode, UnionKind, ValidationTarget};
use super::{CodeGenError, CodeGenResult};
use crate::json_schema::{
    DialectSettings, JsonSchema, JsonSchemaSettings, Registry, RegistryDocumentKind, SchemaRef,
    Scope,
};
use crate::validator::{
    CompiledEntries, CompiledSchema, EmbeddedDocument, EmbeddedDocumentKind, NodeId,
    TypeValidators, validation_classes,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::sync::Arc;

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
    /// How the URIs of documents read from files are made machine-independent.
    rebase: Arc<FileRebase>,
    /// The root document's URI and, when the root declares `$id`, its resource's URI, without
    /// fragment: entries resolve like a `$ref` written in the root, so an entry under either is
    /// embedded as its fragment alone (`#/$defs/a`).
    root_bases: Vec<String>,
}

/// Maps the `file:` URIs of documents (and of the schemas in them) to their path relative to the
/// deepest directory holding every `file:` document of the generation: with
/// `file:///home/ada/api/schemas/` as that directory, `file:///home/ada/api/schemas/pet.json`
/// becomes `file:///pet.json` on every machine. Relative references between the documents
/// resolve exactly as before, since every one moves by the same prefix.
#[derive(Debug, Default)]
struct FileRebase {
    /// The common directory, ending in `/`; `None` without `file:` documents.
    root: Option<String>,
}

impl FileRebase {
    fn new<'u>(uris: impl Iterator<Item = &'u str>) -> FileRebase {
        let mut root: Option<&str> = None;
        for uri in uris.filter(|uri| uri.starts_with("file:")) {
            let directory: &str = uri.rfind('/').map_or(uri, |end| &uri[..=end]);
            root = Some(match root {
                None => directory,
                Some(current) => {
                    let common: usize = current
                        .bytes()
                        .zip(directory.bytes())
                        .take_while(|(a, b)| a == b)
                        .count();
                    let prefix: &str = &current[..common];
                    prefix.rfind('/').map_or("", |end| &prefix[..=end])
                }
            });
        }
        FileRebase {
            root: root
                .filter(|root| root.len() > "file:".len())
                .map(str::to_string),
        }
    }

    /// `uri` with the common directory replaced by `file:///`.
    fn apply(&self, uri: &str) -> String {
        match &self.root {
            Some(root) if uri.starts_with(root.as_str()) => {
                format!("file:///{}", &uri[root.len()..])
            }
            _ => uri.to_string(),
        }
    }
}

/// The embedded documents of every input's registry.
pub(crate) fn embedded_sets(
    registries: &[Registry<'_>],
    retrieval_settings: &JsonSchemaSettings,
) -> CodeGenResult<Vec<EmbeddedSet>> {
    let settings: Option<DialectSettings> = non_default(retrieval_settings.dialect_settings());
    let rebase: Arc<FileRebase> = Arc::new(FileRebase::new(registries.iter().flat_map(|r| {
        r.embeddable_documents()
            .into_iter()
            .map(|document| document.uri)
    })));
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
            let document_scope: Scope = registry.root_scope();
            let resource_scope: Scope = match registry.root() {
                SchemaRef::Object(root) => registry.enter(document_scope, root),
                SchemaRef::Bool(_) => document_scope,
            };
            let mut root_bases: Vec<String> = Vec::new();
            for scope in [resource_scope, document_scope] {
                if let Some(uri) = registry.location_uri(scope, &[]) {
                    let base: String = uri.trim_end_matches('#').to_string();
                    if !base.is_empty() && !root_bases.contains(&base) {
                        root_bases.push(base);
                    }
                }
            }
            Ok(EmbeddedSet {
                settings,
                documents,
                rebase: Arc::clone(&rebase),
                root_bases,
            })
        })
        .collect()
}

fn non_default(settings: DialectSettings) -> Option<DialectSettings> {
    (settings != DialectSettings::default()).then_some(settings)
}

/// Per type-graph node, what deduplication compares its validation by (see design.md,
/// *Self-validating types*, *Identity*): two nodes with equal keys validate every instance
/// identically, errors included, wherever their schemas sit. Each input's targets are compiled
/// once, from the schema objects the walk found them at (`sources`), and a target's key is the
/// [`validation_classes`] of its entry nodes. Only `candidates` (see `merge_candidates`) are
/// compiled; every other node, and every node of a registry that tracks the dynamic scope, is
/// keyed by location ([`ValidationTarget::key`]). `None` for a node without a target.
pub(crate) fn validation_keys<'r>(
    graph: &TypeGraph,
    registries: &'r [Registry<'r>],
    sources: &TargetSources<'r>,
    sets: &[EmbeddedSet],
    candidates: &[bool],
) -> CodeGenResult<Vec<Option<String>>> {
    /// One input's distinct entries, in first-use order: their URIs, their schema objects, and
    /// each URI list's index.
    #[derive(Default)]
    struct Group<'r> {
        uris: Vec<Vec<String>>,
        schemas: Vec<Vec<(&'r JsonSchema, Scope)>>,
        index: HashMap<Vec<String>, usize>,
    }
    let mut groups: BTreeMap<usize, Group<'r>> = BTreeMap::new();
    for (i, (target, &candidate)) in graph.targets.iter().zip(candidates).enumerate() {
        let Some(target) = target.as_ref().filter(|_| candidate) else {
            continue;
        };
        let group: &mut Group<'r> = groups.entry(target.schema).or_default();
        if !group.index.contains_key(&target.uris) {
            group.index.insert(target.uris.clone(), group.uris.len());
            group.uris.push(target.uris.clone());
            let id: TypeId = TypeId(u32::try_from(i).expect("fewer than 2^32 nodes"));
            group.schemas.push(sources[&id].clone());
        }
    }
    let mut compiled: Vec<(usize, CompiledEntries)> = Vec::with_capacity(groups.len());
    for (&schema, group) in &groups {
        // The schema objects in the input's own registry; the embedded documents when they do
        // not compile there (a `$ref` the registry resolved only through an entry), which
        // reports a schema the validator cannot compile as the generated static would.
        let entries: CompiledEntries = if let Ok(entries) =
            CompiledSchema::with_schema_entries(&registries[schema], &group.schemas)
        {
            entries
        } else {
            let set: &EmbeddedSet = &sets[schema];
            #[expect(
                clippy::redundant_closure_for_method_calls,
                reason = "the method path is bound to one `TypeValidators` lifetime; the closure is not"
            )]
            let compiled = compile(set.settings, &set.documents, &group.uris, |v| v.compile());
            compiled.map_err(|e| CodeGenError::Batch {
                index: schema,
                source: Box::new(CodeGenError::SelfValidation(e)),
            })?
        };
        compiled.push((schema, entries));
    }
    let schemas: Vec<&CompiledSchema> = compiled.iter().map(|(_, (schema, _))| schema).collect();
    let classes: Vec<Option<Vec<u32>>> = validation_classes(&schemas);
    let position: HashMap<usize, usize> = compiled
        .iter()
        .enumerate()
        .map(|(position, (schema, _))| (*schema, position))
        .collect();
    Ok(graph
        .targets
        .iter()
        .zip(candidates)
        .map(|(target, &candidate)| {
            let target: &ValidationTarget = target.as_ref()?;
            if !candidate {
                return Some(format!("location|{}", target.key()));
            }
            let at: usize = position[&target.schema];
            let Some(node_classes) = &classes[at] else {
                return Some(format!("location|{}", target.key()));
            };
            let entry: usize = groups[&target.schema].index[&target.uris];
            let nodes: &[NodeId] = &compiled[at].1.1[entry];
            let mut key: String = "validation".to_string();
            for node in nodes {
                let _ = write!(key, "|{}", node_classes[node.index()]);
            }
            Some(key)
        })
        .collect())
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
    /// Registers the validators of `items` (the named items of one buffer); `unions`: whether
    /// `anyOf` / `oneOf` unions choose their variant by validation (self-validating types).
    pub(crate) fn new(graph: &TypeGraph, items: &[TypeId], unions: bool) -> BufferValidators {
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
            if !unions {
                continue;
            }
            if union.kind == UnionKind::TypeArray {
                continue;
            }
            // The variants' targets as the union's walk built them (a merged variant type may
            // carry an equivalent target elsewhere), so the union's registry resolves them; a
            // union without a target uses its first variant's registry.
            let Some(variant_targets) = graph.variant_targets.get(&id) else {
                continue;
            };
            let schema: Option<usize> = target
                .map(|t| t.schema)
                .or_else(|| variant_targets.iter().flatten().map(|t| t.schema).next());
            let Some(schema) = schema else {
                continue;
            };
            let union_index: Option<usize> = target.map(|_| out.items[&id].1);
            let variants: Vec<usize> = variant_targets
                .iter()
                .map(|variant| {
                    // A variant with no target accepts any value (a `true` branch): the empty
                    // conjunction.
                    let uris: &[String] = variant.as_ref().map_or(&[], |t| t.uris.as_slice());
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
            let (documents, entries): (Vec<OwnedDocument>, Vec<Vec<String>>) =
                embedding(&sets[schema], entries).map_err(|e| CodeGenError::Batch {
                    index: schema,
                    source: Box::new(e),
                })?;
            let set: &EmbeddedSet = &sets[schema];
            let _ = writeln!(
                out,
                "static {}: json_schema_rs::TypeValidators<'static> = json_schema_rs::TypeValidators::new(",
                self.static_name(schema)
            );
            let _ = writeln!(out, "    {},", dialect_settings_tokens(set.settings));
            out.push_str("    &[\n");
            for document in &documents {
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
            for uris in &entries {
                let list: Vec<String> = uris.iter().map(|u| string_literal(u)).collect();
                let _ = writeln!(out, "        &[{}],", list.join(", "));
            }
            out.push_str("    ],\n);\n\n");
        }
        Ok(out)
    }
}

/// What one static embeds: its documents and its entries' URIs. Compiles `entries` against
/// `set` exactly as the generated static will (an error is a schema the validator cannot use),
/// then makes the URIs machine-independent and prunes the documents to what the compiled schemas
/// sit in. The first candidate that compiles to the same validator as the documents read is
/// embedded: rebased and pruned, else rebased, else as read; the first two each with the entries
/// under the root resource written as their fragment alone, else in full.
fn embedding(
    set: &EmbeddedSet,
    entries: &[Vec<String>],
) -> CodeGenResult<(Vec<OwnedDocument>, Vec<Vec<String>>)> {
    let ((reference, reference_entries), located) =
        compile(set.settings, &set.documents, entries, |v| {
            v.compile_located()
        })
        .map_err(CodeGenError::SelfValidation)?;
    let rebase = |uri: &str| -> String { set.rebase.apply(uri) };
    let rebased_entries: Vec<Vec<String>> = entries
        .iter()
        .map(|uris| uris.iter().map(|uri| rebase(uri)).collect())
        .collect();
    let rebased: Vec<OwnedDocument> = set
        .documents
        .iter()
        .map(|document| OwnedDocument {
            uri: rebase(&document.uri),
            ..document.clone()
        })
        .collect();
    let mut kept: HashMap<&str, Vec<&[String]>> = HashMap::new();
    for (uri, pointer) in &located {
        kept.entry(uri.as_str()).or_default().push(pointer);
    }
    let none: Vec<&[String]> = Vec::new();
    let pruned: Option<Vec<OwnedDocument>> = set
        .documents
        .iter()
        .zip(&rebased)
        .map(|(read, rebased)| {
            let mut value: Value = serde_json::from_str(&rebased.json).ok()?;
            prune(&mut value, kept.get(read.uri.as_str()).unwrap_or(&none));
            Some(OwnedDocument {
                json: serde_json::to_string(&value).ok()?,
                ..rebased.clone()
            })
        })
        .collect();
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "the method path is bound to one `TypeValidators` lifetime; the closure is not"
    )]
    let same = |documents: &[OwnedDocument], entries: &[Vec<String>]| -> bool {
        compile(set.settings, documents, entries, |v| v.compile()).is_ok_and(
            |(compiled, compiled_entries)| {
                compiled_entries == reference_entries
                    && reference.same_validation(&compiled, |uri| rebase(uri))
            },
        )
    };
    let root_bases: Vec<String> = set.root_bases.iter().map(|base| rebase(base)).collect();
    let short = |uri: &String| -> String {
        root_bases
            .iter()
            .find_map(|base| {
                uri.strip_prefix(base.as_str())
                    .filter(|fragment| fragment.starts_with('#'))
            })
            .map_or_else(|| uri.clone(), str::to_string)
    };
    let short_entries: Vec<Vec<String>> = rebased_entries
        .iter()
        .map(|uris| uris.iter().map(short).collect())
        .collect();
    let with_entries = |documents: Vec<OwnedDocument>| {
        if same(&documents, &short_entries) {
            return Some((documents, short_entries.clone()));
        }
        log::debug!(
            "json-schema-rs: the embedded entries resolve differently as fragments; embedding them in full"
        );
        same(&documents, &rebased_entries).then(|| (documents, rebased_entries.clone()))
    };
    if let Some(embedded) = pruned.and_then(with_entries) {
        return Ok(embedded);
    }
    log::debug!(
        "json-schema-rs: the pruned embedded documents validate differently; embedding them whole"
    );
    if let Some(embedded) = with_entries(rebased) {
        return Ok(embedded);
    }
    log::debug!(
        "json-schema-rs: the embedded documents validate differently under machine-independent URIs; embedding their URIs as read"
    );
    Ok((set.documents.clone(), entries.to_vec()))
}

/// Runs `compile` on a [`TypeValidators`] over `documents` and `entries`, as the generated
/// static would.
fn compile<T>(
    settings: Option<DialectSettings>,
    documents: &[OwnedDocument],
    entries: &[Vec<String>],
    compile: impl FnOnce(&TypeValidators<'_>) -> Result<T, crate::validator::EmbeddedSchemaError>,
) -> Result<T, Arc<crate::validator::EmbeddedSchemaError>> {
    let documents: Vec<EmbeddedDocument<'_>> = documents
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
    compile(&TypeValidators::new(settings, &documents, &types)).map_err(Arc::new)
}

/// `validate` and `TryFrom<serde_json::Value>` for the item `name`: one
/// `json_schema_rs::self_validating!` line.
pub(crate) fn render_item_impls(name: &str, entry: &Entry) -> String {
    let Entry { validators, index } = entry;
    format!("json_schema_rs::self_validating!({name}, {validators}, {index});\n\n")
}

/// One variant of a generated union.
pub(crate) struct UnionVariant {
    pub(crate) name: String,
    /// Its Rust type.
    pub(crate) ty: String,
    /// It reads through `json_schema_rs::integer` (an integer type, or a container of them).
    pub(crate) integers: bool,
}

/// The `Deserialize` of an `anyOf` / `oneOf` union: one `json_schema_rs::deserialize_union!`,
/// whose runtime reads the variant validation recorded for the value, else buffers the value and
/// takes the first variant whose branch it satisfies (for `oneOf`, the one) and that
/// deserializes. Each arm reads the variant at a position.
pub(crate) fn render_union_deserialize(
    name: &str,
    kind: UnionKind,
    entries: &UnionEntries,
    variants: &[UnionVariant],
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
    let _ = writeln!(
        out,
        "json_schema_rs::deserialize_union!({name}, {}, {mode}, {union}, [{}], |value| {{",
        entries.validators,
        indexes.join(", ")
    );
    for (
        i,
        UnionVariant {
            name: variant,
            ty,
            integers,
        },
    ) in variants.iter().enumerate()
    {
        let arm: String = if i + 1 == variants.len() {
            "_".to_string()
        } else {
            i.to_string()
        };
        let read: String = if *integers {
            format!("json_schema_rs::integer::deserialize::<_, {ty}>(value).map(Self::{variant})")
        } else {
            format!("<{ty} as Deserialize>::deserialize(value).map(Self::{variant})")
        };
        let _ = writeln!(out, "    {arm} => {read},");
    }
    out.push_str(
        "});

",
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
        "Some(json_schema_rs::DialectSettings {{ forced_spec_version: {}, default_spec_version: {}, format_mode: {}, content_mode: {}, integer_semantics: {}, exclusive_bounds: {}, ref_siblings: {} }})",
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
            s.content_mode
                .map(|v| format!("json_schema_rs::ContentMode::{v:?}"))
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

#[cfg(test)]
mod tests {
    use super::{EmbeddedSet, FileRebase, OwnedDocument, embedding};
    use crate::validator::EmbeddedDocumentKind;
    use std::sync::Arc;

    fn rebased(uris: &[&str], uri: &str) -> String {
        FileRebase::new(uris.iter().copied()).apply(uri)
    }

    #[test]
    fn rebase_keeps_the_path_below_the_common_directory() {
        let uris: [&str; 3] = [
            "file:///home/ada/api/a.json",
            "file:///home/ada/api/sub/b.json",
            "https://example.com/c.json",
        ];
        let expected: [String; 3] = [
            "file:///a.json".to_string(),
            "file:///sub/b.json#/$defs/x".to_string(),
            "https://example.com/c.json".to_string(),
        ];
        let actual: [String; 3] = [
            rebased(&uris, "file:///home/ada/api/a.json"),
            rebased(&uris, "file:///home/ada/api/sub/b.json#/$defs/x"),
            rebased(&uris, "https://example.com/c.json"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn rebase_common_directory_ends_at_a_segment_boundary() {
        let actual: String = rebased(
            &["file:///data/ab/x.json", "file:///data/ac/y.json"],
            "file:///data/ab/x.json",
        );
        let expected: String = "file:///ab/x.json".to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn rebase_drops_a_windows_drive_path() {
        let actual: String = rebased(
            &["file:///C:/Users/ada/s.json"],
            "file:///C:/Users/ada/s.json#",
        );
        let expected: String = "file:///s.json#".to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn rebase_without_file_documents_keeps_every_uri() {
        let actual: String = rebased(&["json-schema:///"], "json-schema:///#/a");
        let expected: String = "json-schema:///#/a".to_string();
        assert_eq!(expected, actual);
    }

    fn set(documents: &[(&str, &str)]) -> EmbeddedSet {
        EmbeddedSet {
            settings: None,
            documents: documents
                .iter()
                .map(|(uri, json)| OwnedDocument {
                    uri: (*uri).to_string(),
                    json: (*json).to_string(),
                    kind: EmbeddedDocumentKind::Parsed(None),
                })
                .collect(),
            rebase: Arc::new(FileRebase::new(documents.iter().map(|(uri, _)| *uri))),
            root_bases: documents
                .first()
                .map(|(uri, _)| (*uri).to_string())
                .into_iter()
                .collect(),
        }
    }

    /// The embedded (URI, JSON) pairs and entries of `set`.
    fn embedded(
        set: &EmbeddedSet,
        entries: &[&[&str]],
    ) -> (Vec<(String, String)>, Vec<Vec<String>>) {
        let entries: Vec<Vec<String>> = entries
            .iter()
            .map(|uris| uris.iter().map(|u| (*u).to_string()).collect())
            .collect();
        let (documents, entries) = embedding(set, &entries).expect("embeds");
        (
            documents.into_iter().map(|d| (d.uri, d.json)).collect(),
            entries,
        )
    }

    #[test]
    fn pruned_documents_that_validate_differently_are_embedded_whole() {
        // Draft 7 ignores `$id` next to `$ref`; the path to `C` keeps `$id` but not `$ref`, which
        // would make `A` a resource of its own and move `C`'s canonical URI.
        let json: &str = r##"{"$schema":"http://json-schema.org/draft-07/schema#","$id":"https://example.com/root.json","definitions":{"A":{"$id":"https://example.com/a.json","$ref":"#/definitions/B","definitions":{"C":{"type":"string","title":"C"}}},"B":{}}}"##;
        let actual = embedded(
            &set(&[("https://example.com/root.json", json)]),
            &[&["https://example.com/root.json#/definitions/A/definitions/C"]],
        );
        let expected = (
            vec![(
                "https://example.com/root.json".to_string(),
                json.to_string(),
            )],
            vec![vec!["#/definitions/A/definitions/C".to_string()]],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn entries_outside_the_root_resource_keep_their_full_uri() {
        let a: &str = r#"{"$ref":"b.json"}"#;
        let b: &str = r#"{"type":"string"}"#;
        let actual = embedded(
            &set(&[
                ("https://example.com/a.json", a),
                ("https://example.com/b.json", b),
            ]),
            &[
                &["https://example.com/a.json#"],
                &["https://example.com/b.json#"],
            ],
        );
        let expected = (
            vec![
                ("https://example.com/a.json".to_string(), a.to_string()),
                ("https://example.com/b.json".to_string(), b.to_string()),
            ],
            vec![
                vec!["#".to_string()],
                vec!["https://example.com/b.json#".to_string()],
            ],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn documents_naming_an_absolute_file_uri_keep_their_uris_as_read() {
        let a: &str = r#"{"$ref":"file:///abs/dir/b.json"}"#;
        let b: &str = r#"{"type":"string"}"#;
        let actual = embedded(
            &set(&[("file:///abs/dir/a.json", a), ("file:///abs/dir/b.json", b)]),
            &[&["file:///abs/dir/a.json#"]],
        );
        let expected = (
            vec![
                ("file:///abs/dir/a.json".to_string(), a.to_string()),
                ("file:///abs/dir/b.json".to_string(), b.to_string()),
            ],
            vec![vec!["file:///abs/dir/a.json#".to_string()]],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn relative_references_between_files_embed_rebased_and_pruned() {
        let a: &str = r#"{"title":"A","$ref":"sub/b.json"}"#;
        let b: &str = r#"{"description":"B","type":"string"}"#;
        let actual = embedded(
            &set(&[
                ("file:///abs/dir/a.json", a),
                ("file:///abs/dir/sub/b.json", b),
            ]),
            &[&["file:///abs/dir/a.json#"]],
        );
        let expected = (
            vec![
                (
                    "file:///a.json".to_string(),
                    r#"{"$ref":"sub/b.json"}"#.to_string(),
                ),
                (
                    "file:///sub/b.json".to_string(),
                    r#"{"type":"string"}"#.to_string(),
                ),
            ],
            vec![vec!["#".to_string()]],
        );
        assert_eq!(expected, actual);
    }
}
