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

/// How deeply schemas may nest during one evaluation (each subschema entered, in place or at a
/// child instance, is one level). A crash guard, not a policy limit: measured on a 2 MiB thread
/// stack (the default for spawned threads), the costliest recursion shapes (an
/// `unevaluatedProperties` node re-entered through `allOf` and `$ref`, an `anyOf` branch
/// re-entered through `items` and `$ref`) overflow after about 1,200 levels in a debug build and
/// several thousand in a release build, so 512 keeps a margin of more than two in either.
/// `depth_guard_fits_two_mib_thread` checks the guard fires first on such a thread.
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
    /// Where the depth guard fired. Once set, every evaluation fails at once and the result is
    /// that one error: a branch cut short by the guard must not read as a branch that failed
    /// (which would make `not` pass, or `anyOf` report a mismatch).
    depth_exceeded: Option<JsonPointer>,
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

/// Records an error in collect mode. Out of line and cold, so building the error (and its
/// temporaries) takes no space in the evaluator's recursive frames.
#[cold]
#[inline(never)]
fn push_error(errors: &mut Vec<ValidationError>, make: impl FnOnce() -> ValidationError) {
    errors.push(make());
}

/// Records `$error` in collect mode and evaluates to `false` (the keyword failed).
macro_rules! fail {
    ($collect:expr, $cx:expr, $error:expr) => {{
        if $collect {
            push_error(&mut $cx.errors, || $error);
        }
        false
    }};
}

/// Runs `$check` for each of a keyword's parts; a failure stops the loop in verdict mode and
/// is remembered in collect mode. Evaluates to whether every part passed.
macro_rules! each {
    ($collect:expr, $iter:expr, |$item:pat_param| $check:expr) => {{
        let mut valid: bool = true;
        for $item in $iter {
            if !$check {
                valid = false;
                if !$collect {
                    break;
                }
            }
        }
        valid
    }};
}

/// The shared inputs of a keyword check.
#[derive(Clone, Copy)]
struct At<'i, 'l> {
    instance: &'i Value,
    location: &'l Location<'l>,
}

impl CompiledSchema {
    /// Whether `instance` is valid against the schema. Stops at the first failing keyword and
    /// allocates nothing for errors, so it is the fastest check; equal to
    /// `self.validate(instance).is_ok()`.
    #[must_use]
    pub fn is_valid(&self, instance: &Value) -> bool {
        let mut cx: Context = Context::default();
        self.eval::<false>(self.root, instance, &Location::ROOT, &mut cx, None)
            && cx.depth_exceeded.is_none()
    }

    /// Validates `instance`, collecting every error (one per problem, see [`ValidationError`]).
    /// When the instance nests deeper than the depth guard allows, the result is that one
    /// [`ValidationError::DepthLimitExceeded`].
    ///
    /// # Errors
    ///
    /// Returns `Err(errors)` when the instance does not conform to the schema.
    pub fn validate(&self, instance: &Value) -> ValidationResult {
        let mut cx: Context = Context::default();
        let valid: bool = self.eval::<true>(self.root, instance, &Location::ROOT, &mut cx, None);
        if let Some(instance_path) = cx.depth_exceeded {
            return Err(vec![ValidationError::DepthLimitExceeded {
                instance_path,
                limit: DEPTH_LIMIT,
            }]);
        }
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
        if cx.depth_exceeded.is_some() {
            return false;
        }
        if cx.depth >= DEPTH_LIMIT {
            cx.depth_exceeded = Some(location.pointer());
            return false;
        }
        cx.depth += 1;
        let entered: bool = self.uses_dynamic_scope && cx.dynamic_scope.last() != Some(&node.scope);
        if entered {
            cx.dynamic_scope.push(node.scope);
        }
        let at: At<'_, '_> = At { instance, location };
        let valid: bool = if node.has_unevaluated {
            self.keywords_with_unevaluated::<COLLECT>(node, at, cx, evaluated)
        } else {
            self.keywords::<COLLECT>(node, at, cx, evaluated)
        };
        if entered {
            cx.dynamic_scope.pop();
        }
        cx.depth -= 1;
        valid
    }

    /// The keywords of a node with `unevaluated*`, which reads every sibling's annotations, so
    /// they are collected here and passed on only when the node passes.
    #[inline(never)]
    fn keywords_with_unevaluated<const COLLECT: bool>(
        &self,
        node: &Node,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let mut local: Evaluated = Evaluated::default();
        let valid: bool = self.keywords::<COLLECT>(node, at, cx, Some(&mut local));
        if valid && let Some(parent) = evaluated {
            parent.merge(&local);
        }
        valid
    }

    fn keywords<const COLLECT: bool>(
        &self,
        node: &Node,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(COLLECT, node.keywords.iter(), |keyword| self
            .keyword::<COLLECT>(keyword, at, cx, evaluated.as_deref_mut()))
    }

    /// Dispatches one keyword. Each keyword is its own function, so a recursive evaluation
    /// holds only the frames of the keywords it passes through.
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per keyword; the closed enum is matched exhaustively"
    )]
    fn keyword<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match (keyword, at.instance) {
            (Keyword::False, _) => Self::false_schema::<COLLECT>(at, cx),
            (
                Keyword::Type {
                    types,
                    integer_semantics,
                },
                instance,
            ) => {
                types.matches(instance, *integer_semantics)
                    || Self::type_error::<COLLECT>(*types, at, cx)
            }
            (Keyword::Const(expected), _) => Self::const_check::<COLLECT>(expected, at, cx),
            (Keyword::Enum(allowed), _) => Self::enum_check::<COLLECT>(allowed, at, cx),
            (
                Keyword::Minimum(_)
                | Keyword::Maximum(_)
                | Keyword::ExclusiveMinimum(_)
                | Keyword::ExclusiveMaximum(_)
                | Keyword::MultipleOf(_),
                Value::Number(n),
            ) => Self::numeric::<COLLECT>(keyword, n, at, cx),
            (
                Keyword::MinLength(_)
                | Keyword::MaxLength(_)
                | Keyword::Pattern(_)
                | Keyword::Format(_),
                Value::String(s),
            ) => self.string::<COLLECT>(keyword, s, at, cx),
            (
                Keyword::MinItems(_) | Keyword::MaxItems(_) | Keyword::UniqueItems,
                Value::Array(items),
            ) => Self::array_assertion::<COLLECT>(keyword, items, at, cx),
            (
                Keyword::Required(_)
                | Keyword::MinProperties(_)
                | Keyword::MaxProperties(_)
                | Keyword::DependentRequired(_)
                | Keyword::NoAdditionalProperties(_),
                Value::Object(object),
            ) => self.object_assertion::<COLLECT>(keyword, object, at, cx),
            (Keyword::Ref { target, ref_str }, _) => {
                self.follow::<COLLECT>(*target, ref_str, at, cx, evaluated)
            }
            (
                Keyword::DynamicRef {
                    fallback,
                    anchor,
                    ref_str,
                },
                _,
            ) => {
                let target: NodeId = self.dynamic_target(*fallback, anchor, cx);
                self.follow::<COLLECT>(target, ref_str, at, cx, evaluated)
            }
            (Keyword::RecursiveRef { fallback, ref_str }, _) => {
                let target: NodeId = cx
                    .dynamic_scope
                    .iter()
                    .find_map(|scope| self.recursive_anchors.get(scope).copied())
                    .unwrap_or(*fallback);
                self.follow::<COLLECT>(target, ref_str, at, cx, evaluated)
            }
            (Keyword::Properties(properties), Value::Object(object)) => {
                self.properties::<COLLECT>(properties, object, at, cx, evaluated)
            }
            (Keyword::PatternProperties(patterns), Value::Object(object)) => {
                self.pattern_properties::<COLLECT>(patterns, object, at, cx, evaluated)
            }
            (Keyword::AdditionalProperties { node, known }, Value::Object(object)) => {
                self.additional_properties::<COLLECT>(*node, known, object, at, cx, evaluated)
            }
            (Keyword::PropertyNames(node), Value::Object(object)) => {
                self.property_names::<COLLECT>(*node, object, at, cx)
            }
            (Keyword::DependentSchemas(dependencies), Value::Object(object)) => {
                self.dependent_schemas::<COLLECT>(dependencies, object, at, cx, evaluated)
            }
            (Keyword::UnevaluatedProperties(node), Value::Object(object)) => {
                self.unevaluated_properties::<COLLECT>(*node, object, at, cx, evaluated)
            }
            (Keyword::Items { prefix, rest }, Value::Array(items)) => {
                self.items::<COLLECT>(prefix, *rest, items, at, cx, evaluated)
            }
            (Keyword::Contains { .. }, Value::Array(items)) => {
                self.contains::<COLLECT>(keyword, items, at, cx, evaluated)
            }
            (Keyword::UnevaluatedItems(node), Value::Array(items)) => {
                self.unevaluated_items::<COLLECT>(*node, items, at, cx, evaluated)
            }
            (Keyword::AllOf(nodes), _) => self.all_of::<COLLECT>(nodes, at, cx, evaluated),
            (Keyword::AnyOf(nodes), _) => self.any_of::<COLLECT>(nodes, at, cx, evaluated),
            (Keyword::OneOf(nodes), _) => self.one_of::<COLLECT>(nodes, at, cx, evaluated),
            (Keyword::Not(node), _) => self.not::<COLLECT>(*node, at, cx),
            (
                Keyword::IfThenElse {
                    condition,
                    then,
                    otherwise,
                },
                _,
            ) => self.if_then_else::<COLLECT>(*condition, *then, *otherwise, at, cx, evaluated),
            // Type-specific keywords ignore instances of other types.
            (
                Keyword::Minimum(_)
                | Keyword::Maximum(_)
                | Keyword::ExclusiveMinimum(_)
                | Keyword::ExclusiveMaximum(_)
                | Keyword::MultipleOf(_)
                | Keyword::MinLength(_)
                | Keyword::MaxLength(_)
                | Keyword::Pattern(_)
                | Keyword::Format(_)
                | Keyword::MinItems(_)
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
                | Keyword::UnevaluatedProperties(_)
                | Keyword::Items { .. }
                | Keyword::Contains { .. }
                | Keyword::UnevaluatedItems(_),
                _,
            ) => true,
        }
    }

    /// Evaluates a subschema at the same instance location; its annotations count only when it
    /// passes.
    fn in_place<const COLLECT: bool>(
        &self,
        id: NodeId,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        match evaluated {
            None => self.eval::<COLLECT>(id, at.instance, at.location, cx, None),
            Some(parent) => self.in_place_annotated::<COLLECT>(id, at, cx, parent),
        }
    }

    #[inline(never)]
    fn in_place_annotated<const COLLECT: bool>(
        &self,
        id: NodeId,
        at: At<'_, '_>,
        cx: &mut Context,
        parent: &mut Evaluated,
    ) -> bool {
        let mut sub: Evaluated = Evaluated::default();
        let ok: bool = self.eval::<COLLECT>(id, at.instance, at.location, cx, Some(&mut sub));
        if ok {
            parent.merge(&sub);
        }
        ok
    }

    /// Follows a reference to `target` in place, failing with [`ValidationError::RefCycle`] when
    /// `target` is already being evaluated at this instance location.
    fn follow<const COLLECT: bool>(
        &self,
        target: NodeId,
        ref_str: &str,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let address: *const Value = at.instance;
        // Entries for this instance location are the newest ones: a child location's entries
        // are popped before its parent is evaluated further.
        let looping: bool = cx
            .refs
            .iter()
            .rev()
            .take_while(|(_, p)| std::ptr::eq(*p, address))
            .any(|(node, _)| *node == target);
        if looping {
            return fail!(
                COLLECT,
                cx,
                ValidationError::RefCycle {
                    instance_path: at.location.pointer(),
                    ref_str: ref_str.to_string(),
                }
            );
        }
        cx.refs.push((target, address));
        let ok: bool = self.in_place::<COLLECT>(target, at, cx, evaluated);
        cx.refs.pop();
        ok
    }

    /// The target of a dynamic `$dynamicRef`: the outermost resource in the dynamic scope that
    /// declares `anchor`, else `fallback`.
    fn dynamic_target(&self, fallback: NodeId, anchor: &str, cx: &Context) -> NodeId {
        cx.dynamic_scope
            .iter()
            .find_map(|scope| {
                self.dynamic_anchors.get(scope).and_then(|anchors| {
                    anchors
                        .iter()
                        .find(|(name, _)| **name == *anchor)
                        .map(|(_, node)| *node)
                })
            })
            .unwrap_or(fallback)
    }

    fn false_schema<const COLLECT: bool>(at: At<'_, '_>, cx: &mut Context) -> bool {
        fail!(
            COLLECT,
            cx,
            ValidationError::FalseSchema {
                instance_path: at.location.pointer(),
            }
        )
    }

    fn type_error<const COLLECT: bool>(types: TypeSet, at: At<'_, '_>, cx: &mut Context) -> bool {
        fail!(
            COLLECT,
            cx,
            type_mismatch(types, at.instance, at.location.pointer())
        )
    }

    fn const_check<const COLLECT: bool>(
        expected: &Value,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        number::json_equal(at.instance, expected)
            || fail!(
                COLLECT,
                cx,
                ValidationError::NotConst {
                    instance_path: at.location.pointer(),
                    expected: value_to_display_string(expected),
                    actual: value_to_display_string(at.instance),
                }
            )
    }

    fn enum_check<const COLLECT: bool>(
        allowed: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        allowed.iter().any(|a| number::json_equal(a, at.instance))
            || fail!(
                COLLECT,
                cx,
                ValidationError::NotInEnum {
                    instance_path: at.location.pointer(),
                    invalid_value: value_to_display_string(at.instance),
                    allowed: allowed.iter().map(value_to_display_string).collect(),
                }
            )
    }

    fn numeric<const COLLECT: bool>(
        keyword: &Keyword,
        n: &Number,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let path = || at.location.pointer();
        match keyword {
            Keyword::Minimum(minimum) => {
                number::cmp_num(n, minimum) != Ordering::Less
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::BelowMinimum {
                            instance_path: path(),
                            minimum: minimum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::Maximum(maximum) => {
                number::cmp_num(n, maximum) != Ordering::Greater
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::AboveMaximum {
                            instance_path: path(),
                            maximum: maximum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::ExclusiveMinimum(exclusive_minimum) => {
                number::cmp_num(n, exclusive_minimum) == Ordering::Greater
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotAboveExclusiveMinimum {
                            instance_path: path(),
                            exclusive_minimum: exclusive_minimum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::ExclusiveMaximum(exclusive_maximum) => {
                number::cmp_num(n, exclusive_maximum) == Ordering::Less
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotBelowExclusiveMaximum {
                            instance_path: path(),
                            exclusive_maximum: exclusive_maximum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::MultipleOf(multiple_of) => {
                number::is_multiple_of(n, multiple_of)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::NotMultipleOf {
                            instance_path: path(),
                            multiple_of: multiple_of.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            _ => true,
        }
    }

    fn string<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        s: &str,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let path = || at.location.pointer();
        // Lengths count Unicode code points; a string has at most as many as it has bytes.
        match keyword {
            Keyword::MinLength(min_length) => {
                let count: u64 = s.chars().count() as u64;
                count >= *min_length
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooShort {
                            instance_path: path(),
                            min_length: *min_length,
                            actual_length: count,
                        }
                    )
            }
            Keyword::MaxLength(max_length) => {
                if s.len() as u64 <= *max_length {
                    return true;
                }
                let count: u64 = s.chars().count() as u64;
                count <= *max_length
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooLong {
                            instance_path: path(),
                            max_length: *max_length,
                            actual_length: count,
                        }
                    )
            }
            Keyword::Pattern(id) => {
                let pattern = self.pattern(*id);
                pattern.regex.find(s).is_some()
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::PatternMismatch {
                            instance_path: path(),
                            pattern: pattern.source.clone(),
                            value: s.to_string(),
                        }
                    )
            }
            Keyword::Format(format) => {
                format.is_valid(s)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::InvalidFormat {
                            instance_path: path(),
                            format: format.name().to_string(),
                            value: s.to_string(),
                        }
                    )
            }
            _ => true,
        }
    }

    fn array_assertion<const COLLECT: bool>(
        keyword: &Keyword,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let path = || at.location.pointer();
        let count: u64 = items.len() as u64;
        match keyword {
            Keyword::MinItems(min_items) => {
                count >= *min_items
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooFewItems {
                            instance_path: path(),
                            min_items: *min_items,
                            actual_count: count,
                        }
                    )
            }
            Keyword::MaxItems(max_items) => {
                count <= *max_items
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooManyItems {
                            instance_path: path(),
                            max_items: *max_items,
                            actual_count: count,
                        }
                    )
            }
            Keyword::UniqueItems => match first_duplicate(items) {
                None => true,
                Some(duplicate) => fail!(
                    COLLECT,
                    cx,
                    ValidationError::DuplicateArrayItems {
                        instance_path: path(),
                        duplicate_value: value_to_display_string(duplicate),
                    }
                ),
            },
            _ => true,
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

    fn object_assertion<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let count: u64 = object.len() as u64;
        match keyword {
            Keyword::Required(names) => each!(COLLECT, names.iter(), |name| object
                .contains_key(&**name)
                || fail!(
                    COLLECT,
                    cx,
                    ValidationError::MissingRequired {
                        instance_path: at.location.key(name).pointer(),
                        property: name.to_string(),
                    }
                )),
            Keyword::MinProperties(min_properties) => {
                count >= *min_properties
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooFewProperties {
                            instance_path: at.location.pointer(),
                            min_properties: *min_properties,
                            actual_count: count,
                        }
                    )
            }
            Keyword::MaxProperties(max_properties) => {
                count <= *max_properties
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::TooManyProperties {
                            instance_path: at.location.pointer(),
                            max_properties: *max_properties,
                            actual_count: count,
                        }
                    )
            }
            Keyword::DependentRequired(dependencies) => each!(
                COLLECT,
                dependencies
                    .iter()
                    .filter(|(name, _)| object.contains_key(&**name))
                    .flat_map(|(name, required)| required.iter().map(move |r| (name, r))),
                |(name, property)| object.contains_key(&**property)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::MissingDependentProperty {
                            instance_path: at.location.key(property).pointer(),
                            property: property.to_string(),
                            required_by: name.to_string(),
                        }
                    )
            ),
            Keyword::NoAdditionalProperties(known) => {
                each!(COLLECT, object.keys(), |key| self.is_known(known, key)
                    || fail!(
                        COLLECT,
                        cx,
                        ValidationError::DisallowedAdditionalProperty {
                            instance_path: at.location.key(key).pointer(),
                            property: key.clone(),
                        }
                    ))
            }
            _ => true,
        }
    }

    fn properties<const COLLECT: bool>(
        &self,
        properties: &[(Box<str>, NodeId)],
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        // Walk the shorter side: the schema's names (looking each up in the object) unless
        // annotations need each member's position.
        if evaluated.is_none() && properties.len() <= object.len() {
            return each!(
                COLLECT,
                properties
                    .iter()
                    .filter_map(|(name, node)| object.get_key_value(&**name).map(|kv| (kv, node))),
                |((key, value), node)| self.eval::<COLLECT>(
                    *node,
                    value,
                    &at.location.key(key),
                    cx,
                    None
                )
            );
        }
        each!(
            COLLECT,
            object.iter().enumerate(),
            |(i, (key, value))| match properties.binary_search_by(|(n, _)| (**n).cmp(key)) {
                Err(_) => true,
                Ok(found) => {
                    if let Some(evaluated) = evaluated.as_deref_mut() {
                        evaluated.mark(i);
                    }
                    self.eval::<COLLECT>(
                        properties[found].1,
                        value,
                        &at.location.key(key),
                        cx,
                        None,
                    )
                }
            }
        )
    }

    fn pattern_properties<const COLLECT: bool>(
        &self,
        patterns: &[(super::compiled::RegexId, NodeId)],
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(
            COLLECT,
            object.iter().enumerate().flat_map(|(i, kv)| patterns
                .iter()
                .filter(|(pattern, _)| self.pattern(*pattern).regex.find(kv.0).is_some())
                .map(move |(_, node)| (i, kv, node))),
            |(i, (key, value), node)| {
                if let Some(evaluated) = evaluated.as_deref_mut() {
                    evaluated.mark(i);
                }
                self.eval::<COLLECT>(*node, value, &at.location.key(key), cx, None)
            }
        )
    }

    fn additional_properties<const COLLECT: bool>(
        &self,
        node: NodeId,
        known: &KnownProperties,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(
            COLLECT,
            object
                .iter()
                .enumerate()
                .filter(|(_, (key, _))| !self.is_known(known, key)),
            |(i, (key, value))| {
                if let Some(evaluated) = evaluated.as_deref_mut() {
                    evaluated.mark(i);
                }
                self.eval::<COLLECT>(node, value, &at.location.key(key), cx, None)
            }
        )
    }

    fn property_names<const COLLECT: bool>(
        &self,
        node: NodeId,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        each!(COLLECT, object.keys(), |key| {
            let name: Value = Value::String(key.clone());
            let child: Location<'_> = at.location.key(key);
            self.eval::<false>(node, &name, &child, cx, None)
                || fail!(
                    COLLECT,
                    cx,
                    ValidationError::InvalidPropertyName {
                        instance_path: child.pointer(),
                        property: key.clone(),
                    }
                )
        })
    }

    fn dependent_schemas<const COLLECT: bool>(
        &self,
        dependencies: &[(Box<str>, NodeId)],
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(
            COLLECT,
            dependencies
                .iter()
                .filter(|(name, _)| object.contains_key(&**name)),
            |(_, node)| self.in_place::<COLLECT>(*node, at, cx, evaluated.as_deref_mut())
        )
    }

    fn unevaluated_properties<const COLLECT: bool>(
        &self,
        node: NodeId,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let reject: bool = self.is_false(node);
        let seen: Option<&Evaluated> = evaluated.as_deref();
        let valid: bool = each!(
            COLLECT,
            object
                .iter()
                .enumerate()
                .filter(|(i, _)| !seen.is_some_and(|e| e.contains(*i))),
            |(_, (key, value))| {
                let child: Location<'_> = at.location.key(key);
                if reject {
                    fail!(
                        COLLECT,
                        cx,
                        ValidationError::DisallowedUnevaluatedProperty {
                            instance_path: child.pointer(),
                            property: key.clone(),
                        }
                    )
                } else {
                    self.eval::<COLLECT>(node, value, &child, cx, None)
                }
            }
        );
        if let Some(evaluated) = evaluated {
            evaluated.mark_all();
        }
        valid
    }

    fn items<const COLLECT: bool>(
        &self,
        prefix: &[NodeId],
        rest: Option<NodeId>,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let valid: bool = each!(
            COLLECT,
            items.iter().enumerate().map_while(|(i, item)| prefix
                .get(i)
                .copied()
                .or(rest)
                .map(|node| (i, item, node))),
            |(i, item, node)| self.eval::<COLLECT>(node, item, &at.location.index(i), cx, None)
        );
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

    fn contains<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let Keyword::Contains {
            node,
            min,
            max,
            annotates,
        } = keyword
        else {
            return true;
        };
        let annotating: bool = *annotates && evaluated.is_some();
        let mut match_count: u64 = 0;
        for (i, item) in items.iter().enumerate() {
            if self.eval::<false>(*node, item, &at.location.index(i), cx, None) {
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
            return fail!(
                COLLECT,
                cx,
                ValidationError::TooFewContains {
                    instance_path: at.location.pointer(),
                    min_contains: *min,
                    match_count,
                }
            );
        }
        match max {
            Some(max) if match_count > *max => fail!(
                COLLECT,
                cx,
                ValidationError::TooManyContains {
                    instance_path: at.location.pointer(),
                    max_contains: *max,
                    match_count,
                }
            ),
            _ => true,
        }
    }

    fn unevaluated_items<const COLLECT: bool>(
        &self,
        node: NodeId,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let seen: Option<&Evaluated> = evaluated.as_deref();
        let valid: bool = each!(
            COLLECT,
            items
                .iter()
                .enumerate()
                .filter(|(i, _)| !seen.is_some_and(|e| e.contains(*i))),
            |(i, item)| self.eval::<COLLECT>(node, item, &at.location.index(i), cx, None)
        );
        if let Some(evaluated) = evaluated {
            evaluated.mark_all();
        }
        valid
    }

    fn all_of<const COLLECT: bool>(
        &self,
        nodes: &[NodeId],
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(COLLECT, nodes.iter(), |node| self.in_place::<COLLECT>(
            *node,
            at,
            cx,
            evaluated.as_deref_mut()
        ))
    }

    fn any_of<const COLLECT: bool>(
        &self,
        nodes: &[NodeId],
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let mut matched: bool = false;
        for node in nodes {
            if self.in_place::<false>(*node, at, cx, evaluated.as_deref_mut()) {
                matched = true;
                // Every passing branch annotates, so keep going only when annotations are
                // collected.
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
                    instance_path: at.location.pointer(),
                    subschema_count: nodes.len(),
                }
            )
    }

    fn one_of<const COLLECT: bool>(
        &self,
        nodes: &[NodeId],
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let mut match_count: usize = 0;
        let mut first: Option<Evaluated> = None;
        for node in nodes {
            let mut sub: Evaluated = Evaluated::default();
            let wants: Option<&mut Evaluated> = evaluated.is_some().then_some(&mut sub);
            if self.eval::<false>(*node, at.instance, at.location, cx, wants) {
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
                    instance_path: at.location.pointer(),
                    subschema_count: nodes.len(),
                }
            ),
            _ => fail!(
                COLLECT,
                cx,
                ValidationError::MultipleSubschemasMatched {
                    instance_path: at.location.pointer(),
                    subschema_count: nodes.len(),
                    match_count,
                }
            ),
        }
    }

    fn not<const COLLECT: bool>(&self, node: NodeId, at: At<'_, '_>, cx: &mut Context) -> bool {
        let matched: bool = self.eval::<false>(node, at.instance, at.location, cx, None);
        // A branch cut short by the depth guard did not fail: it must not make `not` pass.
        if cx.depth_exceeded.is_some() {
            return false;
        }
        !matched
            || fail!(
                COLLECT,
                cx,
                ValidationError::NotSchemaMatched {
                    instance_path: at.location.pointer(),
                }
            )
    }

    fn if_then_else<const COLLECT: bool>(
        &self,
        condition: NodeId,
        then: Option<NodeId>,
        otherwise: Option<NodeId>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let passed: bool = self.in_place::<false>(condition, at, cx, evaluated.as_deref_mut());
        match if passed { then } else { otherwise } {
            Some(node) => self.in_place::<COLLECT>(node, at, cx, evaluated),
            None => true,
        }
    }
}
