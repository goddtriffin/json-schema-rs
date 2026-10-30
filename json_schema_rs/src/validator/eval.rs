//! Evaluates a [`CompiledSchema`] against an instance.
//!
//! One evaluator serves two modes, chosen by the `COLLECT` const parameter: the verdict mode
//! (`is_valid`) stops at the first failing keyword and allocates nothing for errors; the collect
//! mode (`validate`) evaluates every keyword and records one [`ValidationError`] per problem.
//! Combinator branches (`anyOf`, `oneOf`, `not`, `if`, `contains`, `propertyNames`) always run in
//! the verdict mode, since only their outcome matters.
//!
//! The evaluator recurses (one native frame group per nested schema), guarded by a depth limit
//! that returns [`ValidationError::DepthLimitExceeded`] before the stack can overflow. An
//! in-place loop (a `$ref` re-entered at the same instance location without consuming any of
//! the instance) fails that branch with [`ValidationError::RefCycle`].

use super::compiled::{CompiledSchema, Keyword, KnownProperties, Node, NodeId};
use super::error::{ValidationError, ValidationResult};
use crate::json_pointer::JsonPointer;
use crate::json_schema::{JsonType, Scope, TypeSet};
use crate::number::{self, JsonEqKey};
use serde_json::{Map, Number, Value};
use std::cmp::Ordering;
use std::collections::HashSet;

/// How deeply schemas may nest during one evaluation. Each level costs a few native stack
/// frames; the limit keeps a debug build of the evaluator inside a 2 MiB thread stack (the
/// default for spawned threads) with a wide margin. Measured by
/// `depth_guard_fits_two_mib_thread` in this module's tests.
pub(crate) const DEPTH_LIMIT: usize = 512;

/// An instance location: a linked list on the native stack, encoded as a [`JsonPointer`] only
/// when an error is created.
#[derive(Clone, Copy)]
pub(crate) struct Location<'a> {
    parent: Option<&'a Location<'a>>,
    step: Step<'a>,
}

#[derive(Clone, Copy)]
enum Step<'a> {
    Root,
    Key(&'a str),
    Index(usize),
}

impl<'a> Location<'a> {
    pub(crate) const ROOT: Location<'static> = Location {
        parent: None,
        step: Step::Root,
    };

    fn key(&'a self, key: &'a str) -> Location<'a> {
        Location {
            parent: Some(self),
            step: Step::Key(key),
        }
    }

    fn index(&'a self, index: usize) -> Location<'a> {
        Location {
            parent: Some(self),
            step: Step::Index(index),
        }
    }

    fn pointer(&self) -> JsonPointer {
        let mut segments: Vec<String> = Vec::new();
        let mut cursor: Option<&Location<'_>> = Some(self);
        while let Some(location) = cursor {
            match location.step {
                Step::Root => {}
                Step::Key(key) => segments.push(key.to_string()),
                Step::Index(index) => segments.push(index.to_string()),
            }
            cursor = location.parent;
        }
        segments.reverse();
        JsonPointer::from(segments)
    }
}

/// Which members (by position in the object) or items (by index) of the current instance some
/// keyword evaluated: the annotations `unevaluatedProperties` / `unevaluatedItems` read.
#[derive(Default)]
pub(crate) struct Evaluated {
    all: bool,
    bits: Vec<u64>,
}

impl Evaluated {
    fn mark(&mut self, index: usize) {
        let word: usize = index / 64;
        if self.bits.len() <= word {
            self.bits.resize(word + 1, 0);
        }
        self.bits[word] |= 1 << (index % 64);
    }

    const fn mark_all(&mut self) {
        self.all = true;
    }

    fn contains(&self, index: usize) -> bool {
        self.all
            || self
                .bits
                .get(index / 64)
                .is_some_and(|word| word >> (index % 64) & 1 == 1)
    }

    fn merge(&mut self, other: &Evaluated) {
        self.all |= other.all;
        if self.bits.len() < other.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        for (mine, theirs) in self.bits.iter_mut().zip(&other.bits) {
            *mine |= theirs;
        }
    }
}

/// Per-evaluation state.
#[derive(Default)]
struct Context {
    errors: Vec<ValidationError>,
    depth: usize,
    /// `$ref` targets entered, with the instance they were entered at (by address): the
    /// in-place loop guard.
    refs: Vec<(NodeId, *const Value)>,
    /// Schema resources entered, outermost first (only when a dynamic reference needs them).
    dynamic_scope: Vec<Scope>,
}

/// Returns the JSON type name of the value for use in "got" error messages.
fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Serializes a JSON value to a string for error display. Never truncates.
fn value_to_display_string(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "?".to_string())
}

/// The error for an instance that `type` rejects: the single-type variant for a set of one,
/// [`ValidationError::ExpectedType`] otherwise.
fn type_mismatch(
    expected: TypeSet,
    instance: &Value,
    instance_path: JsonPointer,
) -> ValidationError {
    let got: String = json_type_name(instance).to_string();
    match expected.single() {
        Some(JsonType::Object) => ValidationError::ExpectedObject { instance_path, got },
        Some(JsonType::String) => ValidationError::ExpectedString { instance_path, got },
        Some(JsonType::Integer) => ValidationError::ExpectedInteger { instance_path, got },
        Some(JsonType::Number) => ValidationError::ExpectedNumber { instance_path, got },
        Some(JsonType::Array) => ValidationError::ExpectedArray { instance_path, got },
        Some(JsonType::Boolean) => ValidationError::ExpectedBoolean { instance_path, got },
        Some(JsonType::Null) => ValidationError::ExpectedNull { instance_path, got },
        None => ValidationError::ExpectedType {
            instance_path,
            expected,
            got,
        },
    }
}

/// Arrays up to this length are checked pairwise for `uniqueItems`; longer ones through a hash
/// set (the pairwise scan does no hashing or allocation, which wins on short arrays; jsonschema-rs
/// switches at the same size).
const UNIQUE_ITEMS_PAIRWISE_MAX: usize = 15;

/// The first element of `items` equal (by [`number::json_equal`]) to an earlier one.
fn first_duplicate(items: &[Value]) -> Option<&Value> {
    if items.len() <= UNIQUE_ITEMS_PAIRWISE_MAX {
        return items.iter().enumerate().find_map(|(j, later)| {
            items[..j]
                .iter()
                .any(|earlier| number::json_equal(earlier, later))
                .then_some(later)
        });
    }
    let mut seen: HashSet<JsonEqKey<'_>> = HashSet::with_capacity(items.len());
    items.iter().find(|item| !seen.insert(JsonEqKey(item)))
}

/// Records `$error` in collect mode and evaluates to `false` (the keyword failed).
macro_rules! fail {
    ($collect:expr, $cx:expr, $error:expr) => {{
        if $collect {
            $cx.errors.push($error);
        }
        false
    }};
}

impl CompiledSchema {
    /// Whether `instance` is valid against the schema. Stops at the first failing keyword and
    /// allocates nothing for errors, so it is the fastest check; equal to
    /// `self.validate(instance).is_ok()`.
    #[must_use]
    pub fn is_valid(&self, instance: &Value) -> bool {
        let mut cx: Context = Context::default();
        self.eval::<false>(self.root, instance, &Location::ROOT, &mut cx, None)
    }

    /// Validates `instance`, collecting every error (one per problem, see [`ValidationError`]).
    ///
    /// # Errors
    ///
    /// Returns `Err(errors)` when the instance does not conform to the schema.
    pub fn validate(&self, instance: &Value) -> ValidationResult {
        let mut cx: Context = Context::default();
        let valid: bool = self.eval::<true>(self.root, instance, &Location::ROOT, &mut cx, None);
        if valid && cx.errors.is_empty() {
            Ok(())
        } else {
            Err(cx.errors)
        }
    }

    fn is_false(&self, id: NodeId) -> bool {
        matches!(*self.node(id).keywords, [Keyword::False])
    }

    /// Evaluates node `id` against `instance` at `location`, adding the members / items it
    /// evaluates to `evaluated` when asked to.
    fn eval<const COLLECT: bool>(
        &self,
        id: NodeId,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let node: &Node = self.node(id);
        if node.keywords.is_empty() {
            return true;
        }
        if cx.depth >= DEPTH_LIMIT {
            return fail!(
                COLLECT,
                cx,
                ValidationError::DepthLimitExceeded {
                    instance_path: location.pointer(),
                    limit: DEPTH_LIMIT,
                }
            );
        }
        cx.depth += 1;
        let entered: bool = self.uses_dynamic_scope && cx.dynamic_scope.last() != Some(&node.scope);
        if entered {
            cx.dynamic_scope.push(node.scope);
        }
        let valid: bool = if node.has_unevaluated {
            // `unevaluated*` reads every sibling's annotations, so collect them here.
            let mut local: Evaluated = Evaluated::default();
            let valid: bool =
                self.keywords::<COLLECT>(node, instance, location, cx, Some(&mut local));
            if valid && let Some(parent) = evaluated {
                parent.merge(&local);
            }
            valid
        } else {
            self.keywords::<COLLECT>(node, instance, location, cx, evaluated)
        };
        if entered {
            cx.dynamic_scope.pop();
        }
        cx.depth -= 1;
        valid
    }

    fn keywords<const COLLECT: bool>(
        &self,
        node: &Node,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let mut valid: bool = true;
        for keyword in &node.keywords {
            let ok: bool = match instance {
                Value::Object(object) => self.object_keyword::<COLLECT>(
                    keyword,
                    instance,
                    object,
                    location,
                    cx,
                    evaluated.as_deref_mut(),
                ),
                Value::Array(items) => self.array_keyword::<COLLECT>(
                    keyword,
                    instance,
                    items,
                    location,
                    cx,
                    evaluated.as_deref_mut(),
                ),
                _ => self.keyword::<COLLECT>(
                    keyword,
                    instance,
                    location,
                    cx,
                    evaluated.as_deref_mut(),
                ),
            };
            if !ok {
                valid = false;
                if !COLLECT {
                    return false;
                }
            }
        }
        valid
    }

    /// Evaluates a subschema at the same instance location; its annotations count only when it
    /// passes.
    fn in_place<const COLLECT: bool>(
        &self,
        id: NodeId,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match evaluated {
            None => self.eval::<COLLECT>(id, instance, location, cx, None),
            Some(parent) => {
                let mut sub: Evaluated = Evaluated::default();
                let ok: bool = self.eval::<COLLECT>(id, instance, location, cx, Some(&mut sub));
                if ok {
                    parent.merge(&sub);
                }
                ok
            }
        }
    }

    /// Follows a reference to `target` in place, failing with [`ValidationError::RefCycle`] when
    /// `target` is already being evaluated at this instance location.
    fn follow<const COLLECT: bool>(
        &self,
        target: NodeId,
        ref_str: &str,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let at: *const Value = instance;
        // Entries for this instance location are the newest ones: a child location's entries
        // are popped before its parent is evaluated further.
        let looping: bool = cx
            .refs
            .iter()
            .rev()
            .take_while(|(_, p)| std::ptr::eq(*p, at))
            .any(|(node, _)| *node == target);
        if looping {
            return fail!(
                COLLECT,
                cx,
                ValidationError::RefCycle {
                    instance_path: location.pointer(),
                    ref_str: ref_str.to_string(),
                }
            );
        }
        cx.refs.push((target, at));
        let ok: bool = self.in_place::<COLLECT>(target, instance, location, cx, evaluated);
        cx.refs.pop();
        ok
    }

    /// Keywords that apply to every instance, and the scalar assertions.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per keyword; the closed enum is matched exhaustively"
    )]
    fn keyword<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match keyword {
            Keyword::False => fail!(
                COLLECT,
                cx,
                ValidationError::FalseSchema {
                    instance_path: location.pointer(),
                }
            ),
            Keyword::Type {
                types,
                integer_semantics,
            } => {
                types.matches(instance, *integer_semantics)
                    || fail!(
                        COLLECT,
                        cx,
                        type_mismatch(*types, instance, location.pointer())
                    )
            }
            Keyword::Const(expected) => {
                number::json_equal(instance, expected)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotConst {
                            instance_path: location.pointer(),
                            expected: value_to_display_string(expected),
                            actual: value_to_display_string(instance),
                        }
                    )
            }
            Keyword::Enum(allowed) => {
                allowed.iter().any(|a| number::json_equal(a, instance))
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotInEnum {
                            instance_path: location.pointer(),
                            invalid_value: value_to_display_string(instance),
                            allowed: allowed.iter().map(value_to_display_string).collect(),
                        }
                    )
            }
            Keyword::Minimum(_)
            | Keyword::Maximum(_)
            | Keyword::ExclusiveMinimum(_)
            | Keyword::ExclusiveMaximum(_)
            | Keyword::MultipleOf(_) => match instance {
                Value::Number(n) => Self::numeric::<COLLECT>(keyword, n, location, cx),
                _ => true,
            },
            Keyword::MinLength(_)
            | Keyword::MaxLength(_)
            | Keyword::Pattern(_)
            | Keyword::Format(_) => match instance {
                Value::String(s) => self.string::<COLLECT>(keyword, s, location, cx),
                _ => true,
            },
            Keyword::Ref { target, ref_str } => {
                self.follow::<COLLECT>(*target, ref_str, instance, location, cx, evaluated)
            }
            Keyword::DynamicRef {
                fallback,
                anchor,
                ref_str,
            } => {
                let target: NodeId = cx
                    .dynamic_scope
                    .iter()
                    .find_map(|scope| {
                        self.dynamic_anchors.get(scope).and_then(|anchors| {
                            anchors
                                .iter()
                                .find(|(name, _)| **name == **anchor)
                                .map(|(_, node)| *node)
                        })
                    })
                    .unwrap_or(*fallback);
                self.follow::<COLLECT>(target, ref_str, instance, location, cx, evaluated)
            }
            Keyword::RecursiveRef { fallback, ref_str } => {
                let target: NodeId = cx
                    .dynamic_scope
                    .iter()
                    .find_map(|scope| self.recursive_anchors.get(scope).copied())
                    .unwrap_or(*fallback);
                self.follow::<COLLECT>(target, ref_str, instance, location, cx, evaluated)
            }
            Keyword::AllOf(nodes) => {
                let mut valid: bool = true;
                for node in nodes {
                    if !self.in_place::<COLLECT>(
                        *node,
                        instance,
                        location,
                        cx,
                        evaluated.as_deref_mut(),
                    ) {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::AnyOf(nodes) => {
                let mut matched: bool = false;
                for node in nodes {
                    if self.in_place::<false>(
                        *node,
                        instance,
                        location,
                        cx,
                        evaluated.as_deref_mut(),
                    ) {
                        matched = true;
                        // Every passing branch annotates, so keep going only when annotations
                        // are collected.
                        if evaluated.is_none() {
                            break;
                        }
                    }
                }
                matched
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NoSubschemaMatched {
                            instance_path: location.pointer(),
                            subschema_count: nodes.len(),
                        }
                    )
            }
            Keyword::OneOf(nodes) => {
                let mut match_count: usize = 0;
                let mut first: Option<Evaluated> = None;
                for node in nodes {
                    let mut sub: Evaluated = Evaluated::default();
                    let wants: Option<&mut Evaluated> = evaluated.is_some().then_some(&mut sub);
                    if self.eval::<false>(*node, instance, location, cx, wants) {
                        match_count += 1;
                        if first.is_none() {
                            first = Some(sub);
                        }
                        // A second match decides the verdict; the error counts every match.
                        if match_count > 1 && !COLLECT {
                            break;
                        }
                    }
                }
                match match_count {
                    1 => {
                        if let (Some(parent), Some(sub)) = (evaluated, first) {
                            parent.merge(&sub);
                        }
                        true
                    }
                    0 => fail!(
                        COLLECT,
                        cx,
                        ValidationError::NoSubschemaMatched {
                            instance_path: location.pointer(),
                            subschema_count: nodes.len(),
                        }
                    ),
                    _ => fail!(
                        COLLECT,
                        cx,
                        ValidationError::MultipleSubschemasMatched {
                            instance_path: location.pointer(),
                            subschema_count: nodes.len(),
                            match_count,
                        }
                    ),
                }
            }
            Keyword::Not(node) => {
                !self.eval::<false>(*node, instance, location, cx, None)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotSchemaMatched {
                            instance_path: location.pointer(),
                        }
                    )
            }
            Keyword::IfThenElse {
                condition,
                then,
                otherwise,
            } => {
                let passed: bool = self.in_place::<false>(
                    *condition,
                    instance,
                    location,
                    cx,
                    evaluated.as_deref_mut(),
                );
                match if passed { then } else { otherwise } {
                    Some(node) => {
                        self.in_place::<COLLECT>(*node, instance, location, cx, evaluated)
                    }
                    None => true,
                }
            }
            // Object and array keywords ignore other instances.
            Keyword::MinItems(_)
            | Keyword::MaxItems(_)
            | Keyword::UniqueItems
            | Keyword::Required(_)
            | Keyword::MinProperties(_)
            | Keyword::MaxProperties(_)
            | Keyword::DependentRequired(_)
            | Keyword::NoAdditionalProperties(_)
            | Keyword::Properties(_)
            | Keyword::PatternProperties(_)
            | Keyword::AdditionalProperties { .. }
            | Keyword::PropertyNames(_)
            | Keyword::DependentSchemas(_)
            | Keyword::Items { .. }
            | Keyword::Contains { .. }
            | Keyword::UnevaluatedProperties(_)
            | Keyword::UnevaluatedItems(_) => true,
        }
    }

    fn numeric<const COLLECT: bool>(
        keyword: &Keyword,
        n: &Number,
        location: &Location<'_>,
        cx: &mut Context,
    ) -> bool {
        let error: Option<ValidationError> = match keyword {
            Keyword::Minimum(minimum) => {
                (number::cmp_num(n, minimum) == Ordering::Less).then(|| {
                    ValidationError::BelowMinimum {
                        instance_path: location.pointer(),
                        minimum: minimum.clone(),
                        actual: n.clone(),
                    }
                })
            }
            Keyword::Maximum(maximum) => {
                (number::cmp_num(n, maximum) == Ordering::Greater).then(|| {
                    ValidationError::AboveMaximum {
                        instance_path: location.pointer(),
                        maximum: maximum.clone(),
                        actual: n.clone(),
                    }
                })
            }
            Keyword::ExclusiveMinimum(exclusive_minimum) => (number::cmp_num(n, exclusive_minimum)
                != Ordering::Greater)
                .then(|| ValidationError::NotAboveExclusiveMinimum {
                    instance_path: location.pointer(),
                    exclusive_minimum: exclusive_minimum.clone(),
                    actual: n.clone(),
                }),
            Keyword::ExclusiveMaximum(exclusive_maximum) => (number::cmp_num(n, exclusive_maximum)
                != Ordering::Less)
                .then(|| ValidationError::NotBelowExclusiveMaximum {
                    instance_path: location.pointer(),
                    exclusive_maximum: exclusive_maximum.clone(),
                    actual: n.clone(),
                }),
            Keyword::MultipleOf(multiple_of) => {
                (!number::is_multiple_of(n, multiple_of)).then(|| ValidationError::NotMultipleOf {
                    instance_path: location.pointer(),
                    multiple_of: multiple_of.clone(),
                    actual: n.clone(),
                })
            }
            _ => None,
        };
        match error {
            None => true,
            Some(error) => fail!(COLLECT, cx, error),
        }
    }

    fn string<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        s: &str,
        location: &Location<'_>,
        cx: &mut Context,
    ) -> bool {
        // Lengths count Unicode code points; a string has at most as many as it has bytes.
        let error: Option<ValidationError> = match keyword {
            Keyword::MinLength(min_length) => {
                let short: Option<u64> =
                    Some(s.chars().count() as u64).filter(|count| *count < *min_length);
                short.map(|actual_length| ValidationError::TooShort {
                    instance_path: location.pointer(),
                    min_length: *min_length,
                    actual_length,
                })
            }
            Keyword::MaxLength(max_length) => {
                if s.len() as u64 <= *max_length {
                    None
                } else {
                    let count: u64 = s.chars().count() as u64;
                    (count > *max_length).then(|| ValidationError::TooLong {
                        instance_path: location.pointer(),
                        max_length: *max_length,
                        actual_length: count,
                    })
                }
            }
            Keyword::Pattern(id) => {
                let pattern = self.pattern(*id);
                pattern
                    .regex
                    .find(s)
                    .is_none()
                    .then(|| ValidationError::PatternMismatch {
                        instance_path: location.pointer(),
                        pattern: pattern.source.clone(),
                        value: s.to_string(),
                    })
            }
            Keyword::Format(format) => {
                (!format.is_valid(s)).then(|| ValidationError::InvalidFormat {
                    instance_path: location.pointer(),
                    format: format.name().to_string(),
                    value: s.to_string(),
                })
            }
            _ => None,
        };
        match error {
            None => true,
            Some(error) => fail!(COLLECT, cx, error),
        }
    }

    /// Whether `name` is one `properties` or `patternProperties` covers.
    fn is_known(&self, known: &KnownProperties, name: &str) -> bool {
        known.names.binary_search_by(|n| (**n).cmp(name)).is_ok()
            || known
                .patterns
                .iter()
                .any(|id| self.pattern(*id).regex.find(name).is_some())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per object keyword; the closed enum is matched exhaustively"
    )]
    fn object_keyword<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        instance: &Value,
        object: &Map<String, Value>,
        location: &Location<'_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match keyword {
            Keyword::Required(names) => {
                let mut valid: bool = true;
                for name in names {
                    if !object.contains_key(&**name) {
                        valid = fail!(
                            COLLECT,
                            cx,
                            ValidationError::MissingRequired {
                                instance_path: location.key(name).pointer(),
                                property: name.to_string(),
                            }
                        );
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::MinProperties(min_properties) => {
                object.len() as u64 >= *min_properties
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooFewProperties {
                            instance_path: location.pointer(),
                            min_properties: *min_properties,
                            actual_count: object.len() as u64,
                        }
                    )
            }
            Keyword::MaxProperties(max_properties) => {
                object.len() as u64 <= *max_properties
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooManyProperties {
                            instance_path: location.pointer(),
                            max_properties: *max_properties,
                            actual_count: object.len() as u64,
                        }
                    )
            }
            Keyword::DependentRequired(dependencies) => {
                let mut valid: bool = true;
                for (name, required) in dependencies {
                    if !object.contains_key(&**name) {
                        continue;
                    }
                    for property in required {
                        if !object.contains_key(&**property) {
                            valid = fail!(
                                COLLECT,
                                cx,
                                ValidationError::MissingDependentProperty {
                                    instance_path: location.key(property).pointer(),
                                    property: property.to_string(),
                                    required_by: name.to_string(),
                                }
                            );
                            if !COLLECT {
                                return false;
                            }
                        }
                    }
                }
                valid
            }
            Keyword::NoAdditionalProperties(known) => {
                let mut valid: bool = true;
                for key in object.keys() {
                    if !self.is_known(known, key) {
                        valid = fail!(
                            COLLECT,
                            cx,
                            ValidationError::DisallowedAdditionalProperty {
                                instance_path: location.key(key).pointer(),
                                property: key.clone(),
                            }
                        );
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::Properties(properties) => {
                let mut valid: bool = true;
                if evaluated.is_none() && properties.len() <= object.len() {
                    for (name, node) in properties {
                        if let Some((key, value)) = object.get_key_value(&**name) {
                            let child: Location<'_> = location.key(key);
                            if !self.eval::<COLLECT>(*node, value, &child, cx, None) {
                                valid = false;
                                if !COLLECT {
                                    return false;
                                }
                            }
                        }
                    }
                } else {
                    for (i, (key, value)) in object.iter().enumerate() {
                        let Ok(at) = properties.binary_search_by(|(n, _)| (**n).cmp(key)) else {
                            continue;
                        };
                        if let Some(evaluated) = evaluated.as_deref_mut() {
                            evaluated.mark(i);
                        }
                        let child: Location<'_> = location.key(key);
                        if !self.eval::<COLLECT>(properties[at].1, value, &child, cx, None) {
                            valid = false;
                            if !COLLECT {
                                return false;
                            }
                        }
                    }
                }
                valid
            }
            Keyword::PatternProperties(patterns) => {
                let mut valid: bool = true;
                for (i, (key, value)) in object.iter().enumerate() {
                    for (pattern, node) in patterns {
                        if self.pattern(*pattern).regex.find(key).is_none() {
                            continue;
                        }
                        if let Some(evaluated) = evaluated.as_deref_mut() {
                            evaluated.mark(i);
                        }
                        let child: Location<'_> = location.key(key);
                        if !self.eval::<COLLECT>(*node, value, &child, cx, None) {
                            valid = false;
                            if !COLLECT {
                                return false;
                            }
                        }
                    }
                }
                valid
            }
            Keyword::AdditionalProperties { node, known } => {
                let mut valid: bool = true;
                for (i, (key, value)) in object.iter().enumerate() {
                    if self.is_known(known, key) {
                        continue;
                    }
                    if let Some(evaluated) = evaluated.as_deref_mut() {
                        evaluated.mark(i);
                    }
                    let child: Location<'_> = location.key(key);
                    if !self.eval::<COLLECT>(*node, value, &child, cx, None) {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::PropertyNames(node) => {
                let mut valid: bool = true;
                for key in object.keys() {
                    let name: Value = Value::String(key.clone());
                    let child: Location<'_> = location.key(key);
                    if !self.eval::<false>(*node, &name, &child, cx, None) {
                        valid = fail!(
                            COLLECT,
                            cx,
                            ValidationError::InvalidPropertyName {
                                instance_path: child.pointer(),
                                property: key.clone(),
                            }
                        );
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::DependentSchemas(dependencies) => {
                let mut valid: bool = true;
                for (name, node) in dependencies {
                    if object.contains_key(&**name)
                        && !self.in_place::<COLLECT>(
                            *node,
                            instance,
                            location,
                            cx,
                            evaluated.as_deref_mut(),
                        )
                    {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                valid
            }
            Keyword::UnevaluatedProperties(node) => {
                let mut valid: bool = true;
                let reject: bool = self.is_false(*node);
                for (i, (key, value)) in object.iter().enumerate() {
                    if evaluated.as_deref().is_some_and(|e| e.contains(i)) {
                        continue;
                    }
                    let child: Location<'_> = location.key(key);
                    let ok: bool = if reject {
                        fail!(
                            COLLECT,
                            cx,
                            ValidationError::DisallowedUnevaluatedProperty {
                                instance_path: child.pointer(),
                                property: key.clone(),
                            }
                        )
                    } else {
                        self.eval::<COLLECT>(*node, value, &child, cx, None)
                    };
                    if !ok {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                if let Some(evaluated) = evaluated {
                    evaluated.mark_all();
                }
                valid
            }
            _ => self.keyword::<COLLECT>(keyword, instance, location, cx, evaluated),
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per array keyword; the closed enum is matched exhaustively"
    )]
    fn array_keyword<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        instance: &Value,
        items: &[Value],
        location: &Location<'_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match keyword {
            Keyword::MinItems(min_items) => {
                items.len() as u64 >= *min_items
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooFewItems {
                            instance_path: location.pointer(),
                            min_items: *min_items,
                            actual_count: items.len() as u64,
                        }
                    )
            }
            Keyword::MaxItems(max_items) => {
                items.len() as u64 <= *max_items
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooManyItems {
                            instance_path: location.pointer(),
                            max_items: *max_items,
                            actual_count: items.len() as u64,
                        }
                    )
            }
            Keyword::UniqueItems => match first_duplicate(items) {
                None => true,
                Some(duplicate) => fail!(
                    COLLECT,
                    cx,
                    ValidationError::DuplicateArrayItems {
                        instance_path: location.pointer(),
                        duplicate_value: value_to_display_string(duplicate),
                    }
                ),
            },
            Keyword::Items { prefix, rest } => {
                let mut valid: bool = true;
                for (i, item) in items.iter().enumerate() {
                    let node: NodeId = match prefix.get(i) {
                        Some(node) => *node,
                        None => match rest {
                            Some(node) => *node,
                            None => break,
                        },
                    };
                    let child: Location<'_> = location.index(i);
                    if !self.eval::<COLLECT>(node, item, &child, cx, None) {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                if let Some(evaluated) = evaluated {
                    if rest.is_some() {
                        evaluated.mark_all();
                    } else {
                        for i in 0..prefix.len().min(items.len()) {
                            evaluated.mark(i);
                        }
                    }
                }
                valid
            }
            Keyword::Contains {
                node,
                min,
                max,
                annotates,
            } => {
                let annotating: bool = *annotates && evaluated.is_some();
                let mut match_count: u64 = 0;
                for (i, item) in items.iter().enumerate() {
                    let child: Location<'_> = location.index(i);
                    if self.eval::<false>(*node, item, &child, cx, None) {
                        match_count += 1;
                        if annotating && let Some(evaluated) = evaluated.as_deref_mut() {
                            evaluated.mark(i);
                        }
                        if !annotating && max.is_none() && match_count >= *min {
                            break;
                        }
                    }
                }
                if match_count < *min {
                    fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooFewContains {
                            instance_path: location.pointer(),
                            min_contains: *min,
                            match_count,
                        }
                    )
                } else if let Some(max) = max
                    && match_count > *max
                {
                    fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooManyContains {
                            instance_path: location.pointer(),
                            max_contains: *max,
                            match_count,
                        }
                    )
                } else {
                    true
                }
            }
            Keyword::UnevaluatedItems(node) => {
                let mut valid: bool = true;
                for (i, item) in items.iter().enumerate() {
                    if evaluated.as_deref().is_some_and(|e| e.contains(i)) {
                        continue;
                    }
                    let child: Location<'_> = location.index(i);
                    if !self.eval::<COLLECT>(*node, item, &child, cx, None) {
                        valid = false;
                        if !COLLECT {
                            return false;
                        }
                    }
                }
                if let Some(evaluated) = evaluated {
                    evaluated.mark_all();
                }
                valid
            }
            _ => self.keyword::<COLLECT>(keyword, instance, location, cx, evaluated),
        }
    }
}
