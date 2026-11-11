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
use super::type_graph::{TypeGraph, TypeId, TypeNode, UnionKind, ValidationTarget};
use super::{CodeGenError, CodeGenResult};
use crate::json_schema::{
    DialectSettings, JsonSchemaSettings, Registry, RegistryDocumentKind, SchemaRef,
};
use crate::validator::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
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
            Ok(EmbeddedSet {
                settings,
                documents,
                rebase: Arc::clone(&rebase),
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
/// embedded: rebased and pruned, else rebased, else as read.
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
    let same = |documents: &[OwnedDocument]| -> bool {
        compile(set.settings, documents, &rebased_entries, |v| v.compile()).is_ok_and(
            |(compiled, compiled_entries)| {
                compiled_entries == reference_entries
                    && reference.same_validation(&compiled, |uri| rebase(uri))
            },
        )
    };
    if let Some(pruned) = pruned.filter(|pruned| same(pruned)) {
        return Ok((pruned, rebased_entries));
    }
    log::debug!(
        "json-schema-rs: the pruned embedded documents validate differently; embedding them whole"
    );
    if same(&rebased) {
        return Ok((rebased, rebased_entries));
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

/// `validate` and `TryFrom<serde_json::Value>` for the item `name`.
pub(crate) fn render_item_impls(name: &str, entry: &Entry) -> String {
    let Entry { validators, index } = entry;
    format!(
        "impl {name} {{
    /// Validates this value against its schema.
    ///
    /// # Errors
    ///
    /// Every error the schema's validator reports, when this value does not satisfy the schema.
    pub fn validate(&self) -> Result<(), json_schema_rs::TypeValidationError> {{
        {validators}.validate_serialized({index}, self)
    }}
}}

impl TryFrom<serde_json::Value> for {name} {{
    type Error = json_schema_rs::TypeValidationError;

    /// Validates `value` against the schema, then deserializes it.
    fn try_from(value: serde_json::Value) -> Result<Self, json_schema_rs::TypeValidationError> {{
        {validators}.deserialize({index}, value)
    }}
}}

"
    )
}

/// One variant of a generated union.
pub(crate) struct UnionVariant {
    pub(crate) name: String,
    /// Its Rust type.
    pub(crate) ty: String,
    /// It reads through `json_schema_rs::integer` (an integer type, or a container of them).
    pub(crate) integers: bool,
}

/// The `Deserialize` of an `anyOf` / `oneOf` union: one call to the runtime's
/// `deserialize_union`, which buffers the value once and takes the first variant whose branch the
/// value satisfies (for `oneOf`, the one) and that deserializes; the closure reads the variant at
/// a position.
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
    let read = |UnionVariant {
                    name: variant,
                    ty,
                    integers,
                }: &UnionVariant|
     -> String {
        if *integers {
            format!("json_schema_rs::integer::deserialize::<_, {ty}>(value).map(Self::{variant})")
        } else {
            format!("<{ty} as Deserialize>::deserialize(value).map(Self::{variant})")
        }
    };
    let mut out: String = String::new();
    let _ = writeln!(out, "impl<'de> Deserialize<'de> for {name} {{");
    out.push_str(
        "    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {\n",
    );
    let _ = writeln!(
        out,
        "        {}.deserialize_union(\n            deserializer,\n            \"{name}\",\n            json_schema_rs::UnionMatch::{mode},\n            {union},\n            &[{}],",
        entries.validators,
        indexes.join(", ")
    );
    if let [only] = variants {
        let _ = writeln!(
            out,
            "            |_: usize, value: &serde_json::Value| {},",
            read(only)
        );
    } else {
        out.push_str("            |variant: usize, value: &serde_json::Value| match variant {\n");
        for (i, variant) in variants.iter().enumerate() {
            let arm: String = if i + 1 == variants.len() {
                "_".to_string()
            } else {
                i.to_string()
            };
            let _ = writeln!(out, "                {arm} => {},", read(variant));
        }
        out.push_str("            },\n");
    }
    out.push_str("        )\n    }\n}\n\n");
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
            vec![vec![
                "https://example.com/root.json#/definitions/A/definitions/C".to_string(),
            ]],
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
            vec![vec!["file:///a.json#".to_string()]],
        );
        assert_eq!(expected, actual);
    }
}
