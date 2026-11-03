//! Pruning an embedded schema document to what validation reads (see design.md,
//! *Self-validating types*).
//!
//! The compiler reports every position its nodes were built from (each subschema, `$ref` target
//! and entry). The pruned document keeps, at each such position, every keyword that can decide
//! validity, and on the path from the root to it only what scopes and dialects depend on
//! (`$schema`, `$id`, `id`, `$vocabulary`). It drops annotations (`title`, `description`,
//! `examples`, ...), unknown keywords and the `$defs` no compiled schema reaches. Removing an
//! object member never moves another position, so every JSON Pointer (and with it every error
//! location) stays the same; an array element is replaced by `{}`, never removed. The caller
//! proves the result by compiling it and comparing the compiled form with the unpruned one
//! (`CompiledSchema::same_validation`), and embeds the unpruned documents when they differ.

use crate::json_schema::{Dialect, SpecVersion};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Every draft whose keywords a compiled schema object may hold.
const DRAFTS: [SpecVersion; 10] = [
    SpecVersion::Draft00,
    SpecVersion::Draft01,
    SpecVersion::Draft02,
    SpecVersion::Draft03,
    SpecVersion::Draft04,
    SpecVersion::Draft05,
    SpecVersion::Draft06,
    SpecVersion::Draft07,
    SpecVersion::Draft201909,
    SpecVersion::Draft202012,
];

/// Keywords no draft's validation reads (annotations), and the definition containers, whose
/// entries the compiler reaches through `$ref` positions instead.
const NEVER_ASSERTED: [&str; 13] = [
    "title",
    "description",
    "$comment",
    "examples",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    "contentEncoding",
    "contentMediaType",
    "contentSchema",
    "$defs",
    "definitions",
];

/// What scopes and dialects depend on: kept on every object on the path to a compiled position.
const IDENTITY: [&str; 4] = ["$schema", "$id", "id", "$vocabulary"];

/// How a value on the path to a compiled position is pruned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// A container a compiled schema's keyword holds (`properties`, `allOf`, ...): every
    /// member stays.
    Keep,
    /// Anything else (`$defs`, an unknown keyword's value, a schema no node was built from):
    /// only the path and the identity keywords stay.
    Prune,
}

/// Whether `key` may decide validity in a schema object of some draft of `dialects`.
fn may_assert(dialects: &[Dialect], key: &str) -> bool {
    !NEVER_ASSERTED.contains(&key) && dialects.iter().any(|d| d.keyword(key).is_some())
}

/// The compiled positions as a tree of JSON Pointer segments.
#[derive(Debug, Default)]
struct Trie<'k> {
    /// Whether a compiled node was built from this position.
    kept: bool,
    children: HashMap<&'k str, Trie<'k>>,
}

impl<'k> Trie<'k> {
    fn new(kept: &[&'k [String]]) -> Trie<'k> {
        let mut root: Trie<'k> = Trie::default();
        for pointer in kept {
            let mut node: &mut Trie<'k> = &mut root;
            for segment in *pointer {
                node = node.children.entry(segment.as_str()).or_default();
            }
            node.kept = true;
        }
        root
    }
}

/// Prunes `document` to the compiled positions `kept` (JSON Pointer segments from its root), in
/// one walk of the document guided by the positions' tree.
pub(crate) fn prune(document: &mut Value, kept: &[&[String]]) {
    let dialects: Vec<Dialect> = DRAFTS.into_iter().map(Dialect::for_spec_version).collect();
    let may_assert = |key: &str| -> bool { may_assert(&dialects, key) };
    let trie: Trie<'_> = Trie::new(kept);
    let mut stack: Vec<(&mut Value, &Trie<'_>, Mode)> = vec![(document, &trie, Mode::Prune)];
    while let Some((value, node, mode)) = stack.pop() {
        match value {
            Value::Object(map) => {
                map.retain(|key, _| {
                    node.children.contains_key(key.as_str())
                        || if node.kept {
                            may_assert(key)
                        } else {
                            mode == Mode::Keep || IDENTITY.contains(&key.as_str())
                        }
                });
                for (key, child) in map.iter_mut() {
                    let Some(next) = node.children.get(key.as_str()) else {
                        continue;
                    };
                    let next_mode: Mode =
                        if (node.kept && may_assert(key)) || (!node.kept && mode == Mode::Keep) {
                            Mode::Keep
                        } else {
                            Mode::Prune
                        };
                    stack.push((child, next, next_mode));
                }
            }
            Value::Array(items) => {
                for (index, item) in items.iter_mut().enumerate() {
                    match node.children.get(index.to_string().as_str()) {
                        Some(next) => stack.push((item, next, mode)),
                        None if mode == Mode::Prune => *item = Value::Object(Map::new()),
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::prune;
    use serde_json::{Value, json};

    fn pruned(mut document: Value, kept: &[&[&str]]) -> Value {
        let kept: Vec<Vec<String>> = kept
            .iter()
            .map(|p| p.iter().map(|s| (*s).to_string()).collect())
            .collect();
        let kept: Vec<&[String]> = kept.iter().map(Vec::as_slice).collect();
        prune(&mut document, &kept);
        document
    }

    #[test]
    fn compiled_root_drops_annotations_and_unused_defs() {
        let actual: Value = pruned(
            json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "title": "Root",
                "description": "A root",
                "type": "object",
                "required": ["a"],
                "properties": {"a": {"type": "string", "examples": ["x"]}},
                "$defs": {"Unused": {"type": "integer"}},
                "x-vendor": {"big": true}
            }),
            &[&[], &["properties", "a"]],
        );
        let expected: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["a"],
            "properties": {"a": {"type": "string"}}
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn uncompiled_root_keeps_only_identity_on_the_path_to_a_used_def() {
        let actual: Value = pruned(
            json!({
                "$id": "https://example.com/s.json",
                "type": "object",
                "properties": {"big": {"type": "string"}},
                "$defs": {
                    "Pet": {"type": "object", "title": "Pet", "minProperties": 1},
                    "Other": {"type": "string"}
                }
            }),
            &[&["$defs", "Pet"]],
        );
        let expected: Value = json!({
            "$id": "https://example.com/s.json",
            "$defs": {"Pet": {"type": "object", "minProperties": 1}}
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn nothing_compiled_keeps_only_identity() {
        let actual: Value = pruned(
            json!({
                "$schema": "https://example.com/meta",
                "$vocabulary": {"https://json-schema.org/draft/2020-12/vocab/core": true},
                "title": "Meta",
                "type": "object"
            }),
            &[],
        );
        let expected: Value = json!({
            "$schema": "https://example.com/meta",
            "$vocabulary": {"https://json-schema.org/draft/2020-12/vocab/core": true}
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn keyword_containers_of_compiled_schemas_keep_every_member() {
        let actual: Value = pruned(
            json!({
                "dependencies": {"a": ["b"], "c": {"required": ["d"], "title": "C"}},
                "allOf": [{"minimum": 1}, {"maximum": 5, "description": "max"}]
            }),
            &[
                &[],
                &["dependencies", "c"],
                &["allOf", "0"],
                &["allOf", "1"],
            ],
        );
        let expected: Value = json!({
            "dependencies": {"a": ["b"], "c": {"required": ["d"]}},
            "allOf": [{"minimum": 1}, {"maximum": 5}]
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn array_elements_off_the_path_become_empty_schemas() {
        let actual: Value = pruned(
            json!({"x-list": [{"type": "string"}, {"type": "integer"}]}),
            &[&["x-list", "1"]],
        );
        let expected: Value = json!({"x-list": [{}, {"type": "integer"}]});
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_target_inside_an_annotation_keeps_its_path() {
        let actual: Value = pruned(
            json!({"$ref": "#/default/s", "default": {"s": {"type": "string"}, "t": 1}}),
            &[&[], &["default", "s"]],
        );
        let expected: Value = json!({"$ref": "#/default/s", "default": {"s": {"type": "string"}}});
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_schema_at_a_compiled_position_stays() {
        let actual: Value = pruned(
            json!({"properties": {"a": false}, "additionalProperties": true}),
            &[&[], &["properties", "a"], &["additionalProperties"]],
        );
        let expected: Value = json!({"properties": {"a": false}, "additionalProperties": true});
        assert_eq!(expected, actual);
    }
}
