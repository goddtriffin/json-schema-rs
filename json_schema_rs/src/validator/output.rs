//! The standard output formats of JSON Schema 2020-12 (core §12): `flag`, `basic`, `detailed`
//! and `verbose`, and the annotations of a valid instance.
//!
//! The errors of `basic` and `detailed` are rendered from the error list: every error already
//! carries what an output unit needs (its keyword locations and instance location). `basic` lists every error
//! and, after each `anyOf` / `oneOf` failure, its causes. `detailed` nests the errors by where
//! the schema applied a subschema: each applicator a path goes through (`properties/<name>`,
//! `items`, `allOf/<i>`, `$ref`, ...) is a node at its keyword location and the instance location
//! it applied to, a node with a single child is replaced by that child (§12.4.3), and the causes
//! of an `anyOf` / `oneOf` failure nest under its own unit. Rendering is iterative, so no depth of
//! errors or causes can overflow the stack.
//!
//! Annotations and `verbose` come from the evaluator's trace (see [`CompiledSchema::trace`]),
//! which records every subschema evaluated, with its errors and annotations, as a tree in
//! evaluation order. An annotation is kept when its subschema and every subschema above it
//! passed (2020-12 core §7.7.1.2); `verbose` renders the whole tree.

use super::compiled::CompiledSchema;
use super::error::{ValidationError, ValidationErrorKind};
use super::eval::{TraceKind, TraceRecord, TraceUnit, UnitLocations};
use crate::json_pointer::{self, JsonPointer};
use serde::Serialize;
use serde_json::Value;

/// Which output format to render (2020-12 core §12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputFormat {
    /// The verdict alone: `{"valid": false}`.
    Flag,
    /// A flat list of output units, one per error, or one per annotation of a valid instance
    /// (§12.4.2).
    Basic,
    /// Output units nested by the schema's structure, condensed (§12.4.3).
    Detailed,
    /// Every subschema evaluated, passing or failing, with its errors and annotations: the
    /// uncondensed hierarchy (§12.4.4).
    Verbose,
}

/// A validation result in one of the standard output formats. Serializes to the JSON the
/// specification shows.
///
/// # Example
///
/// ```
/// use json_schema_rs::{CompiledSchema, Output, OutputFormat, Registry, Schema};
/// use serde_json::json;
///
/// let schema: Schema = serde_json::from_value(json!({"minimum": 3})).unwrap();
/// let registry: Registry<'_> = Registry::new(&schema).unwrap();
/// let compiled: CompiledSchema = CompiledSchema::new(&registry).unwrap();
/// let output: Output = compiled.evaluate(&json!(2), OutputFormat::Basic);
/// assert_eq!(
///     json!({"valid": false, "errors": [{
///         "valid": false,
///         "keywordLocation": "/minimum",
///         "instanceLocation": "",
///         "error": "value 2 is below minimum 3"
///     }]}),
///     serde_json::to_value(&output).unwrap()
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Output {
    /// `flag` (§12.4.1).
    Flag(FlagOutput),
    /// `basic` (§12.4.2).
    Basic(BasicOutput),
    /// `detailed` (§12.4.3): the root output unit.
    Detailed(OutputUnit),
    /// `verbose` (§12.4.4): the root output unit.
    Verbose(OutputUnit),
}

/// The `flag` output: the verdict alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlagOutput {
    /// Whether the instance is valid.
    pub valid: bool,
}

/// The `basic` output: the verdict and a flat list of output units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BasicOutput {
    /// Whether the instance is valid.
    pub valid: bool,
    /// One unit per error, in the order of the error list; the causes of an `anyOf` / `oneOf`
    /// failure follow its own unit. Empty (and not serialized) when the instance is valid.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<OutputUnit>,
    /// One unit per annotation of a valid instance, in evaluation order. Empty (and not
    /// serialized) when the instance is invalid or the schema was compiled without annotations
    /// (see [`CompiledSchemaBuilder::annotations`](crate::CompiledSchemaBuilder::annotations)).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<OutputUnit>,
}

/// One output unit (2020-12 core §12.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputUnit {
    /// Whether this part of the evaluation passed (`false` for every error, `true` for every
    /// annotation).
    pub valid: bool,
    /// The keyword's location through the schema as evaluated, a JSON Pointer (§12.3.1).
    pub keyword_location: String,
    /// The keyword's absolute location, when known (§12.3.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_keyword_location: Option<String>,
    /// The instance location, a JSON Pointer (§12.3.3).
    pub instance_location: String,
    /// The error message, for a unit that is an error (a branch unit has none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The annotation's value, for a unit that is an annotation (§12.3.4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<Value>,
    /// Nested units of a failing unit (`detailed` and `verbose`; §12.3.5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<OutputUnit>,
    /// Nested units of a passing unit (`detailed` and `verbose`; §12.3.5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<OutputUnit>,
}

/// An annotation a valid instance collected: the value of an annotation keyword (`title`,
/// `default`, `readOnly`, an unknown keyword, ...) of a subschema that passed at
/// `instance_location`, from [`CompiledSchema::annotations`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    /// The keyword.
    pub keyword: String,
    /// The keyword's location through the schema as evaluated.
    pub keyword_location: JsonPointer,
    /// The keyword's absolute location, when a reference was followed to reach it.
    pub absolute_keyword_location: Option<String>,
    /// The instance location the annotation applies to.
    pub instance_location: JsonPointer,
    /// The annotation's value: the keyword's value.
    pub value: Value,
}

impl Output {
    /// Whether the instance is valid.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Output::Flag(FlagOutput { valid }) | Output::Basic(BasicOutput { valid, .. }) => *valid,
            Output::Detailed(unit) | Output::Verbose(unit) => unit.valid,
        }
    }
}

impl CompiledSchema {
    /// Validates `instance` and renders the result in `format` (2020-12 core §12). `flag`
    /// stops at the first failure, like [`CompiledSchema::is_valid`]; `basic` and `detailed`
    /// collect every error, like [`CompiledSchema::validate`], and for a valid instance list its
    /// annotations when the schema keeps them (see
    /// [`CompiledSchemaBuilder::annotations`](crate::CompiledSchemaBuilder::annotations));
    /// `verbose` evaluates every subschema and reports each one.
    #[must_use]
    pub fn evaluate(&self, instance: &Value, format: OutputFormat) -> Output {
        match format {
            OutputFormat::Flag => Output::Flag(FlagOutput {
                valid: self.is_valid(instance),
            }),
            OutputFormat::Basic => match self.validate(instance) {
                Err(errors) => Output::Basic(BasicOutput {
                    valid: false,
                    errors: basic(&errors),
                    annotations: Vec::new(),
                }),
                Ok(()) => Output::Basic(BasicOutput {
                    valid: true,
                    errors: Vec::new(),
                    annotations: self.kept_trace(instance).map_or_else(Vec::new, |trace| {
                        kept_annotations(&trace)
                            .into_iter()
                            .map(|(_, unit)| annotation_unit(unit))
                            .collect()
                    }),
                }),
            },
            OutputFormat::Detailed => match self.validate(instance) {
                Err(errors) => Output::Detailed(detailed(&errors)),
                Ok(()) => Output::Detailed(self.kept_trace(instance).map_or_else(
                    || OutputUnit {
                        valid: true,
                        ..branch_unit()
                    },
                    |trace| detailed_annotations(&trace),
                )),
            },
            OutputFormat::Verbose => {
                let (_, trace, depth_exceeded): (bool, TraceRecord, Option<ValidationError>) =
                    self.trace(instance);
                // Cut short, the trace says nothing reliable: the guard's error alone.
                if let Some(error) = depth_exceeded {
                    return Output::Verbose(detailed(&[error]));
                }
                // A failing subschema's annotations are dropped (§7.7.1.2): every subschema
                // and error is reported, annotations only where they are kept.
                let kept: Vec<bool> = kept(&trace);
                Output::Verbose(tree(
                    &trace,
                    |i| kept[i] || !matches!(trace.units[i].kind, TraceKind::Annotation { .. }),
                    false,
                ))
            }
        }
    }

    /// The annotations `instance` collects (2020-12 core §7.7), in evaluation order: the
    /// annotation keywords of every subschema that passed and that only passing subschemas led
    /// to. Empty unless the schema was compiled with
    /// [`CompiledSchemaBuilder::annotations`](crate::CompiledSchemaBuilder::annotations).
    ///
    /// # Errors
    ///
    /// Returns the errors of [`CompiledSchema::validate`] when the instance is invalid: an
    /// invalid instance has no annotations.
    pub fn annotations(&self, instance: &Value) -> Result<Vec<Annotation>, Vec<ValidationError>> {
        if self.annotations.is_empty() {
            return self.validate(instance).map(|()| Vec::new());
        }
        let (valid, trace, depth_exceeded): (bool, TraceRecord, Option<ValidationError>) =
            self.trace(instance);
        if let Some(error) = depth_exceeded {
            return Err(vec![error]);
        }
        if !valid {
            return self.validate(instance).map(|()| Vec::new());
        }
        Ok(kept_annotations(&trace)
            .into_iter()
            .filter_map(|(_, unit)| match &unit.kind {
                TraceKind::Annotation {
                    keyword,
                    value,
                    locations,
                } => Some(Annotation {
                    keyword: keyword.to_string(),
                    keyword_location: locations.keyword_location.clone(),
                    absolute_keyword_location: locations.absolute_keyword_location.clone(),
                    instance_location: locations.instance_location.clone(),
                    value: value.clone(),
                }),
                TraceKind::Subschema { .. } | TraceKind::Error(_) => None,
            })
            .collect())
    }

    /// The trace of a valid `instance`, when the schema keeps annotations (without them the
    /// trace of a valid instance holds nothing to report).
    fn kept_trace(&self, instance: &Value) -> Option<TraceRecord> {
        if self.annotations.is_empty() {
            return None;
        }
        let (valid, trace, depth_exceeded): (bool, TraceRecord, Option<ValidationError>) =
            self.trace(instance);
        (valid && depth_exceeded.is_none()).then_some(trace)
    }
}

/// Which units of `trace` are kept: a subschema that passed, annotates, and sits under kept
/// subschemas only; an annotation under a kept subschema. Parents come before their children,
/// so one pass in order decides every unit.
fn kept(trace: &TraceRecord) -> Vec<bool> {
    let mut kept: Vec<bool> = Vec::with_capacity(trace.units.len());
    for unit in &trace.units {
        let parent: bool = unit.parent.is_none_or(|parent| kept[parent]);
        let keep: bool = parent
            && match &unit.kind {
                TraceKind::Subschema {
                    valid, annotates, ..
                } => *valid && *annotates,
                TraceKind::Annotation { .. } => true,
                TraceKind::Error(_) => false,
            };
        kept.push(keep);
    }
    kept
}

/// The kept annotation units of `trace`, with their indexes, in evaluation order.
fn kept_annotations(trace: &TraceRecord) -> Vec<(usize, &TraceUnit)> {
    let kept: Vec<bool> = kept(trace);
    trace
        .units
        .iter()
        .enumerate()
        .filter(|(i, unit)| kept[*i] && matches!(unit.kind, TraceKind::Annotation { .. }))
        .collect()
}

/// A unit at `locations`.
fn located(valid: bool, locations: &UnitLocations) -> OutputUnit {
    OutputUnit {
        valid,
        keyword_location: locations.keyword_location.to_string(),
        absolute_keyword_location: locations.absolute_keyword_location.clone(),
        instance_location: locations.instance_location.to_string(),
        error: None,
        annotation: None,
        errors: Vec::new(),
        annotations: Vec::new(),
    }
}

/// The output unit of one trace unit, without its children.
fn annotation_unit(unit: &TraceUnit) -> OutputUnit {
    match &unit.kind {
        TraceKind::Subschema {
            valid, locations, ..
        } => located(*valid, locations),
        TraceKind::Error(error) => error_unit(error),
        TraceKind::Annotation {
            value, locations, ..
        } => OutputUnit {
            annotation: Some(value.clone()),
            ..located(true, locations)
        },
    }
}

/// The `detailed` output of a valid instance: the kept annotations, nested under the
/// subschemas that led to them, each subschema with a single child replaced by it.
fn detailed_annotations(trace: &TraceRecord) -> OutputUnit {
    let mut needed: Vec<bool> = vec![false; trace.units.len()];
    for (index, _) in kept_annotations(trace) {
        let mut cursor: Option<usize> = Some(index);
        while let Some(i) = cursor {
            if needed[i] {
                break;
            }
            needed[i] = true;
            cursor = trace.units[i].parent;
        }
    }
    if let Some(root) = needed.first_mut() {
        *root = true;
    }
    tree(trace, |i| needed[i], true)
}

/// The units of `trace` that `include` accepts, nested as evaluated; with `condense`, every
/// subschema but the root that has a single child is replaced by it. Built from the last unit
/// to the first (children always follow their parent), without recursion.
fn tree(trace: &TraceRecord, include: impl Fn(usize) -> bool, condense: bool) -> OutputUnit {
    let units: &[TraceUnit] = &trace.units;
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); units.len()];
    for (index, unit) in units.iter().enumerate() {
        if let Some(parent) = unit.parent
            && include(index)
        {
            children[parent].push(index);
        }
    }
    let mut built: Vec<Option<OutputUnit>> = Vec::with_capacity(units.len());
    built.resize_with(units.len(), || None);
    for index in (0..units.len()).rev() {
        if !include(index) {
            continue;
        }
        let mut unit: OutputUnit = annotation_unit(&units[index]);
        let nested: Vec<OutputUnit> = std::mem::take(&mut children[index])
            .into_iter()
            .filter_map(|child| built[child].take())
            .collect();
        let subschema: bool = matches!(units[index].kind, TraceKind::Subschema { .. });
        if condense && index != 0 && subschema && nested.len() == 1 {
            built[index] = nested.into_iter().next();
            continue;
        }
        if unit.valid {
            unit.annotations = nested;
        } else {
            unit.errors = nested;
        }
        built[index] = Some(unit);
    }
    built
        .into_iter()
        .next()
        .flatten()
        .unwrap_or_else(|| OutputUnit {
            valid: true,
            ..branch_unit()
        })
}

/// The unit of one error, without its causes.
fn error_unit(error: &ValidationError) -> OutputUnit {
    OutputUnit {
        valid: false,
        keyword_location: error.keyword_location.to_string(),
        absolute_keyword_location: error.absolute_keyword_location.clone(),
        instance_location: error.instance_location.to_string(),
        error: Some(error.kind.to_string()),
        annotation: None,
        errors: Vec::new(),
        annotations: Vec::new(),
    }
}

/// The causes of `error`, when it is a combinator failure that has them.
fn causes(error: &ValidationError) -> &[ValidationError] {
    match &error.kind {
        ValidationErrorKind::NoSubschemaMatched { causes, .. } => causes,
        _ => &[],
    }
}

/// Every error, each followed by its causes (depth first, in order).
fn basic(errors: &[ValidationError]) -> Vec<OutputUnit> {
    let mut units: Vec<OutputUnit> = Vec::new();
    let mut stack: Vec<&ValidationError> = errors.iter().rev().collect();
    while let Some(error) = stack.pop() {
        units.push(error_unit(error));
        stack.extend(causes(error).iter().rev());
    }
    units
}

/// What one keyword does to the path, for nesting `detailed` output.
enum Applies {
    /// Applies a subschema to the same instance location (`$ref`, `not`); `named`: the
    /// keyword is followed by a name or an index (`allOf/<i>`, `dependentSchemas/<name>`).
    InPlace { named: bool },
    /// Applies a subschema to a member or an item, the instance's next segment; `named`: the
    /// keyword is followed by the property name, the pattern or the index (`properties/<name>`).
    Child { named: bool },
    /// An assertion: no subschema.
    Nothing,
}

/// What the keyword `keyword`, followed by `next` in a keyword location, applies.
fn applies(keyword: &str, next: Option<&str>) -> Applies {
    match keyword {
        "$ref" | "$dynamicRef" | "$recursiveRef" | "not" | "if" | "then" | "else" => {
            Applies::InPlace { named: false }
        }
        "allOf" | "anyOf" | "oneOf" | "dependentSchemas" | "dependencies" => {
            Applies::InPlace { named: true }
        }
        "properties" | "patternProperties" | "prefixItems" => Applies::Child { named: true },
        // `items` with an index is the tuple form of drafts before 2020-12: a keyword never
        // follows a subschema location as a number.
        "items" => Applies::Child {
            named: next.is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        },
        "additionalProperties"
        | "unevaluatedProperties"
        | "additionalItems"
        | "unevaluatedItems"
        | "contains"
        | "propertyNames" => Applies::Child { named: false },
        _ => Applies::Nothing,
    }
}

/// The subschemas an error's path enters before its own keyword: for each, how many segments
/// of the keyword location and of the instance location lead to it.
fn subschemas(error: &ValidationError) -> Vec<(usize, usize)> {
    let keyword: Vec<&str> = error.keyword_location.segments().collect();
    let instance_len: usize = error.instance_location.len();
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut i: usize = 0;
    let mut instance: usize = 0;
    while i < keyword.len() {
        let (step, consumed): (usize, usize) =
            match applies(keyword[i], keyword.get(i + 1).copied()) {
                Applies::InPlace { named } => (1 + usize::from(named), 0),
                Applies::Child { named } => (1 + usize::from(named), 1),
                Applies::Nothing => break,
            };
        i += step;
        instance = (instance + consumed).min(instance_len);
        // A path that ends at the subschema (a `false` schema, or the applicator's own failure)
        // is the error itself, not a node above it.
        if i >= keyword.len() {
            break;
        }
        out.push((i, instance));
    }
    out
}

/// The absolute location of the subschema `keyword_len` segments into `error`'s keyword
/// location: known when the error's absolute location is and no reference lies between that
/// subschema and the error's keyword.
fn branch_absolute(error: &ValidationError, keyword_len: usize) -> Option<String> {
    let absolute: &str = error.absolute_keyword_location.as_deref()?;
    let segments: Vec<&str> = error.keyword_location.segments().collect();
    let last_ref: usize = segments
        .iter()
        .rposition(|s| matches!(*s, "$ref" | "$dynamicRef" | "$recursiveRef"))?;
    if keyword_len <= last_ref {
        return None;
    }
    // The absolute location is the target's URI followed by the segments after the reference.
    let mut tail: String = String::new();
    json_pointer::push_uri_fragment(segments[last_ref + 1..].iter().copied(), &mut tail);
    let base: &str = absolute.strip_suffix(tail.as_str())?;
    let mut uri: String = base.to_string();
    json_pointer::push_uri_fragment(
        segments[last_ref + 1..keyword_len].iter().copied(),
        &mut uri,
    );
    Some(uri)
}

/// A node of the `detailed` tree while it is built: a unit and its children's indexes.
struct TreeNode {
    unit: OutputUnit,
    /// For branch nodes: the keyword and instance locations, to find the node again.
    key: Option<(String, String)>,
    children: Vec<usize>,
}

/// The `detailed` tree of `errors` under a root unit.
fn detailed(errors: &[ValidationError]) -> OutputUnit {
    let mut nodes: Vec<TreeNode> = vec![TreeNode {
        unit: branch_unit(),
        key: None,
        children: Vec::new(),
    }];
    // (parent node, error, how many of its subschemas the parent already stands for)
    let mut pending: Vec<(usize, &ValidationError, usize)> =
        errors.iter().rev().map(|error| (0, error, 0)).collect();
    while let Some((parent, error, skip)) = pending.pop() {
        let mut current: usize = parent;
        for (keyword_len, instance_len) in subschemas(error).into_iter().skip(skip) {
            let keyword_location: String = error.keyword_location.truncate(keyword_len).to_string();
            let instance_location: String =
                error.instance_location.truncate(instance_len).to_string();
            let key: (String, String) = (keyword_location, instance_location);
            let found: Option<usize> = nodes[current]
                .children
                .iter()
                .copied()
                .find(|child| nodes[*child].key.as_ref() == Some(&key));
            current = if let Some(child) = found {
                child
            } else {
                nodes.push(TreeNode {
                    unit: OutputUnit {
                        valid: false,
                        keyword_location: key.0.clone(),
                        absolute_keyword_location: branch_absolute(error, keyword_len),
                        instance_location: key.1.clone(),
                        error: None,
                        annotation: None,
                        errors: Vec::new(),
                        annotations: Vec::new(),
                    },
                    key: Some(key),
                    children: Vec::new(),
                });
                let child: usize = nodes.len() - 1;
                nodes[current].children.push(child);
                child
            };
        }
        nodes.push(TreeNode {
            unit: error_unit(error),
            key: None,
            children: Vec::new(),
        });
        let leaf: usize = nodes.len() - 1;
        nodes[current].children.push(leaf);
        // Causes nest under the failure's own unit; the subschemas above it are already there.
        let depth: usize = subschemas(error).len();
        pending.extend(causes(error).iter().rev().map(|cause| (leaf, cause, depth)));
    }
    assemble(nodes)
}

/// Turns the arena into nested units, children first, replacing every branch node that has a
/// single child by that child.
fn assemble(nodes: Vec<TreeNode>) -> OutputUnit {
    // Children always come after their parent in the arena, so building from the last node to
    // the first finishes every child before its parent.
    let mut built: Vec<Option<OutputUnit>> = Vec::with_capacity(nodes.len());
    built.resize_with(nodes.len(), || None);
    let mut nodes: Vec<TreeNode> = nodes;
    for index in (0..nodes.len()).rev() {
        let children: Vec<usize> = std::mem::take(&mut nodes[index].children);
        let mut units: Vec<OutputUnit> = children
            .into_iter()
            .filter_map(|child| built[child].take())
            .collect();
        let mut unit: OutputUnit = std::mem::replace(&mut nodes[index].unit, branch_unit());
        let is_branch: bool = unit.error.is_none();
        if index != 0 && is_branch && units.len() == 1 {
            built[index] = units.pop();
            continue;
        }
        unit.errors = units;
        built[index] = Some(unit);
    }
    built
        .into_iter()
        .next()
        .flatten()
        .unwrap_or_else(branch_unit)
}

/// A failing unit at the roots of the schema and the instance, without a message.
fn branch_unit() -> OutputUnit {
    OutputUnit {
        valid: false,
        keyword_location: String::new(),
        absolute_keyword_location: None,
        instance_location: String::new(),
        error: None,
        annotation: None,
        errors: Vec::new(),
        annotations: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::Annotation;
    use super::{Output, OutputFormat};
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::{Registry, Schema};
    use crate::validator::{CompiledSchema, ValidationError, ValidationErrorKind};
    use serde_json::{Value, json};

    fn compile(schema: &Value) -> CompiledSchema {
        let schema: Schema = serde_json::from_value(schema.clone()).expect("test schema parses");
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .expect("test registry builds");
        CompiledSchema::new(&registry).expect("test schema compiles")
    }

    fn compile_annotating(schema: &Value) -> CompiledSchema {
        let schema: Schema = serde_json::from_value(schema.clone()).expect("test schema parses");
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .expect("test registry builds");
        CompiledSchema::builder()
            .annotations(true)
            .build(&registry)
            .expect("test schema compiles")
    }

    fn render_annotating(schema: &Value, instance: &Value, format: OutputFormat) -> Value {
        let output: Output = compile_annotating(schema).evaluate(instance, format);
        serde_json::to_value(&output).expect("output serializes")
    }

    fn render(schema: &Value, instance: &Value, format: OutputFormat) -> Value {
        let output: Output = compile(schema).evaluate(instance, format);
        serde_json::to_value(&output).expect("output serializes")
    }

    fn ptr(pointer: &str) -> JsonPointer {
        JsonPointer::try_from(pointer).expect("valid JSON Pointer")
    }

    /// The schema of the specification's output examples (2020-12 core §12.4).
    fn polygon() -> Value {
        json!({
            "$id": "https://example.com/polygon",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$defs": {
                "point": {
                    "type": "object",
                    "properties": {
                        "x": {"type": "number"},
                        "y": {"type": "number"}
                    },
                    "additionalProperties": false,
                    "required": ["x", "y"]
                }
            },
            "type": "array",
            "items": {"$ref": "#/$defs/point"},
            "minItems": 3
        })
    }

    /// The instance of the specification's output examples.
    fn polygon_instance() -> Value {
        json!([{"x": 2.5, "y": 1.3}, {"x": 1, "z": 6.7}])
    }

    #[test]
    fn flag_reports_invalid() {
        let expected: Value = json!({"valid": false});
        let actual: Value = render(&polygon(), &polygon_instance(), OutputFormat::Flag);
        assert_eq!(expected, actual);
    }

    #[test]
    fn flag_reports_valid() {
        let expected: Value = json!({"valid": true});
        let actual: Value = render(&json!({"minItems": 1}), &json!([1]), OutputFormat::Flag);
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_lists_every_error_of_the_specification_example() {
        let expected: Value = json!({
            "valid": false,
            "errors": [
                {
                    "valid": false,
                    "keywordLocation": "/minItems",
                    "instanceLocation": "",
                    "error": "array has 2 item(s), minimum is 3"
                },
                {
                    "valid": false,
                    "keywordLocation": "/items/$ref/required",
                    "absoluteKeywordLocation": "https://example.com/polygon#/$defs/point/required",
                    "instanceLocation": "/1",
                    "error": "missing required property \"y\""
                },
                {
                    "valid": false,
                    "keywordLocation": "/items/$ref/additionalProperties",
                    "absoluteKeywordLocation":
                        "https://example.com/polygon#/$defs/point/additionalProperties",
                    "instanceLocation": "/1/z",
                    "error": "additional property \"z\" not allowed"
                }
            ]
        });
        let actual: Value = render(&polygon(), &polygon_instance(), OutputFormat::Basic);
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_reports_valid_without_errors() {
        let expected: Value = json!({"valid": true});
        let actual: Value = render(&json!({"minItems": 1}), &json!([1]), OutputFormat::Basic);
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_nests_the_specification_example_by_schema_structure() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [
                {
                    "valid": false,
                    "keywordLocation": "/minItems",
                    "instanceLocation": "",
                    "error": "array has 2 item(s), minimum is 3"
                },
                {
                    "valid": false,
                    "keywordLocation": "/items/$ref",
                    "absoluteKeywordLocation": "https://example.com/polygon#/$defs/point",
                    "instanceLocation": "/1",
                    "errors": [
                        {
                            "valid": false,
                            "keywordLocation": "/items/$ref/required",
                            "absoluteKeywordLocation":
                                "https://example.com/polygon#/$defs/point/required",
                            "instanceLocation": "/1",
                            "error": "missing required property \"y\""
                        },
                        {
                            "valid": false,
                            "keywordLocation": "/items/$ref/additionalProperties",
                            "absoluteKeywordLocation":
                                "https://example.com/polygon#/$defs/point/additionalProperties",
                            "instanceLocation": "/1/z",
                            "error": "additional property \"z\" not allowed"
                        }
                    ]
                }
            ]
        });
        let actual: Value = render(&polygon(), &polygon_instance(), OutputFormat::Detailed);
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_reports_valid_root_unit() {
        let expected: Value = json!({"valid": true, "keywordLocation": "", "instanceLocation": ""});
        let actual: Value = render(&json!({"minItems": 1}), &json!([1]), OutputFormat::Detailed);
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_keeps_root_around_single_error() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/properties/a/minimum",
                "instanceLocation": "/a",
                "error": "value 1 is below minimum 2"
            }]
        });
        let actual: Value = render(
            &json!({"properties": {"a": {"minimum": 2}}}),
            &json!({"a": 1}),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_groups_errors_of_one_member() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/properties/a",
                "instanceLocation": "/a",
                "errors": [
                    {
                        "valid": false,
                        "keywordLocation": "/properties/a/minimum",
                        "instanceLocation": "/a",
                        "error": "value 1 is below minimum 2"
                    },
                    {
                        "valid": false,
                        "keywordLocation": "/properties/a/multipleOf",
                        "instanceLocation": "/a",
                        "error": "value 1 is not a multiple of 3"
                    }
                ]
            }]
        });
        let actual: Value = render(
            &json!({"properties": {"a": {"minimum": 2, "multipleOf": 3}}}),
            &json!({"a": 1}),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_nests_causes_under_their_combinator() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/anyOf",
                "instanceLocation": "",
                "error": "instance does not match any of the 2 subschema(s)",
                "errors": [
                    {
                        "valid": false,
                        "keywordLocation": "/anyOf/0/type",
                        "instanceLocation": "",
                        "error": "expected string, got number"
                    },
                    {
                        "valid": false,
                        "keywordLocation": "/anyOf/1/minimum",
                        "instanceLocation": "",
                        "error": "value 1 is below minimum 5"
                    }
                ]
            }]
        });
        let actual: Value = render(
            &json!({"anyOf": [{"type": "string"}, {"minimum": 5}]}),
            &json!(1),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_lists_causes_after_their_combinator() {
        let expected: Value = json!({
            "valid": false,
            "errors": [
                {
                    "valid": false,
                    "keywordLocation": "/oneOf",
                    "instanceLocation": "",
                    "error": "instance does not match any of the 2 subschema(s)"
                },
                {
                    "valid": false,
                    "keywordLocation": "/oneOf/0/type",
                    "instanceLocation": "",
                    "error": "expected string, got number"
                },
                {
                    "valid": false,
                    "keywordLocation": "/oneOf/1/minimum",
                    "instanceLocation": "",
                    "error": "value 1 is below minimum 5"
                }
            ]
        });
        let actual: Value = render(
            &json!({"oneOf": [{"type": "string"}, {"minimum": 5}]}),
            &json!(1),
            OutputFormat::Basic,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_branch_inside_a_cause_nests_under_it() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/anyOf",
                "instanceLocation": "",
                "error": "instance does not match any of the 2 subschema(s)",
                "errors": [
                    {
                        "valid": false,
                        "keywordLocation": "/anyOf/0/type",
                        "instanceLocation": "",
                        "error": "expected string, got object"
                    },
                    {
                        "valid": false,
                        "keywordLocation": "/anyOf/1/properties/a",
                        "instanceLocation": "/a",
                        "errors": [
                            {
                                "valid": false,
                                "keywordLocation": "/anyOf/1/properties/a/minimum",
                                "instanceLocation": "/a",
                                "error": "value 1 is below minimum 2"
                            },
                            {
                                "valid": false,
                                "keywordLocation": "/anyOf/1/properties/a/maximum",
                                "instanceLocation": "/a",
                                "error": "value 1 is above maximum 0"
                            }
                        ]
                    }
                ]
            }]
        });
        let actual: Value = render(
            &json!({"anyOf": [{"type": "string"}, {"properties": {"a": {"minimum": 2, "maximum": 0}}}]}),
            &json!({"a": 1}),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_tuple_items_consume_the_index() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/items/1",
                "instanceLocation": "/1",
                "errors": [
                    {
                        "valid": false,
                        "keywordLocation": "/items/1/minimum",
                        "instanceLocation": "/1",
                        "error": "value 1 is below minimum 2"
                    },
                    {
                        "valid": false,
                        "keywordLocation": "/items/1/maximum",
                        "instanceLocation": "/1",
                        "error": "value 1 is above maximum 0"
                    }
                ]
            }]
        });
        let actual: Value = render(
            &json!({
                "$schema": "http://json-schema.org/draft-07/schema#",
                "items": [true, {"minimum": 2, "maximum": 0}]
            }),
            &json!([0, 1]),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    /// A schema with annotations in a property, behind a reference, in a passing and a
    /// failing `anyOf` branch, and under `not`.
    fn annotated() -> Value {
        json!({
            "$id": "https://example.com/annotated",
            "title": "Root",
            "properties": {
                "a": {"title": "A", "x-widget": "slider", "minimum": 0},
                "b": {"$ref": "#/$defs/b"}
            },
            "$defs": {"b": {"description": "B"}},
            "anyOf": [{"title": "Yes"}, {"title": "No", "type": "string"}],
            "not": {"title": "Hidden", "type": "string"}
        })
    }

    #[test]
    fn basic_lists_the_annotations_of_a_valid_instance() {
        let expected: Value = json!({
            "valid": true,
            "annotations": [
                {
                    "valid": true,
                    "keywordLocation": "/title",
                    "instanceLocation": "",
                    "annotation": "Root"
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/a/title",
                    "instanceLocation": "/a",
                    "annotation": "A"
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/a/x-widget",
                    "instanceLocation": "/a",
                    "annotation": "slider"
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/b/$ref/description",
                    "absoluteKeywordLocation": "https://example.com/annotated#/$defs/b/description",
                    "instanceLocation": "/b",
                    "annotation": "B"
                },
                {
                    "valid": true,
                    "keywordLocation": "/anyOf/0/title",
                    "instanceLocation": "",
                    "annotation": "Yes"
                }
            ]
        });
        let actual: Value = render_annotating(
            &annotated(),
            &json!({"a": 2, "b": true}),
            OutputFormat::Basic,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_lists_no_annotations_without_the_setting() {
        let expected: Value = json!({"valid": true});
        let actual: Value = render(
            &annotated(),
            &json!({"a": 2, "b": true}),
            OutputFormat::Basic,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_lists_only_errors_of_an_invalid_instance() {
        let expected: Value = json!({
            "valid": false,
            "errors": [{
                "valid": false,
                "keywordLocation": "/properties/a/minimum",
                "instanceLocation": "/a",
                "error": "value -1 is below minimum 0"
            }]
        });
        let actual: Value = render_annotating(&annotated(), &json!({"a": -1}), OutputFormat::Basic);
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_nests_the_annotations_of_a_valid_instance() {
        let expected: Value = json!({
            "valid": true,
            "keywordLocation": "",
            "instanceLocation": "",
            "annotations": [
                {
                    "valid": true,
                    "keywordLocation": "/title",
                    "instanceLocation": "",
                    "annotation": "Root"
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/a",
                    "instanceLocation": "/a",
                    "annotations": [
                        {
                            "valid": true,
                            "keywordLocation": "/properties/a/title",
                            "instanceLocation": "/a",
                            "annotation": "A"
                        },
                        {
                            "valid": true,
                            "keywordLocation": "/properties/a/x-widget",
                            "instanceLocation": "/a",
                            "annotation": "slider"
                        }
                    ]
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/b/$ref/description",
                    "absoluteKeywordLocation": "https://example.com/annotated#/$defs/b/description",
                    "instanceLocation": "/b",
                    "annotation": "B"
                },
                {
                    "valid": true,
                    "keywordLocation": "/anyOf/0/title",
                    "instanceLocation": "",
                    "annotation": "Yes"
                }
            ]
        });
        let actual: Value = render_annotating(
            &annotated(),
            &json!({"a": 2, "b": true}),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn detailed_without_the_setting_reports_the_valid_root_alone() {
        let expected: Value = json!({"valid": true, "keywordLocation": "", "instanceLocation": ""});
        let actual: Value = render(
            &annotated(),
            &json!({"a": 2, "b": true}),
            OutputFormat::Detailed,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn verbose_reports_every_subschema_and_drops_failed_annotations() {
        let expected: Value = json!({
            "valid": true,
            "keywordLocation": "",
            "instanceLocation": "",
            "annotations": [
                {
                    "valid": true,
                    "keywordLocation": "/title",
                    "instanceLocation": "",
                    "annotation": "Root"
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/a",
                    "instanceLocation": "/a",
                    "annotations": [
                        {
                            "valid": true,
                            "keywordLocation": "/properties/a/title",
                            "instanceLocation": "/a",
                            "annotation": "A"
                        },
                        {
                            "valid": true,
                            "keywordLocation": "/properties/a/x-widget",
                            "instanceLocation": "/a",
                            "annotation": "slider"
                        }
                    ]
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/b",
                    "instanceLocation": "/b",
                    "annotations": [{
                        "valid": true,
                        "keywordLocation": "/properties/b/$ref",
                        "absoluteKeywordLocation": "https://example.com/annotated#/$defs/b",
                        "instanceLocation": "/b",
                        "annotations": [{
                            "valid": true,
                            "keywordLocation": "/properties/b/$ref/description",
                            "absoluteKeywordLocation":
                                "https://example.com/annotated#/$defs/b/description",
                            "instanceLocation": "/b",
                            "annotation": "B"
                        }]
                    }]
                },
                {
                    "valid": true,
                    "keywordLocation": "/anyOf/0",
                    "instanceLocation": "",
                    "annotations": [{
                        "valid": true,
                        "keywordLocation": "/anyOf/0/title",
                        "instanceLocation": "",
                        "annotation": "Yes"
                    }]
                },
                {
                    "valid": false,
                    "keywordLocation": "/anyOf/1",
                    "instanceLocation": "",
                    "errors": [{
                        "valid": false,
                        "keywordLocation": "/anyOf/1/type",
                        "instanceLocation": "",
                        "error": "expected string, got object"
                    }]
                },
                {
                    "valid": false,
                    "keywordLocation": "/not",
                    "instanceLocation": "",
                    "errors": [{
                        "valid": false,
                        "keywordLocation": "/not/type",
                        "instanceLocation": "",
                        "error": "expected string, got object"
                    }]
                }
            ]
        });
        let actual: Value = render_annotating(
            &annotated(),
            &json!({"a": 2, "b": true}),
            OutputFormat::Verbose,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn verbose_reports_the_failing_path_of_an_invalid_instance() {
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [
                {
                    "valid": false,
                    "keywordLocation": "/properties/a",
                    "instanceLocation": "/a",
                    "errors": [{
                        "valid": false,
                        "keywordLocation": "/properties/a/minimum",
                        "instanceLocation": "/a",
                        "error": "value 1 is below minimum 3"
                    }]
                },
                {
                    "valid": true,
                    "keywordLocation": "/properties/b",
                    "instanceLocation": "/b"
                }
            ]
        });
        let actual: Value = render(
            &json!({"title": "T", "properties": {"a": {"minimum": 3}, "b": {}}}),
            &json!({"a": 1, "b": 2}),
            OutputFormat::Verbose,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn verbose_past_the_depth_limit_reports_the_guard_error() {
        let schema: Schema = serde_json::from_value(json!({"items": {"items": {"items": {}}}}))
            .expect("test schema parses");
        let registry: Registry<'static> = Registry::builder()
            .build_owned(schema)
            .expect("test registry builds");
        let compiled: CompiledSchema = CompiledSchema::builder()
            .max_depth(Some(2))
            .build(&registry)
            .expect("test schema compiles");
        let expected: Value = json!({
            "valid": false,
            "keywordLocation": "",
            "instanceLocation": "",
            "errors": [{
                "valid": false,
                "keywordLocation": "/items/items",
                "instanceLocation": "/0/0",
                "error": "evaluation nested deeper than 2 levels"
            }]
        });
        let actual: Value =
            serde_json::to_value(compiled.evaluate(&json!([[[1]]]), OutputFormat::Verbose))
                .expect("output serializes");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_lists_every_kept_annotation() {
        let expected: Vec<Annotation> = vec![
            Annotation {
                keyword: "title".to_string(),
                keyword_location: ptr("/title"),
                absolute_keyword_location: None,
                instance_location: ptr(""),
                value: json!("Root"),
            },
            Annotation {
                keyword: "title".to_string(),
                keyword_location: ptr("/properties/a/title"),
                absolute_keyword_location: None,
                instance_location: ptr("/a"),
                value: json!("A"),
            },
            Annotation {
                keyword: "x-widget".to_string(),
                keyword_location: ptr("/properties/a/x-widget"),
                absolute_keyword_location: None,
                instance_location: ptr("/a"),
                value: json!("slider"),
            },
            Annotation {
                keyword: "description".to_string(),
                keyword_location: ptr("/properties/b/$ref/description"),
                absolute_keyword_location: Some(
                    "https://example.com/annotated#/$defs/b/description".to_string(),
                ),
                instance_location: ptr("/b"),
                value: json!("B"),
            },
            Annotation {
                keyword: "title".to_string(),
                keyword_location: ptr("/anyOf/0/title"),
                absolute_keyword_location: None,
                instance_location: ptr(""),
                value: json!("Yes"),
            },
        ];
        let actual: Vec<Annotation> = compile_annotating(&annotated())
            .annotations(&json!({"a": 2, "b": true}))
            .expect("instance is valid");
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_of_an_invalid_instance_are_its_errors() {
        let expected: Result<Vec<Annotation>, Vec<ValidationError>> =
            Err(vec![ValidationError::new(
                ValidationErrorKind::BelowMinimum {
                    minimum: json!(0).as_number().cloned().expect("a number"),
                    actual: json!(-1).as_number().cloned().expect("a number"),
                },
                ptr("/a"),
                ptr("/properties/a/minimum"),
            )]);
        let actual: Result<Vec<Annotation>, Vec<ValidationError>> =
            compile_annotating(&annotated()).annotations(&json!({"a": -1}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_without_the_setting_are_empty() {
        let expected: Result<Vec<Annotation>, Vec<ValidationError>> = Ok(Vec::new());
        let actual: Result<Vec<Annotation>, Vec<ValidationError>> =
            compile(&annotated()).annotations(&json!({"a": 2, "b": true}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn annotations_only_annotate_strings_with_content_keywords() {
        let expected: Vec<Annotation> = Vec::new();
        let actual: Vec<Annotation> =
            compile_annotating(&json!({"contentMediaType": "application/json"}))
                .annotations(&json!(42))
                .expect("instance is valid");
        assert_eq!(expected, actual);
    }

    #[test]
    fn output_is_valid_reads_the_verdict() {
        let expected: (bool, bool) = (true, false);
        let compiled: CompiledSchema = compile(&json!({"minimum": 1}));
        let actual: (bool, bool) = (
            compiled
                .evaluate(&json!(1), OutputFormat::Detailed)
                .is_valid(),
            compiled
                .evaluate(&json!(0), OutputFormat::Detailed)
                .is_valid(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn absolute_location_is_percent_encoded() {
        let expected: Value = json!({
            "valid": false,
            "errors": [{
                "valid": false,
                "keywordLocation": "/$ref/properties/a b/minimum",
                "absoluteKeywordLocation": "https://example.com/s#/$defs/d%25/properties/a%20b/minimum",
                "instanceLocation": "/a b",
                "error": "value 1 is below minimum 2"
            }]
        });
        let actual: Value = render(
            &json!({
                "$id": "https://example.com/s",
                "$ref": "#/$defs/d%25",
                "$defs": {"d%": {"properties": {"a b": {"minimum": 2}}}}
            }),
            &json!({"a b": 1}),
            OutputFormat::Basic,
        );
        assert_eq!(expected, actual);
    }
}
