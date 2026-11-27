//! Converting a schema built as 2020-12 into the target dialect.
//!
//! The generator builds every document as 2020-12. For any other target one pass walks the
//! document (an explicit stack, so depth cannot overflow) and rewrites each schema object in
//! place. Every rewrite is exact: a construct the target cannot express is reported as a
//! [`ReverseCodeGenError::UnrepresentableKeyword`] and left as it was, never approximated.
//! Subschemas are found through the 2020-12 keyword table, so hand-written schemas that reach
//! the generator are converted too.

use super::error::ReverseCodeGenError;
use super::settings::TargetDialect;
use super::{RefNode, push_raw_subschemas};
use crate::json_schema::{Dialect, JsonSchema, Schema, SpecVersion, SubschemaPosition};
use crate::number::{cmp_num, json_equal};
use serde_json::{Map, Number, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// The keywords of an `OpenAPI` 3.0 Schema Object that this converter writes or keeps: draft-04's
/// subset plus `nullable`, `readOnly`, `writeOnly`, `example` and `deprecated`.
const OPENAPI_30_KEYWORDS: &[&str] = &[
    "$ref",
    "additionalProperties",
    "allOf",
    "anyOf",
    "default",
    "deprecated",
    "description",
    "enum",
    "exclusiveMaximum",
    "exclusiveMinimum",
    "format",
    "items",
    "maxItems",
    "maxLength",
    "maxProperties",
    "maximum",
    "minItems",
    "minLength",
    "minProperties",
    "minimum",
    "multipleOf",
    "not",
    "nullable",
    "oneOf",
    "pattern",
    "properties",
    "readOnly",
    "required",
    "title",
    "type",
    "uniqueItems",
    "writeOnly",
];

/// Keywords that only annotate: a draft that does not define them ignores them without losing
/// any assertion, so drafts 4 to 7 keep them as unknown keywords.
const ANNOTATIONS: &[&str] = &[
    "$comment",
    "contentEncoding",
    "contentMediaType",
    "deprecated",
    "examples",
    "readOnly",
    "writeOnly",
];

/// Whether `target` defines `keyword`: it is in the target draft's keyword table, or in the
/// `OpenAPI` 3.0 Schema Object's.
pub(super) fn defines(target: TargetDialect, keyword: &str) -> bool {
    match target.spec_version() {
        Some(version) => Dialect::for_spec_version(version)
            .keyword(keyword)
            .is_some(),
        None => OPENAPI_30_KEYWORDS.contains(&keyword),
    }
}

/// Prefix of the key under which a draft-4 `exclusiveMinimum` / `exclusiveMaximum` boolean waits
/// while the converted document is read back into the typed model, whose fields for those
/// keywords hold numbers. [`restore_boolean_bounds`] puts the real key back, in the keywords the
/// model keeps verbatim.
const BOOLEAN_BOUND_MARKER: &str = "\u{1}boolean ";

/// A schema the walk still has to convert.
struct Work<'a> {
    node: &'a mut Value,
    /// Index of the node's JSON Pointer in [`Converter::pointers`].
    at: usize,
    /// Whether the position accepts a boolean schema in every dialect
    /// (`additionalProperties`, `additionalItems`).
    boolean_ok: bool,
    /// Whether this node's own keywords were converted already (a branch split off a multi-type
    /// schema), so only its subschemas remain.
    converted: bool,
    /// Whether this is the document root, whose `$schema` the generator sets itself.
    root: bool,
}

/// Converts schemas built as 2020-12 into `target`, collecting every keyword it cannot express.
pub(super) struct Converter<'a> {
    target: TargetDialect,
    /// `#` and the definitions path and a slash: the prefix of the references to the document's
    /// definitions, which are written final and must not be rewritten.
    document_prefix: String,
    /// The JSON Pointer of every schema reached, as its parent's index and its last token (the
    /// first entry holds the whole pointer of the schema the walk started at). A pointer is
    /// built only when an error names it, so the walk costs no string per level of nesting.
    pointers: Vec<(usize, String)>,
    errors: &'a mut Vec<ReverseCodeGenError>,
}

impl<'a> Converter<'a> {
    pub(super) fn new(
        target: TargetDialect,
        definitions_path: &str,
        errors: &'a mut Vec<ReverseCodeGenError>,
    ) -> Self {
        Converter {
            target,
            document_prefix: format!("#{definitions_path}/"),
            pointers: Vec::new(),
            errors,
        }
    }

    /// Converts one schema, located at `pointer` in the generated document. `root` marks the
    /// document root.
    pub(super) fn convert(&mut self, schema: Schema, pointer: &str, root: bool) -> Schema {
        let Ok(mut value) = serde_json::to_value(&schema) else {
            unreachable!("a schema serializes to JSON");
        };
        self.convert_value(&mut value, pointer, root);
        match serde_json::from_value::<Schema>(value) {
            Ok(mut converted) => {
                if let Schema::Object(object) = &mut converted {
                    restore_boolean_bounds(object);
                }
                converted
            }
            Err(error) => {
                self.errors.push(ReverseCodeGenError::InvalidSchema {
                    json: pointer.to_string(),
                    reason: error.to_string(),
                });
                schema
            }
        }
    }

    /// [`Converter::convert`] on JSON.
    fn convert_value(&mut self, value: &mut Value, pointer: &str, root: bool) {
        self.pointers.clear();
        self.pointers.push((0, pointer.to_string()));
        let mut stack: Vec<Work<'_>> = vec![Work {
            node: value,
            at: 0,
            boolean_ok: false,
            converted: false,
            root,
        }];
        while let Some(work) = stack.pop() {
            let Work {
                node,
                at,
                boolean_ok,
                converted,
                root,
            } = work;
            if let Value::Bool(value) = node {
                // Draft 4 and OpenAPI 3.0 have no boolean schemas, except where a keyword takes
                // a boolean in place of a schema.
                if !boolean_ok && self.no_boolean_schemas() {
                    *node = if *value {
                        Value::Object(Map::new())
                    } else {
                        let mut not: Map<String, Value> = Map::new();
                        not.insert("not".to_string(), Value::Object(Map::new()));
                        Value::Object(not)
                    };
                }
                continue;
            }
            let Value::Object(map) = node else {
                continue;
            };
            let split: bool = !converted && self.convert_keywords(map, at, root);
            if split {
                if let Some(Value::Array(branches)) = map.get_mut("anyOf") {
                    let any_of: usize = self.child(at, "anyOf");
                    for (index, branch) in branches.iter_mut().enumerate() {
                        stack.push(Work {
                            node: branch,
                            at: self.child(any_of, &index.to_string()),
                            boolean_ok: false,
                            converted: true,
                            root: false,
                        });
                    }
                }
            } else {
                push_subschemas(map, at, &mut self.pointers, &mut stack);
            }
        }
    }

    fn no_boolean_schemas(&self) -> bool {
        matches!(
            self.target,
            TargetDialect::Draft04 | TargetDialect::OpenApi30
        )
    }

    /// The pointer entry of the child of `parent` at `token` (unescaped).
    fn child(&mut self, parent: usize, token: &str) -> usize {
        self.pointers.push((parent, escape(token)));
        self.pointers.len() - 1
    }

    /// The JSON Pointer of the entry `at`.
    fn pointer(&self, at: usize) -> String {
        let mut tokens: Vec<&str> = Vec::new();
        let mut current: usize = at;
        while current != 0 {
            let (parent, token): &(usize, String) = &self.pointers[current];
            tokens.push(token);
            current = *parent;
        }
        let mut pointer: String = self.pointers[0].1.clone();
        for token in tokens.into_iter().rev() {
            pointer.push('/');
            pointer.push_str(token);
        }
        pointer
    }

    fn report(&mut self, keyword: &str, at: usize) {
        let pointer: String = self.pointer(at);
        self.errors
            .push(ReverseCodeGenError::UnrepresentableKeyword {
                dialect: self.target,
                keyword: keyword.to_string(),
                pointer,
            });
    }

    /// Whether the target defines `keyword`.
    fn target_defines(&self, keyword: &str) -> bool {
        defines(self.target, keyword)
    }

    /// Rewrites the keywords of one schema object. Returns whether it was split into an `anyOf`
    /// of single-type branches (`OpenAPI` 3.0).
    #[expect(
        clippy::too_many_lines,
        reason = "one block per construct, in the order the table in design.md lists them"
    )]
    fn convert_keywords(&mut self, map: &mut Map<String, Value>, at: usize, root: bool) -> bool {
        let target: TargetDialect = self.target;
        let mut reported: BTreeSet<String> = BTreeSet::new();
        let is_draft_4_to_7: bool = matches!(
            target,
            TargetDialect::Draft04 | TargetDialect::Draft06 | TargetDialect::Draft07
        );
        let is_openapi_30: bool = target == TargetDialect::OpenApi30;
        let ignores_ref_siblings: bool = is_draft_4_to_7 || is_openapi_30;

        // `$schema` marks a schema resource of another dialect; only 2019-09 may embed one.
        if let Some(value) = map.get("$schema") {
            let allowed: bool = root
                || match target {
                    TargetDialect::Draft201909 => true,
                    TargetDialect::Draft04 | TargetDialect::Draft06 | TargetDialect::Draft07 => {
                        value.as_str().and_then(SpecVersion::from_schema_uri)
                            == target.spec_version()
                    }
                    _ => false,
                };
            if !allowed {
                self.report("$schema", at);
            }
            reported.insert("$schema".to_string());
        }

        // Definitions live in `definitions` before 2019-09.
        if is_draft_4_to_7 {
            if let Some(Value::String(reference)) = map.get_mut("$ref")
                && reference.starts_with("#/$defs/")
                && !reference.starts_with(&self.document_prefix)
            {
                *reference = format!("#/definitions/{}", &reference["#/$defs/".len()..]);
            }
            if let Some(defs) = map.remove("$defs") {
                match (map.get_mut("definitions"), defs) {
                    (None, defs) => {
                        map.insert("definitions".to_string(), defs);
                    }
                    (Some(Value::Object(existing)), Value::Object(defs))
                        if defs.keys().all(|key| !existing.contains_key(key)) =>
                    {
                        existing.extend(defs);
                    }
                    (Some(_), defs) => {
                        map.insert("$defs".to_string(), defs);
                        self.report("$defs", at);
                        reported.insert("$defs".to_string());
                    }
                }
            }
        } else if is_openapi_30 && map.contains_key("$defs") {
            self.report("$defs", at);
            reported.insert("$defs".to_string());
        }

        // `$ref` siblings are ignored before 2019-09: the reference goes into an `allOf`.
        if ignores_ref_siblings
            && map.len() > 1
            && let Some(reference) = map.remove("$ref")
        {
            let mut wrapped: Map<String, Value> = Map::new();
            wrapped.insert("$ref".to_string(), reference);
            match map.get_mut("allOf") {
                Some(Value::Array(items)) => items.push(Value::Object(wrapped)),
                Some(_) => {
                    // A malformed `allOf` has no place for the reference.
                    map.insert(
                        "$ref".to_string(),
                        wrapped.remove("$ref").unwrap_or_default(),
                    );
                    self.report("allOf", at);
                    reported.insert("allOf".to_string());
                }
                None => {
                    map.insert(
                        "allOf".to_string(),
                        Value::Array(vec![Value::Object(wrapped)]),
                    );
                }
            }
        }

        // Draft 4 spells `$id` as `id`.
        if target == TargetDialect::Draft04
            && let Some(id) = map.remove("$id")
        {
            if map.contains_key("id") {
                map.insert("$id".to_string(), id);
                self.report("$id", at);
                reported.insert("$id".to_string());
            } else {
                map.insert("id".to_string(), id);
            }
        }

        // 2020-12's `prefixItems` is the array form of `items` before it, and its `items`
        // is `additionalItems`.
        if target != TargetDialect::Draft202012
            && target != TargetDialect::OpenApi31
            && let Some(prefix) = map.remove("prefixItems")
        {
            if is_openapi_30 {
                map.insert("prefixItems".to_string(), prefix);
                self.report("prefixItems", at);
                reported.insert("prefixItems".to_string());
            } else {
                let rest: Option<Value> = map.remove("items");
                map.insert("items".to_string(), prefix);
                if let Some(rest) = rest {
                    map.insert("additionalItems".to_string(), rest);
                }
            }
        }
        if is_openapi_30 && matches!(map.get("items"), Some(Value::Array(_))) {
            self.report("items", at);
            reported.insert("items".to_string());
        }

        if target == TargetDialect::Draft04 || is_openapi_30 {
            Self::convert_const(map);
            convert_bound(map, "minimum", "exclusiveMinimum", Ordering::Greater);
            convert_bound(map, "maximum", "exclusiveMaximum", Ordering::Less);
        }

        if is_draft_4_to_7 {
            convert_dependent(map);
        }

        if is_openapi_30 && let Some(Value::Array(examples)) = map.get("examples") {
            if examples.len() == 1 && !map.contains_key("example") {
                if let Some(Value::Array(mut examples)) = map.remove("examples") {
                    let example: Value = examples.remove(0);
                    map.insert("example".to_string(), example);
                }
            } else {
                self.report("examples", at);
            }
            reported.insert("examples".to_string());
        }

        // Every other keyword 2020-12 defines must exist in the target, or only annotate.
        let source: Dialect = Dialect::default();
        let missing: Vec<String> = map
            .keys()
            .filter(|key| {
                let annotation: bool = is_draft_4_to_7 && ANNOTATIONS.contains(&key.as_str());
                !reported.contains(key.as_str())
                    && source.keyword(key).is_some()
                    && !self.target_defines(key)
                    && !annotation
            })
            .cloned()
            .collect();
        for keyword in missing {
            self.report(&keyword, at);
        }

        if is_openapi_30 {
            return self.convert_type(map, at);
        }
        false
    }

    /// `const` is a one-value `enum` in a dialect without it. With an `enum` too, the value
    /// must be one of its members and the `enum` shrinks to it.
    fn convert_const(map: &mut Map<String, Value>) {
        let Some(value) = map.remove("const") else {
            return;
        };
        let members: Vec<Value> = match map.get("enum") {
            Some(Value::Array(members)) => {
                if members.iter().any(|member| json_equal(member, &value)) {
                    vec![value]
                } else {
                    Vec::new()
                }
            }
            _ => vec![value],
        };
        map.insert("enum".to_string(), Value::Array(members));
    }

    /// `OpenAPI` 3.0 has no `null` type: `null` among the types becomes `nullable: true`, and
    /// several types become an `anyOf` of one single-type schema each. Returns whether the
    /// schema was split.
    fn convert_type(&mut self, map: &mut Map<String, Value>, at: usize) -> bool {
        let types: Vec<String> = match map.get("type") {
            Some(Value::Array(types)) => types
                .iter()
                .filter_map(|ty| ty.as_str().map(str::to_string))
                .collect(),
            Some(Value::String(ty)) => vec![ty.clone()],
            _ => return false,
        };
        let nullable: bool = types.iter().any(|ty| ty == "null");
        let others: Vec<&String> = types.iter().filter(|ty| *ty != "null").collect();
        match others.as_slice() {
            [] => {
                self.report("type", at);
                false
            }
            [single] => {
                map.insert("type".to_string(), Value::String((*single).clone()));
                if nullable {
                    map.insert("nullable".to_string(), Value::Bool(true));
                }
                false
            }
            several => {
                let branches: Vec<Value> = several
                    .iter()
                    .map(|ty| {
                        let mut branch: Map<String, Value> = map.clone();
                        branch.insert("type".to_string(), Value::String((*ty).clone()));
                        if nullable {
                            branch.insert("nullable".to_string(), Value::Bool(true));
                        }
                        Value::Object(branch)
                    })
                    .collect();
                map.clear();
                map.insert("anyOf".to_string(), Value::Array(branches));
                true
            }
        }
    }
}

/// Draft 4's `exclusiveMinimum` / `exclusiveMaximum` is a boolean that makes `minimum` /
/// `maximum` exclusive. A numeric exclusive bound `e` and an inclusive bound `b` together keep
/// the tighter: `b` alone when it is `tighter` than `e` (strictly), else `e` exclusive.
fn convert_bound(
    map: &mut Map<String, Value>,
    inclusive: &str,
    exclusive: &str,
    tighter: Ordering,
) {
    let Some(Value::Number(bound)) = map.get(exclusive) else {
        return;
    };
    let bound: Number = bound.clone();
    map.remove(exclusive);
    if let Some(Value::Number(existing)) = map.get(inclusive)
        && cmp_num(existing, &bound) == tighter
    {
        return;
    }
    map.insert(inclusive.to_string(), Value::Number(bound));
    map.insert(
        format!("{BOOLEAN_BOUND_MARKER}{exclusive}"),
        Value::Bool(true),
    );
}

/// Puts the keyword back under every [`BOOLEAN_BOUND_MARKER`] key, in `schema` and in every
/// subschema the model keeps as raw JSON. Iterative, so depth cannot overflow the stack.
fn restore_boolean_bounds(schema: &mut JsonSchema) {
    let source: Dialect = Dialect::default();
    let mut stack: Vec<RefNode<'_>> = vec![RefNode::Typed(schema)];
    while let Some(node) = stack.pop() {
        match node {
            RefNode::Typed(current) => {
                let marked: Vec<String> = current
                    .extensions
                    .keys()
                    .filter(|key| key.starts_with(BOOLEAN_BOUND_MARKER))
                    .cloned()
                    .collect();
                for key in marked {
                    if let Some(value) = current.extensions.remove(&key) {
                        current
                            .extensions
                            .insert(key[BOOLEAN_BOUND_MARKER.len()..].to_string(), value);
                    }
                }
                let (subschemas, extensions) = current.subschemas_mut();
                for (keyword, value) in extensions {
                    push_raw_subschemas(source, keyword, value, &mut stack);
                }
                stack.extend(subschemas.into_iter().map(RefNode::Typed));
            }
            RefNode::Raw(value) => {
                let Value::Object(object) = value else {
                    continue;
                };
                let marked: Vec<String> = object
                    .keys()
                    .filter(|key| key.starts_with(BOOLEAN_BOUND_MARKER))
                    .cloned()
                    .collect();
                for key in marked {
                    if let Some(value) = object.remove(&key) {
                        object.insert(key[BOOLEAN_BOUND_MARKER.len()..].to_string(), value);
                    }
                }
                for (keyword, value) in object.iter_mut() {
                    push_raw_subschemas(source, keyword, value, &mut stack);
                }
            }
        }
    }
}

/// `dependentRequired` and `dependentSchemas` are `dependencies` before 2019-09. A property that
/// both name (or that `dependencies` already names) needs both: its entries become one
/// `allOf` of schemas.
fn convert_dependent(map: &mut Map<String, Value>) {
    let required: Option<Value> = map.remove("dependentRequired");
    let schemas: Option<Value> = map.remove("dependentSchemas");
    if required.is_none() && schemas.is_none() {
        return;
    }
    let mut dependencies: Map<String, Value> = match map.remove("dependencies") {
        Some(Value::Object(existing)) => existing,
        _ => Map::new(),
    };
    let mut parts: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for entries in [required, schemas].into_iter().flatten() {
        if let Value::Object(entries) = entries {
            for (property, entry) in entries {
                parts.entry(property).or_default().push(entry);
            }
        }
    }
    for (property, mut entries) in parts {
        if let Some(existing) = dependencies.remove(&property) {
            entries.insert(0, existing);
        }
        let merged: Value = if entries.len() == 1 {
            entries.remove(0)
        } else {
            let all_of: Vec<Value> = entries.into_iter().map(as_schema).collect();
            let mut merged: Map<String, Value> = Map::new();
            merged.insert("allOf".to_string(), Value::Array(all_of));
            Value::Object(merged)
        };
        dependencies.insert(property, merged);
    }
    map.insert("dependencies".to_string(), Value::Object(dependencies));
}

/// A `dependencies` entry as a schema: a list of property names is `{"required": [...]}`.
fn as_schema(entry: Value) -> Value {
    if let Value::Array(_) = entry {
        let mut schema: Map<String, Value> = Map::new();
        schema.insert("required".to_string(), entry);
        Value::Object(schema)
    } else {
        entry
    }
}

/// Pushes the schemas `map`'s keywords hold, per the 2020-12 keyword table. `at` is `map`'s own
/// entry in `pointers`; each pushed schema gets an entry of its own.
fn push_subschemas<'a>(
    map: &'a mut Map<String, Value>,
    at: usize,
    pointers: &mut Vec<(usize, String)>,
    stack: &mut Vec<Work<'a>>,
) {
    let source: Dialect = Dialect::default();
    let mut child = |parent: usize, token: &str| -> usize {
        pointers.push((parent, escape(token)));
        pointers.len() - 1
    };
    for (key, value) in map.iter_mut() {
        let Some(position) = source.keyword(key).and_then(|keyword| keyword.subschemas) else {
            continue;
        };
        let boolean_ok: bool = key == "additionalProperties" || key == "additionalItems";
        let holds_list: bool = matches!(
            position,
            SubschemaPosition::Array | SubschemaPosition::SingleOrArray
        ) || key == "items";
        let single: bool = matches!(
            position,
            SubschemaPosition::Single | SubschemaPosition::SingleOrArray
        );
        let is_list: bool = holds_list && matches!(value, Value::Array(_));
        let is_map: bool = position == SubschemaPosition::Map && matches!(value, Value::Object(_));
        if is_list {
            if let Value::Array(items) = value {
                let here: usize = child(at, key);
                for (index, item) in items.iter_mut().enumerate() {
                    stack.push(Work {
                        node: item,
                        at: child(here, &index.to_string()),
                        boolean_ok: false,
                        converted: false,
                        root: false,
                    });
                }
            }
        } else if is_map {
            if let Value::Object(entries) = value {
                let here: usize = child(at, key);
                for (name, entry) in entries.iter_mut() {
                    if matches!(entry, Value::Object(_) | Value::Bool(_)) {
                        stack.push(Work {
                            node: entry,
                            at: child(here, name),
                            boolean_ok: false,
                            converted: false,
                            root: false,
                        });
                    }
                }
            }
        } else if single && matches!(value, Value::Object(_) | Value::Bool(_)) {
            stack.push(Work {
                node: value,
                at: child(at, key),
                boolean_ok,
                converted: false,
                root: false,
            });
        }
    }
}

/// A JSON Pointer reference token (RFC 6901 §3).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `input` converted to `target`, or the keywords it could not express.
    fn converted(target: TargetDialect, input: Value) -> Result<Value, Vec<ReverseCodeGenError>> {
        let mut errors: Vec<ReverseCodeGenError> = Vec::new();
        let mut value: Value = input;
        Converter::new(target, target.default_definitions_path(), &mut errors)
            .convert_value(&mut value, "", false);
        let value: Value = unmarked(value);
        if errors.is_empty() {
            Ok(value)
        } else {
            Err(errors)
        }
    }

    /// `value` with every [`BOOLEAN_BOUND_MARKER`] key (at any depth) renamed to its keyword.
    fn unmarked(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        (
                            key.strip_prefix(BOOLEAN_BOUND_MARKER)
                                .map_or(key.clone(), str::to_string),
                            unmarked(value),
                        )
                    })
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(unmarked).collect()),
            other => other,
        }
    }

    fn unrepresentable(
        dialect: TargetDialect,
        keyword: &str,
        pointer: &str,
    ) -> ReverseCodeGenError {
        ReverseCodeGenError::UnrepresentableKeyword {
            dialect,
            keyword: keyword.to_string(),
            pointer: pointer.to_string(),
        }
    }

    #[test]
    fn defs_become_definitions_with_their_refs_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "properties": {"a": {"$ref": "#/definitions/A"}},
            "definitions": {"A": {"type": "string"}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "properties": {"a": {"$ref": "#/$defs/A"}},
                "$defs": {"A": {"type": "string"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn defs_stay_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "properties": {"a": {"$ref": "#/$defs/A"}},
            "$defs": {"A": {"type": "string"}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({
                "properties": {"a": {"$ref": "#/$defs/A"}},
                "$defs": {"A": {"type": "string"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn defs_colliding_with_definitions_are_unrepresentable() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Err(vec![unrepresentable(TargetDialect::Draft07, "$defs", "")]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({"$defs": {"A": true}, "definitions": {"A": false}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_siblings_move_beside_an_all_of_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "allOf": [{"$ref": "#/definitions/A"}],
            "deprecated": true
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({"$ref": "#/$defs/A", "deprecated": true}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_siblings_join_an_existing_all_of() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "allOf": [{"minimum": 1}, {"$ref": "#/definitions/A"}],
            "title": "T"
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({"$ref": "#/$defs/A", "allOf": [{"minimum": 1}], "title": "T"}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_lone_ref_stays_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"$ref": "#/definitions/A"}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::Draft07, json!({"$ref": "#/$defs/A"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_siblings_stay_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"$ref": "#/$defs/A", "deprecated": true}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({"$ref": "#/$defs/A", "deprecated": true}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn ref_siblings_are_wrapped_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "allOf": [{"$ref": "#/components/schemas/A"}],
            "deprecated": true
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"$ref": "#/components/schemas/A", "deprecated": true}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn id_is_id_in_draft_04_and_dollar_id_in_draft_06() {
        let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            Ok(json!({"id": "http://x/a.json"})),
            Ok(json!({"$id": "http://x/a.json"})),
        ];
        let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            converted(TargetDialect::Draft04, json!({"$id": "http://x/a.json"})),
            converted(TargetDialect::Draft06, json!({"$id": "http://x/a.json"})),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn id_is_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Err(vec![unrepresentable(TargetDialect::OpenApi30, "$id", "")]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::OpenApi30, json!({"$id": "http://x/a.json"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn prefix_items_and_items_become_items_and_additional_items_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "items": [{"type": "string"}, {"type": "integer"}],
            "additionalItems": false
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "prefixItems": [{"type": "string"}, {"type": "integer"}],
                "items": false
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn prefix_items_alone_become_items_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"items": [{"type": "string"}]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({"prefixItems": [{"type": "string"}]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_single_schema_items_stays_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"items": {"type": "string"}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::Draft07, json!({"items": {"type": "string"}}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn prefix_items_are_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "prefixItems",
            "",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"prefixItems": [{"type": "string"}]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unevaluated_properties_are_unrepresentable_before_2019_09() {
        let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            Err(vec![unrepresentable(
                TargetDialect::Draft07,
                "unevaluatedProperties",
                "",
            )]),
            Err(vec![unrepresentable(
                TargetDialect::OpenApi30,
                "unevaluatedProperties",
                "",
            )]),
            Ok(json!({"unevaluatedProperties": false})),
        ];
        let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            converted(
                TargetDialect::Draft07,
                json!({"unevaluatedProperties": false}),
            ),
            converted(
                TargetDialect::OpenApi30,
                json!({"unevaluatedProperties": false}),
            ),
            converted(
                TargetDialect::Draft201909,
                json!({"unevaluatedProperties": false}),
            ),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_is_enum_in_draft_04_and_openapi_30_only() {
        let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            Ok(json!({"enum": [1]})),
            Ok(json!({"enum": [1]})),
            Ok(json!({"const": 1})),
        ];
        let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            converted(TargetDialect::Draft04, json!({"const": 1})),
            converted(TargetDialect::OpenApi30, json!({"const": 1})),
            converted(TargetDialect::Draft06, json!({"const": 1})),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_inside_the_enum_narrows_it_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"enum": [2]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"const": 2, "enum": [1, 2, 3]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_outside_the_enum_leaves_no_value_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"enum": []}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"const": 4, "enum": [1, 2, 3]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_exclusive_bounds_become_boolean_modifiers_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "minimum": 0, "exclusiveMinimum": true,
            "maximum": 10, "exclusiveMaximum": true
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"exclusiveMinimum": 0, "exclusiveMaximum": 10}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_tighter_inclusive_bound_wins_over_the_exclusive_one_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"minimum": 5, "maximum": 5}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"minimum": 5, "exclusiveMinimum": 1, "maximum": 5, "exclusiveMaximum": 9}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_tighter_or_equal_exclusive_bound_wins_over_the_inclusive_one_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "minimum": 5, "exclusiveMinimum": true,
            "maximum": 5, "exclusiveMaximum": true
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"minimum": 5, "exclusiveMinimum": 5, "maximum": 5, "exclusiveMaximum": 5}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_exclusive_bounds_stay_in_draft_06() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"exclusiveMinimum": 0}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::Draft06, json!({"exclusiveMinimum": 0}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_subschemas_become_objects_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "properties": {"a": {}, "b": {"not": {}}},
            "additionalProperties": false
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({
                "properties": {"a": true, "b": false},
                "additionalProperties": false
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn boolean_subschemas_stay_in_draft_06() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"properties": {"a": true, "b": false}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft06,
            json!({"properties": {"a": true, "b": false}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn null_in_a_type_becomes_nullable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"type": "string", "nullable": true}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": ["null", "string"]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn several_types_become_an_any_of_of_single_types_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "anyOf": [
                {"type": "integer", "minimum": 1, "nullable": true},
                {"type": "string", "minimum": 1, "nullable": true}
            ]
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": ["integer", "string", "null"], "minimum": 1}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn several_types_without_null_are_not_nullable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "anyOf": [{"type": "integer"}, {"type": "string"}]
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": ["integer", "string"]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn subschemas_of_split_branches_are_converted_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "anyOf": [
                {"type": "array", "items": {"type": "string", "nullable": true}},
                {"type": "object", "items": {"type": "string", "nullable": true}}
            ]
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({
                "type": ["array", "object"],
                "items": {"type": ["string", "null"]}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_type_of_only_null_is_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "type",
            "/anyOf/1",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"anyOf": [{"$ref": "#/components/schemas/A"}, {"type": "null"}]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn null_types_stay_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"type": ["null", "string"]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::Draft04, json!({"type": ["null", "string"]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn dependent_keywords_become_dependencies_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "dependencies": {"a": ["b"], "c": {"required": ["d"]}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "dependentRequired": {"a": ["b"]},
                "dependentSchemas": {"c": {"required": ["d"]}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_property_with_both_dependent_keywords_gets_an_all_of_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "dependencies": {"a": {"allOf": [{"required": ["b"]}, {"minProperties": 2}]}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "dependentRequired": {"a": ["b"]},
                "dependentSchemas": {"a": {"minProperties": 2}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dependent_keywords_stay_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"dependentRequired": {"a": ["b"]}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({"dependentRequired": {"a": ["b"]}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dependent_keywords_are_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "dependentRequired",
            "",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"dependentRequired": {"a": ["b"]}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_are_kept_in_draft_07_and_deprecated_is_kept_in_openapi_30() {
        let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            Ok(json!({"deprecated": true, "$comment": "c", "examples": [1, 2]})),
            Ok(json!({"deprecated": true})),
        ];
        let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            converted(
                TargetDialect::Draft04,
                json!({"deprecated": true, "$comment": "c", "examples": [1, 2]}),
            ),
            converted(TargetDialect::OpenApi30, json!({"deprecated": true})),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_example_becomes_example_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"example": 1}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::OpenApi30, json!({"examples": [1]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn several_examples_are_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "examples",
            "",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::OpenApi30, json!({"examples": [1, 2]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn comment_is_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "$comment",
            "",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::OpenApi30, json!({"$comment": "c"}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn keywords_the_target_lacks_are_unrepresentable() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![
            unrepresentable(TargetDialect::Draft04, "contains", ""),
            unrepresentable(TargetDialect::Draft04, "if", ""),
        ]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"contains": true, "if": true}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn dynamic_references_are_unrepresentable_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::Draft201909,
            "$dynamicRef",
            "/properties/a",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({"properties": {"a": {"$dynamicRef": "#node"}}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_keywords_pass_through_every_dialect() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"x-note": 1, "discriminator": {"propertyName": "t"}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"x-note": 1, "discriminator": {"propertyName": "t"}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_embedded_resource_with_its_own_schema_is_kept_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "$defs": {"R": {"$schema": "http://json-schema.org/draft-07/schema#", "$id": "r.json"}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({
                "$defs": {"R": {"$schema": "http://json-schema.org/draft-07/schema#", "$id": "r.json"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_embedded_resource_with_another_schema_is_unrepresentable_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::Draft07,
            "$schema",
            "/definitions/R",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "$defs": {"R": {"$schema": "https://json-schema.org/draft/2020-12/schema"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_embedded_resource_with_the_target_schema_is_kept_in_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "definitions": {"R": {"$schema": "http://json-schema.org/draft-07/schema#"}}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft07,
            json!({
                "$defs": {"R": {"$schema": "http://json-schema.org/draft-07/schema#"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn document_refs_under_a_defs_path_are_not_rewritten() {
        let mut errors: Vec<ReverseCodeGenError> = Vec::new();
        let mut value: Value = json!({"$ref": "#/$defs/x/A"});
        Converter::new(TargetDialect::Draft07, "/$defs/x", &mut errors)
            .convert_value(&mut value, "", false);
        let expected: Value = json!({"$ref": "#/$defs/x/A"});
        let actual: Value = value;
        assert_eq!(expected, actual);
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        let mut value: Value = json!({"type": "string"});
        for _ in 0..50_000 {
            let mut parent: Map<String, Value> = Map::new();
            parent.insert("items".to_string(), value);
            value = Value::Object(parent);
        }
        let mut errors: Vec<ReverseCodeGenError> = Vec::new();
        let mut converted: Value = value;
        Converter::new(TargetDialect::Draft07, "/definitions", &mut errors).convert_value(
            &mut converted,
            "",
            false,
        );
        let expected: Vec<ReverseCodeGenError> = Vec::new();
        let actual: Vec<ReverseCodeGenError> = errors;
        // Tear the chain down iteratively: `Value`'s own drop would recurse 50 000 levels.
        let mut current: Value = converted;
        while let Value::Object(mut map) = current {
            current = map.remove("items").unwrap_or(Value::Null);
        }
        assert_eq!(expected, actual);
    }
}
