//! Converting a schema built as 2020-12 into the target dialect.
//!
//! The generator builds every document as 2020-12. For any other target one pass walks the
//! document (an explicit stack, so depth cannot overflow) and rewrites each schema object in
//! place. Every rewrite is exact: a construct the target cannot express is reported as a
//! [`ReverseCodeGenError::UnrepresentableKeyword`] and left as it was, never approximated.
//! Subschemas are found through the 2020-12 keyword table and the target's own (draft 4 to 2019-09
//! keep `additionalItems`, `dependencies` and `definitions` there), so hand-written schemas that
//! reach the generator are converted too. The result is JSON: the typed model holds the
//! 2020-12 form, and the converted document is kept beside it (see `GeneratedSchema`).

use super::error::ReverseCodeGenError;
use super::settings::TargetDialect;
use crate::json_schema::{Dialect, SpecVersion, SubschemaPosition};
use crate::number::{cmp_num, json_equal};
use serde::Serialize;
use serde_json::{Map, Number, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// The keywords of an `OpenAPI` 3.0 Schema Object that this converter writes or keeps: draft-04's
/// subset plus `nullable`, `readOnly`, `writeOnly`, `example`, `deprecated`, `discriminator`,
/// `xml` and `externalDocs`. `OpenAPI` 3.0 forbids any other key but `x-` extensions.
const OPENAPI_30_KEYWORDS: &[&str] = &[
    "$ref",
    "additionalProperties",
    "allOf",
    "anyOf",
    "default",
    "deprecated",
    "description",
    "discriminator",
    "enum",
    "example",
    "exclusiveMaximum",
    "externalDocs",
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
    "xml",
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
    /// The target's own keyword table, for the keywords 2020-12 lacks (`additionalItems`,
    /// `dependencies`, `definitions`); none for `OpenAPI` 3.0, whose Schema Object has none of
    /// them.
    target_table: Option<Dialect>,
    /// Whether the document root gets keywords beside it after the conversion (`$schema`,
    /// definitions), which a root `$ref` must not sit next to before 2019-09.
    root_siblings: bool,
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
            target_table: target.spec_version().map(Dialect::for_spec_version),
            root_siblings: false,
        }
    }

    /// Marks that the document root gets keywords beside it after the conversion.
    pub(super) fn with_root_siblings(mut self, root_siblings: bool) -> Self {
        self.root_siblings = root_siblings;
        self
    }

    /// Converts one schema, located at `pointer` in the generated document, to JSON in the
    /// target dialect. `root` marks the document root.
    pub(super) fn convert(&mut self, schema: &impl Serialize, pointer: &str, root: bool) -> Value {
        let Ok(mut value) = serde_json::to_value(schema) else {
            unreachable!("a schema serializes to JSON");
        };
        self.convert_value(&mut value, pointer, root);
        value
    }

    /// [`Converter::convert`] in place.
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
                push_subschemas(map, at, self.target_table, &mut self.pointers, &mut stack);
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
            && (map.len() > 1 || (root && self.root_siblings))
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
            self.convert_dependent(map, at, &mut reported);
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

        // Every other keyword 2020-12 defines must exist in the target, or only annotate (and
        // `OpenAPI` 3.0 has no unknown keywords at all).
        let source: Dialect = Dialect::default();
        let missing: Vec<String> = map
            .keys()
            .filter(|key| {
                let annotation: bool = is_draft_4_to_7 && ANNOTATIONS.contains(&key.as_str());
                // `OpenAPI` 3.0 forbids every key but its own and `x-` extensions; the drafts
                // ignore a keyword they do not define, so only the ones 2020-12 defines can
                // change a verdict.
                let known: bool = if is_openapi_30 {
                    !key.starts_with("x-")
                } else {
                    source.keyword(key).is_some()
                };
                !reported.contains(key.as_str())
                    && known
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
    /// must be one of its members and the `enum` shrinks to it; when it is not, nothing
    /// satisfies the schema, which is `{"not": {}}` (an empty `enum` is invalid in draft 4).
    fn convert_const(map: &mut Map<String, Value>) {
        let Some(value) = map.remove("const") else {
            return;
        };
        match map.get("enum") {
            Some(Value::Array(members))
                if !members.iter().any(|member| json_equal(member, &value)) =>
            {
                map.remove("enum");
                map.insert("not".to_string(), Value::Object(Map::new()));
            }
            _ => {
                map.insert("enum".to_string(), Value::Array(vec![value]));
            }
        }
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
            [] if nullable && matches!(map.get("enum"), None | Some(Value::Array(_))) => {
                // The `OpenAPI` 3.0.3 idiom: `null` is the only member of the `enum`. Only
                // `null` satisfies `type: null`, so an `enum` keeps just its null members; with
                // none, nothing satisfies the schema, which is `{"not": {}}`.
                map.remove("type");
                let has_null: bool = match map.get("enum") {
                    Some(Value::Array(members)) => members.iter().any(Value::is_null),
                    _ => true,
                };
                if has_null {
                    map.insert("nullable".to_string(), Value::Bool(true));
                    map.insert("enum".to_string(), Value::Array(vec![Value::Null]));
                } else {
                    map.remove("enum");
                    map.insert("not".to_string(), Value::Object(Map::new()));
                }
                false
            }
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
    map.insert(exclusive.to_string(), Value::Bool(true));
}

impl Converter<'_> {
    /// `dependentRequired` and `dependentSchemas` are `dependencies` before 2019-09. A property
    /// that both name (or that `dependencies` already names) needs both: its entries become one
    /// `allOf` of schemas. A keyword that is not an object has no entries to move: it is
    /// reported and the schema is left as it was.
    fn convert_dependent(
        &mut self,
        map: &mut Map<String, Value>,
        at: usize,
        reported: &mut BTreeSet<String>,
    ) {
        if !map.contains_key("dependentRequired") && !map.contains_key("dependentSchemas") {
            return;
        }
        let mut malformed: bool = false;
        for keyword in ["dependentRequired", "dependentSchemas", "dependencies"] {
            if matches!(map.get(keyword), Some(value) if !value.is_object()) {
                self.report(keyword, at);
                reported.insert(keyword.to_string());
                malformed = true;
            }
        }
        if malformed {
            return;
        }
        let required: Option<Value> = map.remove("dependentRequired");
        let schemas: Option<Value> = map.remove("dependentSchemas");
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

/// Pushes the schemas `map`'s keywords hold, per the 2020-12 keyword table, then the target's
/// (`additionalItems`, `dependencies` and `definitions` are not 2020-12 keywords). `at` is
/// `map`'s own entry in `pointers`; each pushed schema gets an entry of its own.
fn push_subschemas<'a>(
    map: &'a mut Map<String, Value>,
    at: usize,
    target_table: Option<Dialect>,
    pointers: &mut Vec<(usize, String)>,
    stack: &mut Vec<Work<'a>>,
) {
    let source: Dialect = Dialect::default();
    let mut child = |parent: usize, token: &str| -> usize {
        pointers.push((parent, escape(token)));
        pointers.len() - 1
    };
    for (key, value) in map.iter_mut() {
        let Some(position) = source
            .keyword(key)
            .and_then(|keyword| keyword.subschemas)
            .or_else(|| {
                target_table?
                    .keyword(key)
                    .and_then(|keyword| keyword.subschemas)
            })
        else {
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
        if errors.is_empty() {
            Ok(value)
        } else {
            Err(errors)
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
    fn additional_items_subschemas_are_converted_in_draft_04() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "items": [{"type": "string"}],
            "additionalItems": {"allOf": [{"$ref": "#/definitions/A"}], "enum": [1]}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({
                "prefixItems": [{"type": "string"}],
                "items": {"$ref": "#/$defs/A", "const": 1}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn additional_items_subschemas_are_converted_in_2019_09() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "items": [{"type": "string"}],
            "additionalItems": {"$ref": "#/$defs/A", "items": [{"type": "integer"}]}
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft201909,
            json!({
                "prefixItems": [{"type": "string"}],
                "items": {"$ref": "#/$defs/A", "prefixItems": [{"type": "integer"}]}
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
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"not": {}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::Draft04,
            json!({"const": 4, "enum": [1, 2, 3]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn const_outside_the_enum_conforms_to_the_draft_04_metaschema() {
        let mut schema: Value = converted(
            TargetDialect::Draft04,
            json!({"const": 4, "enum": [1, 2, 3]}),
        )
        .unwrap_or_default();
        schema["$schema"] = json!("http://json-schema.org/draft-04/schema#");
        let expected: bool = true;
        let actual: bool = crate::meta::is_valid(&schema);
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
    fn a_type_of_only_null_is_a_nullable_null_enum_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({
            "anyOf": [
                {"$ref": "#/components/schemas/A"},
                {"nullable": true, "enum": [null]}
            ]
        }));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"anyOf": [{"$ref": "#/components/schemas/A"}, {"type": "null"}]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_type_of_only_null_beside_a_null_enum_is_a_nullable_null_enum_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"nullable": true, "enum": [null]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": "null", "enum": [null]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_type_of_only_null_beside_a_null_const_is_a_nullable_null_enum_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"nullable": true, "enum": [null]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": "null", "const": null}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_type_of_only_null_beside_a_mixed_enum_keeps_only_the_null_member_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"nullable": true, "enum": [null]}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": "null", "enum": [1, null, "a"]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_type_of_only_null_beside_an_enum_without_null_matches_nothing_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"not": {}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"type": "null", "enum": [1, "a"]}),
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
    fn a_dependent_keyword_that_is_not_an_object_is_unrepresentable_in_draft_07() {
        let expected: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            Err(vec![unrepresentable(
                TargetDialect::Draft07,
                "dependentRequired",
                "",
            )]),
            Err(vec![unrepresentable(
                TargetDialect::Draft07,
                "dependentSchemas",
                "",
            )]),
            Err(vec![
                unrepresentable(TargetDialect::Draft07, "dependencies", ""),
                unrepresentable(TargetDialect::Draft07, "dependentRequired", ""),
            ]),
        ];
        let actual: Vec<Result<Value, Vec<ReverseCodeGenError>>> = vec![
            converted(TargetDialect::Draft07, json!({"dependentRequired": 5})),
            converted(TargetDialect::Draft07, json!({"dependentSchemas": []})),
            converted(
                TargetDialect::Draft07,
                json!({"dependentRequired": {"a": ["b"]}, "dependencies": "x"}),
            ),
        ];
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
    fn extensions_and_openapi_keywords_pass_through_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> =
            Ok(json!({"x-note": 1, "discriminator": {"propertyName": "t"}}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> = converted(
            TargetDialect::OpenApi30,
            json!({"x-note": 1, "discriminator": {"propertyName": "t"}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_unknown_keyword_is_unrepresentable_in_openapi_30() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Err(vec![unrepresentable(
            TargetDialect::OpenApi30,
            "custom",
            "",
        )]);
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::OpenApi30, json!({"custom": 1}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_unknown_keyword_passes_through_draft_07() {
        let expected: Result<Value, Vec<ReverseCodeGenError>> = Ok(json!({"custom": 1}));
        let actual: Result<Value, Vec<ReverseCodeGenError>> =
            converted(TargetDialect::Draft07, json!({"custom": 1}));
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
