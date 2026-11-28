//! Rewriting the same-document `$ref`s of a document built as 2020-12 so they resolve once the
//! dialect conversion (see [`super::dialect`]) has renamed the keywords along their pointers.
//!
//! The conversion moves schemas: `$defs` entries into `definitions` (draft 4 to 7),
//! `prefixItems` into `items` and `items` into `additionalItems` (draft 4 to 2019-09), and
//! `dependentSchemas` / `dependentRequired` entries into `dependencies` (draft 4 to 7, under an
//! `allOf` when a property has several). A `$ref` whose JSON Pointer passes through one of them
//! names the schema where the 2020-12 document holds it. Before the conversion, each such
//! pointer is followed through the 2020-12 document from the root of the ref's schema resource,
//! and every token is mapped by what it is there: a keyword of the schema it is read in, an
//! entry name or an index. So a property named `$defs` or `items` is never renamed. The pointer
//! is percent-decoded and split into tokens first, and written back with
//! [`pointer_to_fragment`] only when a token changed. Tokens past the point where the pointer
//! stops resolving are kept as written. A pointer through a keyword the target lacks is still
//! mapped by its 2020-12 position: the conversion reports that keyword as
//! [`super::ReverseCodeGenError::UnrepresentableKeyword`], so no document is produced.

use super::resource_uri::starts_resource;
use super::settings::TargetDialect;
use crate::json_pointer::{decode_token, fragment_to_pointer, pointer_to_fragment};
use crate::json_schema::{Dialect, SubschemaPosition};
use serde_json::{Map, Value};

/// Keywords the conversion renames, or under which it moves schemas: only a pointer holding one
/// of these as a token can change. Each is written without `~` or `%`, so a raw token equals
/// its decoded form.
const MOVED: &[&str] = &[
    "$defs",
    "dependencies",
    "dependentRequired",
    "dependentSchemas",
    "items",
    "prefixItems",
];

/// The 2020-12 document a conversion reads refs against: the root body and the definitions the
/// generator places under `path`, each still in 2020-12 form.
struct Document<'d> {
    target: TargetDialect,
    /// The target's own keyword table (`definitions`, `additionalItems`, `dependencies`).
    target_table: Option<Dialect>,
    /// Whether the target moves `$defs` into `definitions` and the dependent keywords into
    /// `dependencies` (draft 4 to 7).
    renames_containers: bool,
    /// The JSON Pointer tokens of the definitions path, decoded.
    path: Vec<String>,
    root: &'d Value,
    definitions: &'d [(String, Value)],
}

/// Where the next token of a pointer is read.
#[derive(Clone, Copy)]
enum Role {
    /// A schema: the token is one of its keywords.
    Schema,
    /// An object of named schemas (`properties`, `$defs`): the token is an entry name.
    Entries,
    /// An array of schemas (`allOf`, `prefixItems`): the token is an index.
    List,
    /// Any other value: the rest of the pointer is kept as written.
    Value,
}

/// The base a fragment-only `$ref` resolves against: the document root, which holds the
/// generated definitions too, or the root of a nested schema resource.
#[derive(Clone, Copy)]
enum Base<'d> {
    Document,
    Resource(&'d Value),
}

/// One step of a walk to a schema: its parent's entry and its token.
#[derive(Clone, Copy)]
enum Step<'d> {
    Key(&'d str),
    Index(usize),
}

/// Rewrites every same-document `$ref` in `root` and `definitions` (placed under the JSON
/// Pointer `path`, as `(name, schema)`) whose pointer passes through a keyword the conversion
/// to `target` renames or moves. A no-op for targets that move nothing (2020-12, `OpenAPI`
/// 3.0 and 3.1, which keep the names or report the keywords).
pub(super) fn convert_ref_tails(
    target: TargetDialect,
    path: &str,
    root: &mut Value,
    definitions: &mut [(String, Value)],
) {
    let renames_containers: bool = matches!(
        target,
        TargetDialect::Draft04 | TargetDialect::Draft06 | TargetDialect::Draft07
    );
    if !renames_containers && target != TargetDialect::Draft201909 {
        return;
    }
    let path: Vec<String> = path
        .split('/')
        .skip(1)
        .map(|token: &str| decode_token(token).unwrap_or_else(|_| token.to_string()))
        .collect();
    // Found against the unchanged document first, written after: a ref names a schema where
    // the 2020-12 document holds it, whichever refs are rewritten before it.
    let rewrites: Vec<(Option<usize>, String, String)> = {
        let document: Document<'_> = Document {
            target,
            target_table: target.spec_version().map(Dialect::for_spec_version),
            renames_containers,
            path,
            root,
            definitions,
        };
        let mut rewrites: Vec<(Option<usize>, String, String)> = Vec::new();
        document.collect(None, root, &mut rewrites);
        for (index, (_, schema)) in definitions.iter().enumerate() {
            document.collect(Some(index), schema, &mut rewrites);
        }
        rewrites
    };
    for (piece, location, reference) in rewrites {
        let value: &mut Value = match piece {
            None => &mut *root,
            Some(index) => &mut definitions[index].1,
        };
        if let Some(Value::Object(map)) = value.pointer_mut(&location) {
            map.insert("$ref".to_string(), Value::String(reference));
        }
    }
}

impl<'d> Document<'d> {
    /// Collects, as `(piece, location, new ref)`, every ref in `piece` (the root body for
    /// `None`, else that definition) that the conversion would leave dangling.
    fn collect(
        &self,
        index: Option<usize>,
        piece: &'d Value,
        rewrites: &mut Vec<(Option<usize>, String, String)>,
    ) {
        let mut steps: Vec<(usize, Step<'d>)> = vec![(0, Step::Index(0))];
        let mut stack: Vec<(&'d Value, usize, Base<'d>)> = vec![(piece, 0, Base::Document)];
        while let Some((value, at, base)) = stack.pop() {
            let Value::Object(map) = value else {
                continue;
            };
            let base: Base<'d> = if self.is_resource(map) {
                Base::Resource(value)
            } else {
                base
            };
            if let Some(Value::String(reference)) = map.get("$ref")
                && let Some(mapped) = self.map_ref(base, reference)
            {
                rewrites.push((index, location(&steps, at), mapped));
            }
            for (key, child) in map {
                let Some(position) = self.position(key) else {
                    continue;
                };
                let here: usize = steps.len();
                steps.push((at, Step::Key(key)));
                match child {
                    Value::Array(items)
                        if position != SubschemaPosition::Single || key == "items" =>
                    {
                        for (item_index, item) in items.iter().enumerate() {
                            steps.push((here, Step::Index(item_index)));
                            stack.push((item, steps.len() - 1, base));
                        }
                    }
                    Value::Object(entries) if position == SubschemaPosition::Map => {
                        for (name, entry) in entries {
                            steps.push((here, Step::Key(name)));
                            stack.push((entry, steps.len() - 1, base));
                        }
                    }
                    Value::Object(_) if position != SubschemaPosition::Array => {
                        stack.push((child, here, base));
                    }
                    _ => {}
                }
            }
        }
    }

    /// Whether the schema `map` starts a schema resource of its own in the target: its `$id`
    /// (or, in draft 4, `id`) is not fragment-only.
    fn is_resource(&self, map: &Map<String, Value>) -> bool {
        let keywords: &[&str] = if self.target == TargetDialect::Draft04 {
            &["$id", "id"]
        } else {
            &["$id"]
        };
        keywords.iter().any(|keyword: &&str| {
            map.get(*keyword)
                .and_then(Value::as_str)
                .is_some_and(starts_resource)
        })
    }

    /// Where `key` holds subschemas, per the 2020-12 keyword table, then the target's.
    fn position(&self, key: &str) -> Option<SubschemaPosition> {
        Dialect::default()
            .keyword(key)
            .and_then(|keyword| keyword.subschemas)
            .or_else(|| {
                self.target_table?
                    .keyword(key)
                    .and_then(|keyword| keyword.subschemas)
            })
    }

    /// The ref `reference`, found in a schema of `base`, as it reads after the conversion:
    /// `None` when it stays as written (it is not a JSON Pointer fragment, or no token changes).
    fn map_ref(&self, base: Base<'d>, reference: &str) -> Option<String> {
        let pointer: std::borrow::Cow<'_, str> = fragment_to_pointer(reference.strip_prefix('#')?)?;
        let raw: &str = pointer.strip_prefix('/')?;
        if !raw.split('/').any(|token: &str| MOVED.contains(&token)) {
            return None;
        }
        let tokens: Vec<String> = raw
            .split('/')
            .map(decode_token)
            .collect::<Result<Vec<String>, _>>()
            .ok()?;
        let mapped: Vec<String> = self.follow(base, &tokens);
        if mapped == tokens {
            return None;
        }
        let mut converted: String = String::new();
        for token in &mapped {
            converted.push('/');
            converted.push_str(&super::escape_pointer_token(token));
        }
        Some(format!("#{}", pointer_to_fragment(&converted)))
    }

    /// The tokens of the pointer `tokens`, read from `base`, as they read after the conversion.
    fn follow(&self, base: Base<'d>, tokens: &[String]) -> Vec<String> {
        let mut mapped: Vec<String> = Vec::with_capacity(tokens.len() + 2);
        let (mut node, mut at, mut at_document_root): (Option<&'d Value>, usize, bool) = match base
        {
            Base::Resource(resource) => (Some(resource), 0, false),
            Base::Document => {
                let depth: usize = self.path.len();
                match tokens
                    .get(depth)
                    .filter(|_| tokens[..depth] == self.path[..])
                    .and_then(|name: &String| self.generated(name))
                {
                    Some(definition) => {
                        mapped.extend_from_slice(&tokens[..=depth]);
                        (Some(definition), depth + 1, false)
                    }
                    None => (Some(self.root), 0, true),
                }
            }
        };
        let mut role: Role = Role::Schema;
        while at < tokens.len() {
            let Some(current) = node else {
                break;
            };
            let token: &String = &tokens[at];
            match (role, current) {
                (Role::Schema, Value::Object(map)) => {
                    let step: Mapped<'d> = self.schema_step(map, &tokens[at..], at_document_root);
                    mapped.extend(step.tokens);
                    at += step.consumed;
                    node = step.node;
                    role = step.role;
                }
                (Role::Entries, Value::Object(entries)) => {
                    mapped.push(token.clone());
                    node = entries.get(token);
                    role = Role::Schema;
                    at += 1;
                }
                (Role::List, Value::Array(items)) => {
                    mapped.push(token.clone());
                    node = token
                        .parse::<usize>()
                        .ok()
                        .and_then(|index: usize| items.get(index));
                    role = Role::Schema;
                    at += 1;
                }
                _ => break,
            }
            at_document_root = false;
        }
        mapped.extend_from_slice(&tokens[at.min(tokens.len())..]);
        mapped
    }

    /// The first tokens of `rest` (at least one), read in the schema `map`, as they read after
    /// the conversion. `at_document_root` marks the document root, whose definitions container
    /// also holds the generated definitions.
    fn schema_step(
        &self,
        map: &'d Map<String, Value>,
        rest: &[String],
        at_document_root: bool,
    ) -> Mapped<'d> {
        let token: &str = rest[0].as_str();
        let name: Option<&String> = rest.get(1);
        match token {
            "$defs" | "definitions" => self.container_step(map, token, name, at_document_root),
            // The converter always rewrites `prefixItems` for the targets this runs for.
            "prefixItems" => Mapped::one("items", map.get(token), Role::List),
            "items" if map.contains_key("prefixItems") => {
                Mapped::one("additionalItems", map.get(token), Role::Schema)
            }
            "dependencies" | "dependentRequired" | "dependentSchemas"
                if self.renames_containers && merges_dependencies(map) =>
            {
                dependency_step(map, token, name)
            }
            _ => {
                let child: Option<&'d Value> = map.get(token);
                let role: Role = match self.position(token) {
                    _ if matches!(child, Some(Value::Array(_))) && token == "items" => Role::List,
                    Some(SubschemaPosition::Single) => Role::Schema,
                    Some(SubschemaPosition::Map) => Role::Entries,
                    Some(SubschemaPosition::Array) => Role::List,
                    Some(SubschemaPosition::SingleOrArray) => {
                        if matches!(child, Some(Value::Array(_))) {
                            Role::List
                        } else {
                            Role::Schema
                        }
                    }
                    None => Role::Value,
                };
                Mapped::one(token, child, role)
            }
        }
    }

    /// The step through the definitions container `token` (`$defs` or `definitions`) of `map`
    /// and its entry `name`. Draft 4 to 7 call `$defs` `definitions`, whether or not `map`
    /// holds it (unless the two containers collide, which the conversion reports), and the
    /// document root's `definitions` also holds the generated definitions when they go there.
    fn container_step(
        &self,
        map: &'d Map<String, Value>,
        token: &str,
        name: Option<&String>,
        at_document_root: bool,
    ) -> Mapped<'d> {
        let defs: Option<&'d Value> = map.get("$defs");
        let definitions: Option<&'d Value> = map.get("definitions");
        let moved: bool = self.renames_containers
            && match (definitions, defs) {
                (Some(Value::Object(existing)), Some(Value::Object(defs))) => {
                    defs.keys().all(|key: &String| !existing.contains_key(key))
                }
                (Some(_), Some(_)) => false,
                _ => true,
            };
        let container: &str = if token == "$defs" && moved {
            "definitions"
        } else {
            token
        };
        let Some(name) = name else {
            return Mapped::one(container, map.get(token), Role::Entries);
        };
        let generated: Option<&'d Value> =
            (at_document_root && self.renames_containers && self.path == ["definitions"])
                .then(|| self.generated(name))
                .flatten();
        let entry = |container: Option<&'d Value>| -> Option<&'d Value> {
            container?.as_object()?.get(name.as_str())
        };
        let found: Option<(&str, &'d Value)> = if token == "definitions" {
            entry(definitions)
                .or_else(|| moved.then(|| entry(defs)).flatten())
                .or(generated)
                .map(|schema: &'d Value| ("definitions", schema))
        } else {
            entry(defs)
                .map(|schema: &'d Value| (container, schema))
                .or_else(|| generated.map(|schema: &'d Value| ("definitions", schema)))
        };
        match found {
            Some((container, schema)) => Mapped {
                tokens: vec![container.to_string(), name.clone()],
                consumed: 2,
                node: Some(schema),
                role: Role::Schema,
            },
            None => Mapped {
                tokens: vec![container.to_string(), name.clone()],
                consumed: 2,
                node: None,
                role: Role::Value,
            },
        }
    }

    /// The generated definition `name`.
    fn generated(&self, name: &str) -> Option<&'d Value> {
        self.definitions
            .iter()
            .find(|(entry, _)| entry == name)
            .map(|(_, schema)| schema)
    }
}

/// Tokens of a pointer read in one schema, as they read after the conversion.
struct Mapped<'d> {
    tokens: Vec<String>,
    /// How many tokens of the pointer they stand for.
    consumed: usize,
    /// The value the tokens name, `None` when the pointer stops resolving there.
    node: Option<&'d Value>,
    /// How the value reads the next token.
    role: Role,
}

impl<'d> Mapped<'d> {
    fn one(token: &str, node: Option<&'d Value>, role: Role) -> Self {
        Mapped {
            tokens: vec![token.to_string()],
            consumed: 1,
            node,
            role,
        }
    }
}

/// Whether the conversion merges `map`'s dependent keywords into `dependencies`: one of them is
/// present and none of the three is malformed (which the conversion reports instead).
fn merges_dependencies(map: &Map<String, Value>) -> bool {
    (map.contains_key("dependentRequired") || map.contains_key("dependentSchemas"))
        && ["dependencies", "dependentRequired", "dependentSchemas"]
            .iter()
            .all(|keyword: &&str| map.get(*keyword).is_none_or(Value::is_object))
}

/// The step through `map`'s dependency keyword `token` and its property `name`, in a schema
/// whose dependent keywords the conversion merges: the property's entries, from
/// `dependencies`, `dependentRequired` and `dependentSchemas` in that order, become
/// `dependencies/<name>`, or its `allOf` (a property-name list there as `{"required": [...]}`)
/// when there are several.
fn dependency_step<'d>(
    map: &'d Map<String, Value>,
    token: &str,
    name: Option<&String>,
) -> Mapped<'d> {
    let Some(name) = name else {
        return Mapped::one("dependencies", None, Role::Value);
    };
    let entries: Vec<(&str, &'d Value)> = ["dependencies", "dependentRequired", "dependentSchemas"]
        .into_iter()
        .filter_map(|keyword: &str| {
            map.get(keyword)?
                .as_object()?
                .get(name.as_str())
                .map(|entry: &'d Value| (keyword, entry))
        })
        .collect();
    let mut tokens: Vec<String> = vec!["dependencies".to_string(), name.clone()];
    let Some(position) = entries.iter().position(|(keyword, _)| *keyword == token) else {
        return Mapped {
            tokens,
            consumed: 2,
            node: None,
            role: Role::Value,
        };
    };
    let entry: &'d Value = entries[position].1;
    if entries.len() > 1 {
        tokens.push("allOf".to_string());
        tokens.push(position.to_string());
        if entry.is_array() {
            tokens.push("required".to_string());
        }
    }
    Mapped {
        tokens,
        consumed: 2,
        node: Some(entry),
        role: if entry.is_array() {
            Role::Value
        } else {
            Role::Schema
        },
    }
}

/// The RFC 6901 JSON Pointer of the step `at`.
fn location(steps: &[(usize, Step<'_>)], at: usize) -> String {
    let mut tokens: Vec<Step<'_>> = Vec::new();
    let mut current: usize = at;
    while current != 0 {
        let (parent, step): (usize, Step<'_>) = steps[current];
        tokens.push(step);
        current = parent;
    }
    let mut pointer: String = String::new();
    for step in tokens.into_iter().rev() {
        pointer.push('/');
        match step {
            Step::Key(key) => pointer.push_str(&super::escape_pointer_token(key)),
            Step::Index(index) => pointer.push_str(&index.to_string()),
        }
    }
    pointer
}

#[cfg(test)]
mod tests {
    use super::convert_ref_tails;
    use crate::reverse_code_gen::settings::TargetDialect;
    use serde_json::{Value, json};

    /// `root` after the pre-pass for `target`, with no generated definitions.
    fn mapped(target: TargetDialect, root: Value) -> Value {
        let mut root: Value = root;
        convert_ref_tails(
            target,
            target.default_definitions_path(),
            &mut root,
            &mut [],
        );
        root
    }

    /// The `$ref` of the property `r` once `root` (which holds it) is mapped for `target`.
    fn mapped_ref(target: TargetDialect, root: Value) -> Value {
        mapped(target, root)["properties"]["r"]["$ref"].clone()
    }

    #[test]
    fn a_ref_into_a_nested_defs_names_definitions_on_draft_07() {
        let expected: Value = json!("#/properties/x/definitions/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_a_nested_defs_keeps_it_on_2019_09() {
        let expected: Value = json!("#/properties/x/$defs/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft201909,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_a_nested_defs_keeps_it_on_2020_12() {
        let expected: Value = json!("#/properties/x/$defs/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft202012,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_a_nested_defs_keeps_it_on_openapi_3_0() {
        let expected: Value = json!("#/properties/x/$defs/Y");
        let actual: Value = mapped_ref(
            TargetDialect::OpenApi30,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_a_prefix_item_names_items_on_draft_06() {
        let expected: Value = json!("#/properties/x/items/1");
        let actual: Value = mapped_ref(
            TargetDialect::Draft06,
            json!({"properties": {
                "x": {"prefixItems": [true, {"type": "string"}]},
                "r": {"$ref": "#/properties/x/prefixItems/1"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_items_beside_prefix_items_names_additional_items_on_draft_07() {
        let expected: Value = json!("#/properties/x/additionalItems");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"prefixItems": [true], "items": {"type": "string"}},
                "r": {"$ref": "#/properties/x/items"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_items_without_prefix_items_keeps_items_on_draft_07() {
        let expected: Value = json!("#/properties/x/items");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"items": {"type": "string"}},
                "r": {"$ref": "#/properties/x/items"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_property_named_defs_is_not_renamed_on_draft_07() {
        let expected: Value = json!("#/properties/$defs/properties/prefixItems");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "$defs": {"properties": {"prefixItems": true}},
                "r": {"$ref": "#/properties/$defs/properties/prefixItems"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_defs_entry_named_items_is_not_renamed_on_draft_07() {
        let expected: Value = json!("#/definitions/items");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({
                "$defs": {"items": true},
                "prefixItems": [true],
                "properties": {"r": {"$ref": "#/$defs/items"}}
            }),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_defs_colliding_with_definitions_is_kept_on_draft_07() {
        let expected: Value = json!("#/properties/x/$defs/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"$defs": {"Y": true}, "definitions": {"Y": false}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_defs_disjoint_from_definitions_is_renamed_on_draft_07() {
        let expected: Value = json!("#/properties/x/definitions/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"$defs": {"Y": true}, "definitions": {"Z": false}},
                "r": {"$ref": "#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_single_dependent_schema_names_its_dependencies_entry_on_draft_07() {
        let expected: Value = json!("#/properties/x/dependencies/p/properties/q");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"dependentSchemas": {"p": {"properties": {"q": true}}}},
                "r": {"$ref": "#/properties/x/dependentSchemas/p/properties/q"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_dependent_schema_beside_an_existing_dependency_names_its_all_of_branch_on_draft_07() {
        let expected: Value = json!("#/properties/x/dependencies/p/allOf/1");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {
                    "dependencies": {"p": ["q"]},
                    "dependentSchemas": {"p": {"type": "object"}}
                },
                "r": {"$ref": "#/properties/x/dependentSchemas/p"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_existing_dependency_beside_a_dependent_schema_names_its_all_of_branch_on_draft_07() {
        let expected: Value = json!("#/properties/x/dependencies/p/allOf/0");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {
                    "dependencies": {"p": {"type": "object"}},
                    "dependentSchemas": {"p": {"type": "object"}}
                },
                "r": {"$ref": "#/properties/x/dependencies/p"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_dependent_schema_keeps_its_keyword_on_2019_09() {
        let expected: Value = json!("#/properties/x/dependentSchemas/p");
        let actual: Value = mapped_ref(
            TargetDialect::Draft201909,
            json!({"properties": {
                "x": {"dependentSchemas": {"p": {"type": "object"}}},
                "r": {"$ref": "#/properties/x/dependentSchemas/p"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_inside_a_nested_resource_is_read_from_that_resource_on_draft_07() {
        let expected: Value = json!("#/properties/t/additionalItems");
        let actual: Value = mapped(
            TargetDialect::Draft07,
            json!({"properties": {
                "t": {"items": true},
                "n": {
                    "$id": "https://example.com/n.json",
                    "properties": {
                        "t": {"prefixItems": [true], "items": true},
                        "r": {"$ref": "#/properties/t/items"}
                    }
                }
            }}),
        )["properties"]["n"]["properties"]["r"]["$ref"]
            .clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_draft_04_id_starts_a_resource() {
        let expected: Value = json!("#/properties/t/additionalItems");
        let actual: Value = mapped(
            TargetDialect::Draft04,
            json!({"properties": {
                "t": {"items": true},
                "n": {
                    "id": "https://example.com/n.json",
                    "properties": {
                        "t": {"prefixItems": [true], "items": true},
                        "r": {"$ref": "#/properties/t/items"}
                    }
                }
            }}),
        )["properties"]["n"]["properties"]["r"]["$ref"]
            .clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_fragment_only_id_does_not_start_a_resource_on_draft_07() {
        let expected: Value = json!("#/properties/t/additionalItems");
        let actual: Value = mapped(
            TargetDialect::Draft07,
            json!({"properties": {
                "t": {"prefixItems": [true], "items": true},
                "n": {
                    "$id": "#n",
                    "properties": {
                        "t": {"items": true},
                        "r": {"$ref": "#/properties/t/items"}
                    }
                }
            }}),
        )["properties"]["n"]["properties"]["r"]["$ref"]
            .clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_with_a_base_uri_is_kept_on_draft_07() {
        let expected: Value = json!("other.json#/properties/x/$defs/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "other.json#/properties/x/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_percent_encoded_tail_is_decoded_then_re_encoded_on_draft_07() {
        let expected: Value = json!("#/properties/a%20b/definitions/Y%25");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "a b": {"$defs": {"Y%": true}},
                "r": {"$ref": "#/properties/a%20b/%24defs/Y%25"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_escaped_token_keeps_its_escape_on_draft_07() {
        let expected: Value = json!("#/properties/a~1b/definitions/c~0d");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "a/b": {"$defs": {"c~d": true}},
                "r": {"$ref": "#/properties/a~1b/$defs/c~0d"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_that_moves_nothing_is_kept_as_written_on_draft_07() {
        let expected: Value = json!("#/properties/%78");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {"x": true, "r": {"$ref": "#/properties/%78"}}}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_dangling_tail_keeps_its_tokens_and_renames_the_defs_by_position_on_draft_07() {
        let expected: Value = json!("#/properties/x/definitions/Missing/prefixItems/0");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"$defs": {"Y": true}},
                "r": {"$ref": "#/properties/x/$defs/Missing/prefixItems/0"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_tail_through_a_keyword_the_target_lacks_is_mapped_by_its_2020_12_position_on_draft_07() {
        let expected: Value = json!("#/properties/x/unevaluatedProperties/definitions/Y");
        let actual: Value = mapped_ref(
            TargetDialect::Draft07,
            json!({"properties": {
                "x": {"unevaluatedProperties": {"$defs": {"Y": true}}},
                "r": {"$ref": "#/properties/x/unevaluatedProperties/$defs/Y"}
            }}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_into_a_generated_definition_under_a_custom_path_is_followed_into_it() {
        let expected: Value = json!("#/x/G/properties/a/definitions/Y");
        let mut root: Value = json!({"properties": {"r": {"$ref": "#/x/G/properties/a/$defs/Y"}}});
        let mut definitions: Vec<(String, Value)> = vec![(
            "G".to_string(),
            json!({"properties": {"a": {"$defs": {"Y": true}}}}),
        )];
        convert_ref_tails(TargetDialect::Draft07, "/x", &mut root, &mut definitions);
        let actual: Value = root["properties"]["r"]["$ref"].clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_ref_inside_a_generated_definition_is_mapped() {
        let expected: Value = json!("#/definitions/G/properties/a/items/0");
        let mut root: Value = json!({});
        let mut definitions: Vec<(String, Value)> = vec![(
            "G".to_string(),
            json!({"properties": {
                "a": {"prefixItems": [true]},
                "r": {"$ref": "#/definitions/G/properties/a/prefixItems/0"}
            }}),
        )];
        convert_ref_tails(
            TargetDialect::Draft07,
            "/definitions",
            &mut root,
            &mut definitions,
        );
        let actual: Value = definitions[0].1["properties"]["r"]["$ref"].clone();
        assert_eq!(expected, actual);
    }
}
