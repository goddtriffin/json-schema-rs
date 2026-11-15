//! Applying `default` values to an instance before validating it (see
//! [`CompiledSchema::validate_with_defaults`]).

use super::compiled::{CompiledSchema, Keyword, NodeId};
use super::error::ValidationResult;
use serde_json::Value;

/// One step from a value to one of its members or items.
#[derive(Debug, Clone)]
enum Step {
    Key(Box<str>),
    Index(usize),
}

/// The value `path` leads to from `root`, if it is still there.
fn at_mut<'v>(root: &'v mut Value, path: &[Step]) -> Option<&'v mut Value> {
    path.iter()
        .try_fold(root, |value, step| match (step, value) {
            (Step::Key(key), Value::Object(object)) => object.get_mut(&**key),
            (Step::Index(index), Value::Array(items)) => items.get_mut(*index),
            _ => None,
        })
}

impl CompiledSchema {
    /// Inserts the `default` of every missing property the schema lists, then validates the
    /// instance with defaults in place, like [`validate`](Self::validate).
    ///
    /// `default` is an annotation in every draft: [`validate`](Self::validate) never changes
    /// the instance. This method is the opt-in divergence that fills an instance in, as ajv's
    /// `useDefaults` does.
    ///
    /// - A member missing from an object gets the `default` of its `properties` subschema (or
    ///   of a schema that subschema applies in place through `$ref` / `allOf`), copied as is.
    /// - Defaults apply through every schema that applies to a value unconditionally: `$ref`,
    ///   `allOf`, `properties`, `patternProperties`, `additionalProperties`, `items` and
    ///   `prefixItems` (array-form `items` and `additionalItems` in earlier drafts).
    /// - Branches that may not apply are skipped: `anyOf`, `oneOf`, `not`, `if` / `then` /
    ///   `else`, `dependentSchemas`, `contains`, the `unevaluated*` keywords and the dynamic
    ///   references `$dynamicRef` / `$recursiveRef`.
    /// - A default is inserted whole: defaults are not applied inside a value that was itself
    ///   a default, so a recursive schema cannot grow the instance without end.
    /// - Missing array items are left missing: a tuple is not padded.
    ///
    /// ```
    /// use json_schema_rs::{CompiledSchema, JsonSchemaSettings, Registry, Schema};
    /// use serde_json::{Value, json};
    ///
    /// let settings: JsonSchemaSettings = JsonSchemaSettings::default();
    /// let schema: Schema = Schema::new_from_serde_value(
    ///     &json!({"properties": {"port": {"type": "integer", "default": 8080}}}),
    ///     &settings,
    /// )
    /// .unwrap();
    /// let registry: Registry<'_> = Registry::builder().build(&schema).unwrap();
    /// let compiled: CompiledSchema = CompiledSchema::builder().build(&registry).unwrap();
    /// let mut instance: Value = json!({});
    /// compiled.validate_with_defaults(&mut instance).unwrap();
    /// assert_eq!(json!({"port": 8080}), instance);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `Err(errors)` when the instance, defaults applied, does not conform to the
    /// schema. The defaults stay applied.
    pub fn validate_with_defaults(&self, instance: &mut Value) -> ValidationResult {
        self.apply_defaults(instance);
        self.validate(instance)
    }

    /// Inserts the defaults [`validate_with_defaults`](Self::validate_with_defaults) applies.
    fn apply_defaults(&self, instance: &mut Value) {
        if self.defaults.is_empty() {
            return;
        }
        // Each (schema, path to the value it applies to) still to visit; a value nests less
        // deep than the instance, so the stack stays bounded by the instance.
        let mut stack: Vec<(NodeId, Vec<Step>)> = vec![(self.root, Vec::new())];
        let mut children: Vec<(NodeId, Step)> = Vec::new();
        while let Some((node, path)) = stack.pop() {
            let Some(value) = at_mut(instance, &path) else {
                continue;
            };
            self.in_place_closure(node, |node| {
                for keyword in &self.node(node).keywords {
                    self.default_keyword(keyword, value, &mut children);
                }
            });
            // Pushed in reverse so members are visited in order.
            for (child, step) in children.drain(..).rev() {
                let mut child_path: Vec<Step> = Vec::with_capacity(path.len() + 1);
                child_path.extend(path.iter().cloned());
                child_path.push(step);
                stack.push((child, child_path));
            }
        }
    }

    /// Applies `keyword` to `value`: inserts its missing properties' defaults, and lists in
    /// `children` the members and items it applies a subschema to.
    fn default_keyword(
        &self,
        keyword: &Keyword,
        value: &mut Value,
        children: &mut Vec<(NodeId, Step)>,
    ) {
        match (keyword, value) {
            (Keyword::Properties(id) | Keyword::ClosedProperties(id), Value::Object(object)) => {
                for (name, child) in self.property_table(*id).iter() {
                    if object.contains_key(name) {
                        children.push((child, Step::Key(name.into())));
                    } else if let Some(default) = self.default_in_place(child) {
                        object.insert(name.to_owned(), default.clone());
                    }
                }
            }
            (Keyword::PatternProperties(patterns), Value::Object(object)) => {
                for key in object.keys() {
                    for (regex, child) in patterns {
                        if self.pattern(*regex).is_match(key) {
                            children.push((*child, Step::Key(key.as_str().into())));
                        }
                    }
                }
            }
            (Keyword::AdditionalProperties { node, known }, Value::Object(object)) => {
                for key in object.keys() {
                    if !self.is_known(known, key) {
                        children.push((*node, Step::Key(key.as_str().into())));
                    }
                }
            }
            (Keyword::Items { node, .. }, Value::Array(items)) => {
                children.extend((0..items.len()).map(|index| (*node, Step::Index(index))));
            }
            (Keyword::PrefixItems(tuple), Value::Array(items)) => {
                for index in 0..items.len() {
                    let child: Option<NodeId> = tuple.prefix.get(index).copied().or(tuple.rest);
                    if let Some(child) = child {
                        children.push((child, Step::Index(index)));
                    }
                }
            }
            _ => {}
        }
    }

    /// The `default` of `node`, or else of the first schema it applies in place that has one.
    fn default_in_place(&self, node: NodeId) -> Option<&Value> {
        if let Some(default) = self.defaults.get(&node) {
            return Some(default);
        }
        let mut found: Option<&Value> = None;
        self.in_place_closure(node, |node| {
            if found.is_none() {
                found = self.defaults.get(&node);
            }
        });
        found
    }
}
