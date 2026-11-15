//! [`Registry::canonicalize`]: the root document in one normal form.

use super::dereference::{at_mut, subschema_paths};
use super::{Registry, Scope};
use crate::json_schema::dialect::{Dialect, ExclusiveBounds, VocabularyKind};
use serde_json::{Map, Number, Value};
use std::cmp::Ordering;

/// One step of the walk over the document's subschemas.
enum Step {
    /// Queue the schema at this path (in the document) and its subschemas, in scope.
    Enter(Vec<String>, Scope),
    /// Rewrite the schema at this path, whose subschemas are rewritten already.
    Leave(Vec<String>, Dialect),
}

/// `type` names in their canonical order.
const TYPE_ORDER: [&str; 7] = [
    "array", "boolean", "integer", "null", "number", "object", "string",
];

impl Registry<'_> {
    /// The root document rewritten into one normal form, so that schemas written differently
    /// but meaning the same compare equal more often, and tools read a simpler schema.
    ///
    /// Each rewrite keeps the verdict and the annotations of every instance, in every draft
    /// it applies to, and only where the keyword's vocabulary is active:
    ///
    /// - `type`: an array is deduplicated and sorted, `integer` beside `number` is dropped
    ///   (every integer is a number), and one type is written as a string.
    /// - `enum` is deduplicated and sorted; an `enum` of one value becomes `const` (from
    ///   draft 6). `required` is deduplicated and sorted.
    /// - Assertions that accept everything are dropped: `minLength`, `minItems` and
    ///   `minProperties` of `0`, `minContains` of `1`, `uniqueItems: false`, `required: []`,
    ///   and draft 4's `exclusiveMinimum` / `exclusiveMaximum` of `false`.
    /// - Of `minimum` and a numeric `exclusiveMinimum` (from draft 6), the looser bound is
    ///   dropped; likewise for `maximum` and `exclusiveMaximum`.
    /// - A schema left empty (`{}`) becomes `true` (from draft 6).
    /// - Keywords are in sorted order.
    ///
    /// No rewrite moves or removes a subschema, so every JSON Pointer into the document still
    /// reaches the same schema, and `$ref`s are kept as written. Canonicalizing the output
    /// returns it unchanged.
    ///
    /// ```
    /// use json_schema_rs::{JsonSchemaSettings, Registry, Schema};
    /// use serde_json::{Value, json};
    ///
    /// let settings: JsonSchemaSettings = JsonSchemaSettings::default();
    /// let root: Schema = Schema::new_from_serde_value(
    ///     &json!({"type": ["number", "integer"], "enum": [3], "minimum": 1, "exclusiveMinimum": 2}),
    ///     &settings,
    /// )
    /// .unwrap();
    /// let registry: Registry<'_> = Registry::builder().build(&root).unwrap();
    /// let canonical: Value = registry.canonicalize().unwrap();
    /// assert_eq!(json!({"const": 3, "exclusiveMinimum": 2, "type": "number"}), canonical);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns the [`serde_json::Error`] of a document that cannot be serialized to JSON.
    pub fn canonicalize(&self) -> Result<Value, serde_json::Error> {
        let mut documents: Vec<Option<Value>> = vec![None; self.documents.len()];
        let mut output: Value = self.document_value(&mut documents, 0)?.clone();
        let mut stack: Vec<Step> = vec![Step::Enter(Vec::new(), self.root_scope())];
        while let Some(step) = stack.pop() {
            match step {
                Step::Enter(path, scope) => {
                    let Some(Value::Object(object)) = at_mut(&mut output, &path) else {
                        continue;
                    };
                    let scope: Scope = self.enter_raw(scope, object);
                    let dialect: Dialect = self.dialect(scope);
                    let children: Vec<Vec<String>> = subschema_paths(object, dialect);
                    stack.push(Step::Leave(path.clone(), dialect));
                    for child in children {
                        let mut child_path: Vec<String> = path.clone();
                        child_path.extend(child);
                        stack.push(Step::Enter(child_path, scope));
                    }
                }
                Step::Leave(path, dialect) => {
                    let Some(slot) = at_mut(&mut output, &path) else {
                        continue;
                    };
                    if let Value::Object(object) = slot {
                        rewrite(object, dialect);
                        if object.is_empty() && dialect.boolean_schemas() {
                            *slot = Value::Bool(true);
                        }
                    }
                }
            }
        }
        Ok(output)
    }
}

/// Applies every rewrite to one schema object, whose subschemas are canonical already.
fn rewrite(object: &mut Map<String, Value>, dialect: Dialect) {
    if !dialect.vocabulary_active(VocabularyKind::Validation) {
        return;
    }
    let defines = |keyword: &str| dialect.keyword(keyword).is_some();
    if defines("type") {
        canonical_type(object);
    }
    if defines("enum")
        && let Some(Value::Array(values)) = object.get_mut("enum")
    {
        sort_and_dedup(values);
        if values.len() == 1 && defines("const") && !object.contains_key("const") {
            let Some(Value::Array(mut values)) = object.remove("enum") else {
                unreachable!("matched an array above");
            };
            object.insert("const".to_string(), values.remove(0));
        }
    }
    if defines("required")
        && let Some(Value::Array(names)) = object.get_mut("required")
        && names.iter().all(Value::is_string)
    {
        sort_and_dedup(names);
        if names.is_empty() {
            object.remove("required");
        }
    }
    for (keyword, accepts_all) in [
        ("minLength", 0),
        ("minItems", 0),
        ("minProperties", 0),
        ("minContains", 1),
    ] {
        if defines(keyword)
            && let Some(Value::Number(n)) = object.get(keyword)
            && compare(n, &Number::from(accepts_all)) == Some(Ordering::Equal)
        {
            object.remove(keyword);
        }
    }
    if defines("uniqueItems") && object.get("uniqueItems") == Some(&Value::Bool(false)) {
        object.remove("uniqueItems");
    }
    match dialect.exclusive_bounds {
        ExclusiveBounds::BooleanModifier => {
            for keyword in ["exclusiveMinimum", "exclusiveMaximum"] {
                if object.get(keyword) == Some(&Value::Bool(false)) {
                    object.remove(keyword);
                }
            }
        }
        ExclusiveBounds::Numeric => {
            drop_looser_bound(object, "minimum", "exclusiveMinimum", Ordering::Less);
            drop_looser_bound(object, "maximum", "exclusiveMaximum", Ordering::Greater);
        }
    }
}

/// `type` deduplicated and sorted, `integer` dropped beside `number`, one type as a string.
fn canonical_type(object: &mut Map<String, Value>) {
    let Some(Value::Array(types)) = object.get("type") else {
        return;
    };
    let mut names: Vec<&str> = Vec::with_capacity(types.len());
    for value in types {
        match value.as_str() {
            Some(name) if TYPE_ORDER.contains(&name) => names.push(name),
            // Not a type name: the schema is invalid against its metaschema, left alone.
            _ => return,
        }
    }
    if names.is_empty() {
        return;
    }
    let has_number: bool = names.contains(&"number");
    let canonical: Vec<Value> = TYPE_ORDER
        .iter()
        .filter(|name| names.contains(name) && !(**name == "integer" && has_number))
        .map(|name| Value::String((*name).to_string()))
        .collect();
    let value: Value = match <[Value; 1]>::try_from(canonical) {
        Ok([single]) => single,
        Err(canonical) => Value::Array(canonical),
    };
    object.insert("type".to_string(), value);
}

/// `values` sorted by their JSON text, with equal values once.
fn sort_and_dedup(values: &mut Vec<Value>) {
    let mut keyed: Vec<(String, Value)> = values
        .drain(..)
        .map(|value| (value.to_string(), value))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|a, b| a.0 == b.0);
    values.extend(keyed.into_iter().map(|(_, value)| value));
}

/// Of `inclusive` and the numeric `exclusive` bound, drops the one the other implies. `tighter`
/// is how a tighter bound compares: `Less` for lower bounds read as `exclusive >= inclusive`,
/// `Greater` for upper bounds read as `exclusive <= inclusive`.
fn drop_looser_bound(
    object: &mut Map<String, Value>,
    inclusive: &str,
    exclusive: &str,
    tighter: Ordering,
) {
    let (Some(Value::Number(inclusive_bound)), Some(Value::Number(exclusive_bound))) =
        (object.get(inclusive), object.get(exclusive))
    else {
        return;
    };
    match compare(exclusive_bound, inclusive_bound) {
        // The inclusive bound is strictly tighter: every instance it accepts passes the
        // exclusive one.
        Some(ordering) if ordering == tighter => {
            object.remove(exclusive);
        }
        // Equal or looser: the exclusive bound implies the inclusive one.
        Some(_) => {
            object.remove(inclusive);
        }
        None => {}
    }
}

/// The exact order of two JSON numbers, or `None` when it cannot be decided exactly (a
/// decimal that `f64` does not hold exactly, or an integer beyond `f64`'s exact range next to
/// a decimal).
fn compare(a: &Number, b: &Number) -> Option<Ordering> {
    match (integer(a), integer(b)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        _ => exact_f64(a)?.partial_cmp(&exact_f64(b)?),
    }
}

/// `n` as an integer, when it is written as one.
fn integer(n: &Number) -> Option<i128> {
    n.as_i64()
        .map(i128::from)
        .or_else(|| n.as_u64().map(i128::from))
}

/// `n` as an `f64`, when the `f64` is exactly `n`.
fn exact_f64(n: &Number) -> Option<f64> {
    /// Integers up to this magnitude are exact in `f64`.
    const EXACT_INTEGERS: i128 = 1 << 53;
    if let Some(i) = integer(n) {
        #[expect(
            clippy::cast_precision_loss,
            reason = "the magnitude is checked to be exact in f64"
        )]
        return (i.abs() <= EXACT_INTEGERS).then_some(i as f64);
    }
    let f: f64 = n.as_f64()?;
    // `f64` displays the shortest text that reads back as itself, so a decimal held exactly
    // displays as written; any other text was rounded.
    (Number::from_f64(f)?.to_string() == n.to_string()).then_some(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompiledSchema, Schema};
    use serde_json::json;

    /// The canonical form of `root`.
    fn canonicalize(root: &Value) -> Value {
        let schema: Schema = Schema::try_from(root).expect("parse root");
        let registry: Registry<'_> = Registry::builder().build(&schema).expect("build registry");
        registry.canonicalize().expect("canonicalize")
    }

    /// The verdict of each of `instances` against `root`, then against its canonical form.
    fn verdicts(root: &Value, instances: &[Value]) -> (Vec<bool>, Vec<bool>) {
        let compile = |value: &Value| -> Vec<bool> {
            let schema: Schema = Schema::try_from(value).expect("parse");
            let registry: Registry<'_> = Registry::builder().build(&schema).expect("build");
            let compiled: CompiledSchema = CompiledSchema::new(&registry).expect("compile");
            instances.iter().map(|i| compiled.is_valid(i)).collect()
        };
        (compile(root), compile(&canonicalize(root)))
    }

    // --- type ---

    #[test]
    fn type_array_is_deduplicated_and_sorted() {
        let expected: Value = json!({"type": ["null", "object", "string"]});
        let actual: Value = canonicalize(&json!({"type": ["string", "null", "object", "null"]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_beside_number_is_dropped() {
        let expected: Value = json!({"type": ["null", "number"]});
        let actual: Value = canonicalize(&json!({"type": ["integer", "null", "number"]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_array_of_one_becomes_a_string() {
        let expected: Value = json!({"type": "number"});
        let actual: Value = canonicalize(&json!({"type": ["number", "integer"]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_alone_is_kept() {
        let expected: Value = json!({"type": "integer"});
        let actual: Value = canonicalize(&json!({"type": ["integer"]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_and_number_keep_their_verdicts() {
        let instances: Vec<Value> = vec![json!(1), json!(1.5), json!("1"), json!(null)];
        let (expected, actual): (Vec<bool>, Vec<bool>) =
            verdicts(&json!({"type": ["integer", "number"]}), &instances);
        assert_eq!(expected, actual);
    }

    // --- enum, const, required ---

    #[test]
    fn enum_is_deduplicated_and_sorted() {
        let expected: Value = json!({"enum": ["a", 1, 2]});
        let actual: Value = canonicalize(&json!({"enum": ["a", 2, 1, 2]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_of_one_value_becomes_const() {
        let expected: Value = json!({"const": {"a": 1}});
        let actual: Value = canonicalize(&json!({"enum": [{"a": 1}, {"a": 1}]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn enum_of_one_value_beside_const_is_kept() {
        let expected: Value = json!({"const": 1, "enum": [2]});
        let actual: Value = canonicalize(&json!({"const": 1, "enum": [2]}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_enum_of_one_value_stays_enum() {
        let expected: Value =
            json!({"$schema": "http://json-schema.org/draft-04/schema#", "enum": [1]});
        let actual: Value = canonicalize(&expected);
        assert_eq!(expected, actual);
    }

    #[test]
    fn required_is_deduplicated_and_sorted() {
        let expected: Value = json!({"required": ["a", "b"]});
        let actual: Value = canonicalize(&json!({"required": ["b", "a", "b"]}));
        assert_eq!(expected, actual);
    }

    // --- assertions that accept everything ---

    #[test]
    fn assertions_that_accept_everything_are_dropped() {
        let expected: Value = json!({"contains": {"type": "string"}, "type": "array"});
        let actual: Value = canonicalize(&json!({
            "type": "array",
            "contains": {"type": "string", "minLength": 0},
            "minContains": 1,
            "minItems": 0,
            "minProperties": 0.0,
            "uniqueItems": false,
            "required": []
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn assertions_that_reject_something_are_kept() {
        let expected: Value = json!({"minContains": 2, "minLength": 1, "uniqueItems": true});
        let actual: Value = canonicalize(&expected);
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_false_exclusive_bound_is_dropped() {
        let expected: Value =
            json!({"$schema": "http://json-schema.org/draft-04/schema#", "minimum": 1});
        let actual: Value = canonicalize(&json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "minimum": 1,
            "exclusiveMinimum": false
        }));
        assert_eq!(expected, actual);
    }

    // --- bounds ---

    #[test]
    fn exclusive_minimum_at_or_above_minimum_drops_minimum() {
        let expected: Value = json!({"exclusiveMaximum": 5, "exclusiveMinimum": 2});
        let actual: Value = canonicalize(&json!({
            "minimum": 2,
            "exclusiveMinimum": 2,
            "maximum": 7.5,
            "exclusiveMaximum": 5
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn minimum_above_exclusive_minimum_drops_the_exclusive_bound() {
        let expected: Value = json!({"maximum": 1.5, "minimum": 3});
        let actual: Value = canonicalize(&json!({
            "minimum": 3,
            "exclusiveMinimum": 2.5,
            "maximum": 1.5,
            "exclusiveMaximum": 2
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn bounds_keep_their_verdicts() {
        let instances: Vec<Value> = vec![json!(2), json!(2.5), json!(3), json!(5), json!(6)];
        let (expected, actual): (Vec<bool>, Vec<bool>) = verdicts(
            &json!({"minimum": 2, "exclusiveMinimum": 2, "maximum": 5, "exclusiveMaximum": 6}),
            &instances,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_beyond_exact_f64_range_beside_a_decimal_keeps_both_bounds() {
        let expected: Value =
            json!({"exclusiveMinimum": 0.5, "minimum": 9_007_199_254_740_993_i64});
        let actual: Value = canonicalize(&expected);
        assert_eq!(expected, actual);
    }

    #[test]
    fn exact_numbers_compare_exactly() {
        let expected: Vec<Option<Ordering>> = vec![
            Some(Ordering::Less),
            Some(Ordering::Equal),
            Some(Ordering::Greater),
            None,
        ];
        let actual: Vec<Option<Ordering>> = vec![
            compare(&Number::from(-1), &Number::from(u64::MAX)),
            compare(&Number::from(2), &Number::from_f64(2.0).expect("finite")),
            compare(&Number::from_f64(0.5).expect("finite"), &Number::from(0)),
            compare(
                &Number::from(9_007_199_254_740_993_i64),
                &Number::from_f64(0.5).expect("finite"),
            ),
        ];
        assert_eq!(expected, actual);
    }

    // --- boolean schemas ---

    #[test]
    fn schema_left_empty_becomes_true() {
        let expected: Value = json!({"not": true, "properties": {"a": true, "b": true}});
        let actual: Value = canonicalize(&json!({
            "properties": {"a": {}, "b": {"minLength": 0}},
            "not": {}
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn empty_root_becomes_true() {
        let expected: Value = json!(true);
        let actual: Value = canonicalize(&json!({"uniqueItems": false}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn draft4_empty_schema_stays_an_object() {
        let expected: Value = json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "properties": {"a": {}}
        });
        let actual: Value = canonicalize(&json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "properties": {"a": {"minLength": 0}}
        }));
        assert_eq!(expected, actual);
    }

    // --- what is left alone ---

    #[test]
    fn non_schema_values_are_left_alone() {
        let expected: Value = json!({
            "const": {"minLength": 0, "type": ["string", "string"]},
            "default": {},
            "examples": [{}]
        });
        let actual: Value = canonicalize(&expected);
        assert_eq!(expected, actual);
    }

    #[test]
    fn references_and_their_targets_are_kept_in_place() {
        let expected: Value = json!({
            "$defs": {"name": {"type": "string"}, "any": true},
            "properties": {"a": {"$ref": "#/$defs/name"}, "b": {"$ref": "#/$defs/any"}}
        });
        let actual: Value = canonicalize(&json!({
            "$defs": {"name": {"type": ["string"]}, "any": {}},
            "properties": {"a": {"$ref": "#/$defs/name"}, "b": {"$ref": "#/$defs/any"}}
        }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn keywords_of_an_inactive_vocabulary_are_left_alone() {
        let metaschema: Value = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "https://example.com/meta/applicator-only",
            "$vocabulary": {
                "https://json-schema.org/draft/2020-12/vocab/core": true,
                "https://json-schema.org/draft/2020-12/vocab/applicator": true
            }
        });
        let root: Value = json!({
            "$schema": "https://example.com/meta/applicator-only",
            "enum": ["b", "a"],
            "minLength": 0
        });
        let schema: Schema = Schema::try_from(&root).expect("parse root");
        let registry: Registry<'_> = Registry::builder()
            .document("https://example.com/meta/applicator-only", metaschema)
            .build(&schema)
            .expect("build registry");
        let expected: Value = root.clone();
        let actual: Value = registry.canonicalize().expect("canonicalize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn canonical_schema_canonicalizes_to_itself() {
        let canonical: Value = canonicalize(&json!({
            "type": ["integer", "number", "null"],
            "enum": [3, 1, 1],
            "minimum": 0,
            "exclusiveMinimum": 0,
            "items": {"minItems": 0},
            "required": ["b", "a"]
        }));
        let expected: Value = canonical.clone();
        let actual: Value = canonicalize(&canonical);
        assert_eq!(expected, actual);
    }
}
