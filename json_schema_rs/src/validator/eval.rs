//! Evaluates a [`CompiledSchema`] against an instance.
//!
//! One evaluator serves two modes, chosen by the `COLLECT` const parameter: the verdict mode
//! (`is_valid`) stops at the first failing keyword and allocates nothing for errors; the collect
//! mode (`validate`) evaluates every keyword and records one [`ValidationError`] per problem.
//! Combinator branches (`anyOf`, `oneOf`, `not`, `if`, `contains`, `propertyNames`) always run in
//! the verdict mode, since only their outcome matters.
//!
//! The evaluator recurses (one native frame group per nested schema), guarded by a depth limit
//! that returns [`ValidationErrorKind::DepthLimitExceeded`] before the stack can overflow. An
//! in-place loop (a `$ref` re-entered at the same instance location without consuming any of
//! the instance) fails that branch with [`ValidationErrorKind::RefCycle`].
//!
//! Every subschema entered pushes a [`Location`] frame on the native stack recording the
//! instance step (a member or an item, or none for an in-place applicator) and the schema step
//! ([`Via`]); an error's instance path and keyword locations are read from those frames only
//! when the error is built. When `anyOf` / `oneOf` fails in the collect mode, its branches are
//! evaluated again in the collect mode, only to attach their errors as the failure's causes.

use super::compiled::{
    CompiledSchema, Contains, DynamicRef, EnumId, EnumList, FormatCheck, Keyword, KnownProperties,
    Kw, Node, NodeId, Pattern, Pick, PrefixItems, RegexId, Required, Union,
};
use super::content::{Content, ContentFailure};
use super::error::{ValidationError, ValidationErrorKind, ValidationResult};
use super::name_index::PropertyTable;
use crate::json_pointer::{self, JsonPointer};
use crate::json_schema::{JsonType, Scope, TypeSet};
use crate::number::{self, JsonEqKey};
use serde_json::{Map, Number, Value};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;

/// How deeply schemas may nest during one evaluation (each subschema entered, in place or at a
/// child instance, is one level). A crash guard, not a policy limit, sized per build profile
/// from a measurement on a 2 MiB thread stack (the default for spawned threads): the costliest
/// recursion shapes (an `unevaluatedProperties` node re-entered through `allOf` and `$ref`, an
/// `items` / `$ref` chain collecting errors) overflow after about 4,600 levels in a release build
/// and 1,200 in a debug build, so 2,048 and 512 keep a margin of more than two.
/// `depth_guard_fits_two_mib_thread` checks the guard fires first on such a thread.
pub(crate) const DEPTH_LIMIT: usize = if cfg!(debug_assertions) { 512 } else { 2048 };

/// Where evaluation is: a linked list on the native stack, one frame per subschema entered, read
/// (into an instance path and keyword locations) only when an error is created.
#[derive(Clone, Copy)]
pub(crate) struct Location<'a> {
    parent: Option<&'a Location<'a>>,
    step: Step<'a>,
}

#[derive(Clone, Copy)]
enum Step<'a> {
    /// The root schema at the instance root.
    Root,
    /// Into the member `key`, through `Via`.
    Key(&'a str, Via<'a>),
    /// Into the item at the index, through `Via`.
    Index(usize, Via<'a>),
    /// Into a subschema at the same instance (`allOf`, `not`, `if`, ...).
    InPlace(Via<'a>),
    /// A `$ref` (or dynamic reference, `keyword` says which) followed in place to `target`:
    /// the same instance, no pointer segment. These steps are the in-place loop guard (see
    /// [`CompiledSchema::follow`]), with how many resources declaring a dynamic anchor were in
    /// the dynamic scope (see [`CompiledSchema::anchor_resources`]).
    Ref {
        target: NodeId,
        keyword: Kw,
        anchors: usize,
    },
}

/// The schema path from a node to the subschema a [`Step`] enters.
#[derive(Clone, Copy)]
enum Via<'a> {
    /// No subschema: a member named by an error (a property `additionalProperties: false` rejects), whose keyword
    /// location is the parent's.
    None,
    /// The keyword alone (`additionalProperties`, a single `items`, `not`).
    Keyword(Kw),
    /// The keyword and the member name or item index the step enters (`properties/<name>`,
    /// `prefixItems/<index>`).
    Member(Kw),
    /// The keyword and a position in its array (`allOf/<index>`).
    Index(Kw, u32),
    /// The keyword and a name (`dependentSchemas/<name>`). A reference to the `Box` is one
    /// pointer, where `&str` would be two, so the frames stay small.
    #[expect(
        clippy::borrowed_box,
        reason = "a thin pointer keeps the location frames small"
    )]
    Name(Kw, &'a Box<str>),
    /// `patternProperties/<pattern>`.
    Pattern(RegexId),
}

impl<'a> Location<'a> {
    pub(crate) const ROOT: Location<'static> = Location {
        parent: None,
        step: Step::Root,
    };

    fn key(&'a self, key: &'a str, via: Via<'a>) -> Location<'a> {
        Location {
            parent: Some(self),
            step: Step::Key(key, via),
        }
    }

    fn index(&'a self, index: usize, via: Via<'a>) -> Location<'a> {
        Location {
            parent: Some(self),
            step: Step::Index(index, via),
        }
    }

    fn in_place(&'a self, via: Via<'a>) -> Location<'a> {
        Location {
            parent: Some(self),
            step: Step::InPlace(via),
        }
    }

    fn pointer(&self) -> JsonPointer {
        let mut segments: Vec<Cow<'_, str>> = Vec::with_capacity(8);
        let mut cursor: Option<&Location<'_>> = Some(self);
        while let Some(location) = cursor {
            match location.step {
                Step::Root | Step::Ref { .. } | Step::InPlace(_) => {}
                Step::Key(key, _) => segments.push(Cow::Borrowed(key)),
                Step::Index(index, _) => segments.push(Cow::Owned(index.to_string())),
            }
            cursor = location.parent;
        }
        JsonPointer::from_segments(segments.iter().rev().map(|s| &**s))
    }
}

/// Which members (by position in the object) or items (by index) of the current instance some
/// keyword evaluated: the annotations `unevaluatedProperties` / `unevaluatedItems` read. A bitset
/// whose first word is inline, so instances of up to 64 members or items never allocate.
#[derive(Default)]
pub(crate) struct Evaluated {
    all: bool,
    first: u64,
    /// Words for indexes 64 and up.
    rest: Vec<u64>,
}

impl Evaluated {
    fn mark(&mut self, index: usize) {
        let bit: u64 = 1 << (index % 64);
        match (index / 64).checked_sub(1) {
            None => self.first |= bit,
            Some(word) => {
                if self.rest.len() <= word {
                    self.rest.resize(word + 1, 0);
                }
                self.rest[word] |= bit;
            }
        }
    }

    const fn mark_all(&mut self) {
        self.all = true;
    }

    fn contains(&self, index: usize) -> bool {
        let word: Option<u64> = match (index / 64).checked_sub(1) {
            None => Some(self.first),
            Some(word) => self.rest.get(word).copied(),
        };
        self.all || word.is_some_and(|word| word >> (index % 64) & 1 == 1)
    }

    fn merge(&mut self, other: &Evaluated) {
        self.all |= other.all;
        self.first |= other.first;
        if self.rest.len() < other.rest.len() {
            self.rest.resize(other.rest.len(), 0);
        }
        for (mine, theirs) in self.rest.iter_mut().zip(&other.rest) {
            *mine |= theirs;
        }
    }
}

/// The branch each `anyOf` / `oneOf` chose for an object or array, keyed by the value's address
/// and the union's: recorded under [`CompiledSchema::validate_recording_at`] so the trusted
/// deserialization of generated types reads each union's variant without evaluating it again
/// (#128). An entry is the union's own verdict, true wherever the union was evaluated.
#[derive(Debug, Default)]
pub(crate) struct UnionChoices(Vec<((usize, usize), u32)>);

impl UnionChoices {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sorts the entries by key for [`UnionChoices::get`], keeping each key's last entry: an
    /// outer evaluation ends after an inner one at the same value.
    fn finish(&mut self) {
        // Stable, so equal keys stay in recording order and the last is kept.
        self.0.sort_by_key(|(key, _)| *key);
        self.0.reverse();
        self.0.dedup_by_key(|(key, _)| *key);
        self.0.reverse();
    }

    /// The branch recorded for `key`.
    pub(crate) fn get(&self, key: (usize, usize)) -> Option<u32> {
        self.0
            .binary_search_by_key(&key, |(key, _)| *key)
            .ok()
            .map(|at| self.0[at].1)
    }
}

/// The key of `union`'s choice for `instance` in [`UnionChoices`]: their addresses.
pub(crate) fn choice_key(instance: &Value, union: &Union) -> (usize, usize) {
    (std::ptr::from_ref(instance) as usize, union_address(union))
}

/// `union`'s half of a [`UnionChoices`] key.
pub(crate) fn union_address(union: &Union) -> usize {
    std::ptr::from_ref(union) as usize
}

/// Per-evaluation state.
#[derive(Default)]
struct Context {
    /// Where union choices are recorded, when they are.
    choices: Option<UnionChoices>,
    errors: Vec<ValidationError>,
    depth: usize,
    /// Schema resources entered, outermost first (only when a dynamic reference needs them).
    dynamic_scope: Vec<Scope>,
    /// The error of the depth guard, when it fired. Once set, every evaluation fails at once
    /// and the result is that one error: a branch cut short by the guard must not read as a
    /// branch that failed (which would make `not` pass, or `anyOf` report a mismatch).
    depth_exceeded: Option<ValidationError>,
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
/// [`ValidationErrorKind::ExpectedType`] otherwise.
fn type_mismatch(expected: TypeSet, instance: &Value) -> ValidationErrorKind {
    let got: String = json_type_name(instance).to_string();
    match expected.single() {
        Some(JsonType::Object) => ValidationErrorKind::ExpectedObject { got },
        Some(JsonType::String) => ValidationErrorKind::ExpectedString { got },
        Some(JsonType::Integer) => ValidationErrorKind::ExpectedInteger { got },
        Some(JsonType::Number) => ValidationErrorKind::ExpectedNumber { got },
        Some(JsonType::Array) => ValidationErrorKind::ExpectedArray { got },
        Some(JsonType::Boolean) => ValidationErrorKind::ExpectedBoolean { got },
        Some(JsonType::Null) => ValidationErrorKind::ExpectedNull { got },
        None => ValidationErrorKind::ExpectedType { expected, got },
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

/// Walking an object and a name list together (or looking each member up in the list) pays off
/// while neither has more than this many entries per entry of the other; past it, looking up the
/// smaller side's entries in the larger one is cheaper.
const MERGE_RATIO: usize = 4;

/// Whether the object (its keys in sorted order) has every one of the sorted, distinct `names`:
/// one walk over both.
fn contains_all_names(object: &Map<String, Value>, names: &[Box<str>]) -> bool {
    let mut keys: serde_json::map::Keys<'_> = object.keys();
    'names: for name in names {
        for key in keys.by_ref() {
            match key.as_str().cmp(name) {
                Ordering::Less => {}
                Ordering::Equal => continue 'names,
                Ordering::Greater => return false,
            }
        }
        return false;
    }
    true
}

/// Records, in collect mode, the error `$kind` of the keyword `$keyword` evaluated at
/// `$location`, and evaluates to `false` (the keyword failed).
macro_rules! fail {
    ($collect:expr, $this:expr, $cx:expr, $location:expr, $keyword:expr, $kind:expr) => {{
        if $collect {
            $this.report(&mut $cx.errors, $location, Some($keyword), || $kind);
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

    /// Whether `instance` is valid against every node of `nodes` (entry points from
    /// [`CompiledSchema::with_entries`]): [`CompiledSchema::is_valid`] at each, stopping at the
    /// first that fails.
    pub(crate) fn is_valid_at(&self, nodes: &[NodeId], instance: &Value) -> bool {
        let mut cx: Context = Context::default();
        nodes.iter().all(|id| {
            self.eval::<false>(*id, instance, &Location::ROOT, &mut cx, None)
                && cx.depth_exceeded.is_none()
        })
    }

    /// The result of an evaluation in collect mode: the depth guard's error alone when it fired.
    #[inline]
    fn collected(valid: bool, cx: Context) -> ValidationResult {
        if let Some(error) = cx.depth_exceeded {
            return Err(vec![error]);
        }
        if valid && cx.errors.is_empty() {
            Ok(())
        } else {
            Err(cx.errors)
        }
    }

    /// Validates `instance`, collecting every error (one per problem, see [`ValidationError`]).
    /// When the instance nests deeper than the depth guard allows, the result is that one
    /// [`ValidationErrorKind::DepthLimitExceeded`].
    ///
    /// # Errors
    ///
    /// Returns `Err(errors)` when the instance does not conform to the schema.
    pub fn validate(&self, instance: &Value) -> ValidationResult {
        let mut cx: Context = Context::default();
        let valid: bool = self.eval::<true>(self.root, instance, &Location::ROOT, &mut cx, None);
        if let Some(error) = cx.depth_exceeded {
            return Err(vec![error]);
        }
        if valid && cx.errors.is_empty() {
            Ok(())
        } else {
            Err(cx.errors)
        }
    }

    /// Validates `instance` against every node of `nodes` (entry points from
    /// [`CompiledSchema::with_entries`]): the errors of each node in order, each with keyword
    /// locations relative to its node, exactly as when it is the root. When the depth guard fires, the result is that
    /// one error.
    /// [`CompiledSchema::validate_at`], also giving the branch each `anyOf` / `oneOf` chose for
    /// each value it was evaluated on. Nothing is recorded when a dynamic reference is in play:
    /// the same union at the same value can then decide differently in different scopes.
    pub(crate) fn validate_recording_at(
        &self,
        nodes: &[NodeId],
        instance: &Value,
    ) -> (ValidationResult, UnionChoices) {
        let mut cx: Context = Context {
            choices: (!self.uses_dynamic_scope).then(UnionChoices::default),
            ..Context::default()
        };
        let mut valid: bool = true;
        for id in nodes {
            valid &= self.eval::<true>(*id, instance, &Location::ROOT, &mut cx, None);
        }
        let mut choices: UnionChoices = cx.choices.take().unwrap_or_default();
        choices.finish();
        (Self::collected(valid, cx), choices)
    }

    pub(crate) fn validate_at(&self, nodes: &[NodeId], instance: &Value) -> ValidationResult {
        let mut cx: Context = Context::default();
        let mut valid: bool = true;
        for id in nodes {
            valid &= self.eval::<true>(*id, instance, &Location::ROOT, &mut cx, None);
        }
        Self::collected(valid, cx)
    }

    /// Records the error `make` builds, of `keyword` (`None`: of the subschema itself) evaluated
    /// at `location`. Out of line and cold, so building the error (its locations and
    /// temporaries) takes no space in the evaluator's recursive frames.
    #[cold]
    #[inline(never)]
    fn report(
        &self,
        errors: &mut Vec<ValidationError>,
        location: &Location<'_>,
        keyword: Option<Kw>,
        make: impl FnOnce() -> ValidationErrorKind,
    ) {
        errors.push(self.error_at(location, keyword, make()));
    }

    /// The error `kind` of `keyword` (`None`: of the subschema itself) evaluated at `location`,
    /// with its instance path and keyword locations.
    fn error_at(
        &self,
        location: &Location<'_>,
        keyword: Option<Kw>,
        kind: ValidationErrorKind,
    ) -> ValidationError {
        // Segments leaf first; `after_ref` is how many were collected when the nearest
        // reference was reached, and its target.
        let mut segments: Vec<Cow<'_, str>> = Vec::with_capacity(16);
        let mut after_ref: Option<(usize, NodeId)> = None;
        if let Some(keyword) = keyword {
            segments.push(Cow::Borrowed(keyword.as_str()));
        }
        let mut cursor: Option<&Location<'_>> = Some(location);
        while let Some(frame) = cursor {
            match frame.step {
                Step::Root => {}
                Step::Key(key, via) => self.push_via(via, Some(Cow::Borrowed(key)), &mut segments),
                Step::Index(index, via) => {
                    self.push_via(via, Some(Cow::Owned(index.to_string())), &mut segments);
                }
                Step::InPlace(via) => self.push_via(via, None, &mut segments),
                Step::Ref {
                    target, keyword, ..
                } => {
                    if after_ref.is_none() {
                        after_ref = Some((segments.len(), target));
                    }
                    segments.push(Cow::Borrowed(keyword.as_str()));
                }
            }
            cursor = frame.parent;
        }
        let absolute: Option<String> = after_ref.and_then(|(count, target)| {
            let base: &str = self.target_uris.get(&target)?;
            let mut uri: String = String::with_capacity(base.len() + 64);
            uri.push_str(base);
            json_pointer::push_uri_fragment(segments[..count].iter().rev().map(|s| &**s), &mut uri);
            Some(uri)
        });
        ValidationError {
            instance_location: location.pointer(),
            keyword_location: JsonPointer::from_segments(segments.iter().rev().map(|s| &**s)),
            absolute_keyword_location: absolute,
            kind,
        }
    }

    /// Pushes the schema segments of `via`, last first; `member` is the member name or item
    /// index of the step.
    fn push_via<'s>(
        &'s self,
        via: Via<'s>,
        member: Option<Cow<'s, str>>,
        segments: &mut Vec<Cow<'s, str>>,
    ) {
        match via {
            Via::None => {}
            Via::Keyword(keyword) => segments.push(Cow::Borrowed(keyword.as_str())),
            Via::Member(keyword) => {
                segments.push(member.unwrap_or_default());
                segments.push(Cow::Borrowed(keyword.as_str()));
            }
            Via::Index(keyword, index) => {
                segments.push(Cow::Owned(index.to_string()));
                segments.push(Cow::Borrowed(keyword.as_str()));
            }
            Via::Name(keyword, name) => {
                segments.push(Cow::Borrowed(name));
                segments.push(Cow::Borrowed(keyword.as_str()));
            }
            Via::Pattern(id) => {
                segments.push(Cow::Borrowed(&self.pattern(id).source));
                segments.push(Cow::Borrowed(Kw::PatternProperties.as_str()));
            }
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
            self.depth_exceeded(location, cx);
            return false;
        }
        // A leaf that passes is valid in both modes (it records nothing); one that fails is
        // evaluated again below in collect mode, which builds its errors.
        if node.leaf {
            if self.leaf_valid(&node.keywords, instance) {
                return true;
            }
            if !COLLECT {
                return false;
            }
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

    /// The verdict of node `id` on `instance` when it is a leaf, decided where
    /// [`CompiledSchema::eval`] would decide it: only while the depth guard has not fired and the
    /// node sits below the limit, so it is exactly `eval`'s verdict. Callers try it before
    /// building the child's location; they still call `eval` for a node that is not a leaf, and
    /// in collect mode for a leaf that fails, to record its errors.
    #[inline]
    fn leaf_verdict(&self, id: NodeId, instance: &Value, cx: &Context) -> Option<bool> {
        let node: &Node = self.node(id);
        (node.leaf && cx.depth_exceeded.is_none() && cx.depth < DEPTH_LIMIT)
            .then(|| self.leaf_valid(&node.keywords, instance))
    }

    /// [`CompiledSchema::leaf_verdict`] on each of `instances`: whether all of them pass, when
    /// node `id` is a leaf.
    #[inline]
    fn leaf_verdict_all(&self, id: NodeId, instances: &[Value], cx: &Context) -> Option<bool> {
        let node: &Node = self.node(id);
        (node.leaf && cx.depth_exceeded.is_none() && cx.depth < DEPTH_LIMIT)
            .then(|| self.leaf_valid_all(&node.keywords, instances))
    }

    /// Whether `instance` passes every keyword of a leaf node (see [`Node::leaf`]): the same
    /// verdicts as [`CompiledSchema::keyword`], without its dispatch or error reporting.
    #[inline(never)]
    fn leaf_valid(&self, keywords: &[Keyword], instance: &Value) -> bool {
        keywords
            .iter()
            .all(|keyword| self.leaf_keyword(keyword, std::iter::once(instance)))
    }

    /// Whether every one of `instances` passes every keyword of a leaf node: [`leaf_valid`] on
    /// each, with the loops swapped. A leaf's keywords only assert, so the order they run in
    /// changes no verdict, and taking each keyword once keeps its dispatch out of the loop over
    /// the instances.
    ///
    /// [`leaf_valid`]: CompiledSchema::leaf_valid
    #[inline(never)]
    fn leaf_valid_all(&self, keywords: &[Keyword], instances: &[Value]) -> bool {
        keywords
            .iter()
            .all(|keyword| self.leaf_keyword(keyword, instances.iter()))
    }

    /// Whether each of `instances` passes `keyword`, one of a leaf node's.
    #[inline]
    fn leaf_keyword<'v>(
        &self,
        keyword: &Keyword,
        mut instances: impl Iterator<Item = &'v Value>,
    ) -> bool {
        match keyword {
            Keyword::False => instances.next().is_none(),
            Keyword::Type {
                types,
                integer_semantics,
            } => instances.all(|instance| types.matches(instance, *integer_semantics)),
            Keyword::Const(expected) => {
                instances.all(|instance| number::json_equal(instance, expected))
            }
            Keyword::Enum(id) => {
                let list: &EnumList = self.enum_list(*id);
                instances.all(|instance| list.contains(instance))
            }
            Keyword::Minimum(minimum) => instances.all(|instance| match instance {
                Value::Number(n) => number::cmp_num(n, minimum) != Ordering::Less,
                _ => true,
            }),
            Keyword::Maximum(maximum) => instances.all(|instance| match instance {
                Value::Number(n) => number::cmp_num(n, maximum) != Ordering::Greater,
                _ => true,
            }),
            Keyword::ExclusiveMinimum(minimum) => instances.all(|instance| match instance {
                Value::Number(n) => number::cmp_num(n, minimum) == Ordering::Greater,
                _ => true,
            }),
            Keyword::ExclusiveMaximum(maximum) => instances.all(|instance| match instance {
                Value::Number(n) => number::cmp_num(n, maximum) == Ordering::Less,
                _ => true,
            }),
            Keyword::MultipleOf(multiple_of) => instances.all(|instance| match instance {
                Value::Number(n) => number::is_multiple_of(n, multiple_of),
                _ => true,
            }),
            // Lengths count code points; a code point takes one to four bytes.
            Keyword::MinLength(min_length) => instances.all(|instance| match instance {
                Value::String(s) => {
                    s.len() as u64 / 4 >= *min_length || s.chars().count() as u64 >= *min_length
                }
                _ => true,
            }),
            Keyword::MaxLength(max_length) => instances.all(|instance| match instance {
                Value::String(s) => {
                    s.len() as u64 <= *max_length || s.chars().count() as u64 <= *max_length
                }
                _ => true,
            }),
            Keyword::Pattern(id) => {
                let pattern: &Pattern = self.pattern(*id);
                instances.all(|instance| match instance {
                    Value::String(s) => pattern.is_match(s),
                    _ => true,
                })
            }
            Keyword::Format(format) => instances.all(|instance| match instance {
                Value::String(s) => format.is_valid(s),
                _ => true,
            }),
            Keyword::UserFormat(id) => {
                let check: &FormatCheck = &self.user_format(*id).check;
                instances.all(|instance| match instance {
                    Value::String(s) => check(s),
                    _ => true,
                })
            }
            _ => true,
        }
    }

    #[cold]
    #[inline(never)]
    fn depth_exceeded(&self, location: &Location<'_>, cx: &mut Context) {
        cx.depth_exceeded = Some(self.error_at(
            location,
            None,
            ValidationErrorKind::DepthLimitExceeded { limit: DEPTH_LIMIT },
        ));
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
            (Keyword::False, _) => self.false_schema::<COLLECT>(at, cx),
            (
                Keyword::Type {
                    types,
                    integer_semantics,
                },
                instance,
            ) => {
                types.matches(instance, *integer_semantics)
                    || self.type_error::<COLLECT>(*types, at, cx)
            }
            (Keyword::Const(expected), _) => self.const_check::<COLLECT>(expected, at, cx),
            (Keyword::Enum(id), _) => self.enum_check::<COLLECT>(*id, at, cx),
            (
                Keyword::Minimum(_)
                | Keyword::Maximum(_)
                | Keyword::ExclusiveMinimum(_)
                | Keyword::ExclusiveMaximum(_)
                | Keyword::MultipleOf(_),
                Value::Number(n),
            ) => self.numeric::<COLLECT>(keyword, n, at, cx),
            (
                Keyword::MinLength(_)
                | Keyword::MaxLength(_)
                | Keyword::Pattern(_)
                | Keyword::Format(_)
                | Keyword::UserFormat(_)
                | Keyword::Content(_),
                Value::String(s),
            ) => self.string::<COLLECT>(keyword, s, at, cx),
            (
                Keyword::MinItems(_) | Keyword::MaxItems(_) | Keyword::UniqueItems,
                Value::Array(items),
            ) => self.array_assertion::<COLLECT>(keyword, items, at, cx),
            (
                Keyword::Required(_)
                | Keyword::MinProperties(_)
                | Keyword::MaxProperties(_)
                | Keyword::DependentRequired { .. }
                | Keyword::NoAdditionalProperties(_),
                Value::Object(object),
            ) => self.object_assertion::<COLLECT>(keyword, object, at, cx),
            (
                Keyword::Ref {
                    target,
                    keyword,
                    ref_str,
                },
                _,
            ) => self.follow::<COLLECT>(*target, *keyword, ref_str, at, cx, evaluated),
            (Keyword::DynamicRef(dynamic), _) => {
                let DynamicRef {
                    fallback,
                    anchor,
                    ref_str,
                } = &**dynamic;
                let target: NodeId = self.dynamic_target(*fallback, anchor, cx);
                self.follow::<COLLECT>(target, Kw::DynamicRef, ref_str, at, cx, evaluated)
            }
            (Keyword::RecursiveRef { fallback, ref_str }, _) => {
                let target: NodeId = cx
                    .dynamic_scope
                    .iter()
                    .find_map(|scope| self.recursive_anchors.get(scope).copied())
                    .unwrap_or(*fallback);
                self.follow::<COLLECT>(target, Kw::RecursiveRef, ref_str, at, cx, evaluated)
            }
            (Keyword::Properties(id), Value::Object(object)) => {
                self.properties::<COLLECT>(self.property_table(*id), object, at, cx, evaluated)
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
            (
                Keyword::DependentSchemas {
                    dependencies,
                    keyword,
                },
                Value::Object(object),
            ) => {
                self.dependent_schemas::<COLLECT>(dependencies, *keyword, object, at, cx, evaluated)
            }
            (Keyword::UnevaluatedProperties(node), Value::Object(object)) => {
                self.unevaluated_properties::<COLLECT>(*node, object, at, cx, evaluated)
            }
            (Keyword::Items { node, keyword }, Value::Array(items)) => self.items::<COLLECT>(
                &[],
                Some(*node),
                (*keyword, *keyword),
                items,
                at,
                cx,
                evaluated,
            ),
            (Keyword::PrefixItems(tuple), Value::Array(items)) => {
                let PrefixItems {
                    prefix,
                    rest,
                    prefix_keyword,
                    rest_keyword,
                } = &**tuple;
                self.items::<COLLECT>(
                    prefix,
                    *rest,
                    (*prefix_keyword, *rest_keyword),
                    items,
                    at,
                    cx,
                    evaluated,
                )
            }
            (Keyword::Contains(contains), Value::Array(items)) => {
                self.contains::<COLLECT>(contains, items, at, cx, evaluated)
            }
            (Keyword::UnevaluatedItems(node), Value::Array(items)) => {
                self.unevaluated_items::<COLLECT>(*node, items, at, cx, evaluated)
            }
            (Keyword::AllOf(nodes), _) => self.all_of::<COLLECT>(nodes, at, cx, evaluated),
            (Keyword::AnyOf(union), _) => self.any_of::<COLLECT>(union, at, cx, evaluated),
            (Keyword::OneOf(union), _) => self.one_of::<COLLECT>(union, at, cx, evaluated),
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
                | Keyword::UserFormat(_)
                | Keyword::Content(_)
                | Keyword::MinItems(_)
                | Keyword::MaxItems(_)
                | Keyword::UniqueItems
                | Keyword::Required(_)
                | Keyword::MinProperties(_)
                | Keyword::MaxProperties(_)
                | Keyword::DependentRequired { .. }
                | Keyword::NoAdditionalProperties(_)
                | Keyword::Properties(_)
                | Keyword::PatternProperties(_)
                | Keyword::AdditionalProperties { .. }
                | Keyword::PropertyNames(_)
                | Keyword::DependentSchemas { .. }
                | Keyword::UnevaluatedProperties(_)
                | Keyword::Items { .. }
                | Keyword::PrefixItems(_)
                | Keyword::Contains(_)
                | Keyword::UnevaluatedItems(_),
                _,
            ) => true,
        }
    }

    /// Evaluates a subschema at the same instance, entered through `location`; its
    /// annotations count only when it passes.
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
            Some(parent) => self.in_place_annotated::<COLLECT>(id, instance, location, cx, parent),
        }
    }

    #[inline(never)]
    fn in_place_annotated<const COLLECT: bool>(
        &self,
        id: NodeId,
        instance: &Value,
        location: &Location<'_>,
        cx: &mut Context,
        parent: &mut Evaluated,
    ) -> bool {
        let mut sub: Evaluated = Evaluated::default();
        let ok: bool = self.eval::<COLLECT>(id, instance, location, cx, Some(&mut sub));
        if ok {
            parent.merge(&sub);
        }
        ok
    }

    /// Follows a reference (`keyword`) to `target` in place, failing with
    /// [`ValidationErrorKind::RefCycle`] when `target` is already being evaluated at this
    /// instance location.
    fn follow<const COLLECT: bool>(
        &self,
        target: NodeId,
        keyword: Kw,
        ref_str: &str,
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        // The references followed at this instance location are the `Ref` steps above it,
        // through in-place steps: the step that entered the location (a key, an index, the
        // root) ends them.
        // Re-entering under a dynamic scope with more anchor-declaring resources is not a
        // loop: a dynamic reference inside may now resolve elsewhere. That count only grows
        // along a path and is bounded by the number of resources, so the guard still ends
        // every loop.
        let anchors: usize = self.anchor_resources(cx);
        let mut looping: bool = false;
        let mut cursor: Option<&Location<'_>> = Some(at.location);
        while let Some(frame) = cursor {
            let seen: (NodeId, usize) = match frame.step {
                Step::Ref {
                    target, anchors, ..
                } => (target, anchors),
                Step::InPlace(_) => {
                    cursor = frame.parent;
                    continue;
                }
                Step::Root | Step::Key(..) | Step::Index(..) => break,
            };
            if seen == (target, anchors) {
                looping = true;
                break;
            }
            cursor = frame.parent;
        }
        let entered: Location<'_> = Location {
            parent: Some(at.location),
            step: Step::Ref {
                target,
                keyword,
                anchors,
            },
        };
        if looping {
            // The keyword location ends at the reference that closed the loop.
            return fail!(
                COLLECT,
                self,
                cx,
                at.location,
                keyword,
                ValidationErrorKind::RefCycle {
                    ref_str: ref_str.to_string(),
                }
            );
        }
        self.in_place::<COLLECT>(target, at.instance, &entered, cx, evaluated)
    }

    /// How many distinct resources in the dynamic scope declare a dynamic anchor a dynamic
    /// reference can resolve to (`$dynamicAnchor`, or a root `$recursiveAnchor: true`): what
    /// decides where dynamic references resolve. Zero when no schema uses one.
    fn anchor_resources(&self, cx: &Context) -> usize {
        if !self.uses_dynamic_scope {
            return 0;
        }
        let declares = |scope: &Scope| {
            self.dynamic_anchors.contains_key(scope) || self.recursive_anchors.contains_key(scope)
        };
        cx.dynamic_scope
            .iter()
            .enumerate()
            .filter(|(i, scope)| declares(scope) && !cx.dynamic_scope[..*i].contains(scope))
            .count()
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

    fn false_schema<const COLLECT: bool>(&self, at: At<'_, '_>, cx: &mut Context) -> bool {
        if COLLECT {
            self.report(&mut cx.errors, at.location, None, || {
                ValidationErrorKind::FalseSchema
            });
        }
        false
    }

    fn type_error<const COLLECT: bool>(
        &self,
        types: TypeSet,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        fail!(
            COLLECT,
            self,
            cx,
            at.location,
            Kw::Type,
            type_mismatch(types, at.instance)
        )
    }

    fn const_check<const COLLECT: bool>(
        &self,
        expected: &Value,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        number::json_equal(at.instance, expected)
            || fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::Const,
                ValidationErrorKind::NotConst {
                    expected: value_to_display_string(expected),
                    actual: value_to_display_string(at.instance),
                }
            )
    }

    fn enum_check<const COLLECT: bool>(
        &self,
        id: EnumId,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        self.enum_list(id).contains(at.instance)
            || fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::Enum,
                ValidationErrorKind::NotInEnum {
                    invalid_value: value_to_display_string(at.instance),
                    allowed: self.enum_display(id).to_vec(),
                }
            )
    }

    fn numeric<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        n: &Number,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        match keyword {
            Keyword::Minimum(minimum) => {
                number::cmp_num(n, minimum) != Ordering::Less
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::Minimum,
                        ValidationErrorKind::BelowMinimum {
                            minimum: minimum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::Maximum(maximum) => {
                number::cmp_num(n, maximum) != Ordering::Greater
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::Maximum,
                        ValidationErrorKind::AboveMaximum {
                            maximum: maximum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::ExclusiveMinimum(exclusive_minimum) => {
                number::cmp_num(n, exclusive_minimum) == Ordering::Greater
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::ExclusiveMinimum,
                        ValidationErrorKind::NotAboveExclusiveMinimum {
                            exclusive_minimum: exclusive_minimum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::ExclusiveMaximum(exclusive_maximum) => {
                number::cmp_num(n, exclusive_maximum) == Ordering::Less
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::ExclusiveMaximum,
                        ValidationErrorKind::NotBelowExclusiveMaximum {
                            exclusive_maximum: exclusive_maximum.clone(),
                            actual: n.clone(),
                        }
                    )
            }
            Keyword::MultipleOf(multiple_of) => {
                number::is_multiple_of(n, multiple_of)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MultipleOf,
                        ValidationErrorKind::NotMultipleOf {
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
        // Lengths count Unicode code points; a string has at most as many as it has bytes.
        match keyword {
            Keyword::MinLength(min_length) => {
                // A code point takes at most four bytes: enough bytes decide without counting.
                if s.len() as u64 / 4 >= *min_length {
                    return true;
                }
                let count: u64 = s.chars().count() as u64;
                count >= *min_length
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MinLength,
                        ValidationErrorKind::TooShort {
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
                        self,
                        cx,
                        at.location,
                        Kw::MaxLength,
                        ValidationErrorKind::TooLong {
                            max_length: *max_length,
                            actual_length: count,
                        }
                    )
            }
            Keyword::Pattern(id) => {
                let pattern: &Pattern = self.pattern(*id);
                pattern.is_match(s)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::Pattern,
                        ValidationErrorKind::PatternMismatch {
                            pattern: pattern.source.clone(),
                            value: s.to_string(),
                        }
                    )
            }
            Keyword::Format(format) => {
                format.is_valid(s)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::Format,
                        ValidationErrorKind::InvalidFormat {
                            format: format.name().to_string(),
                            value: s.to_string(),
                        }
                    )
            }
            Keyword::Content(content) => self.content::<COLLECT>(content, s, at, cx),
            Keyword::UserFormat(id) => {
                let format = self.user_format(*id);
                (format.check)(s)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::Format,
                        ValidationErrorKind::InvalidFormat {
                            format: format.name.to_string(),
                            value: s.to_string(),
                        }
                    )
            }
            _ => true,
        }
    }

    /// `contentEncoding` / `contentMediaType` on string `s`: a string that does not decode fails
    /// its encoding; a decoded one that is not of the media type fails the media type.
    fn content<const COLLECT: bool>(
        &self,
        content: &Content,
        s: &str,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        match content.check(s) {
            None => true,
            Some(ContentFailure::Encoding(encoding)) => fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::ContentEncoding,
                ValidationErrorKind::InvalidContentEncoding {
                    encoding: encoding.to_string(),
                    value: s.to_string(),
                }
            ),
            Some(ContentFailure::MediaType(media_type)) => fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::ContentMediaType,
                ValidationErrorKind::InvalidContentMediaType {
                    media_type: media_type.to_string(),
                    value: s.to_string(),
                }
            ),
        }
    }

    fn array_assertion<const COLLECT: bool>(
        &self,
        keyword: &Keyword,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let count: u64 = items.len() as u64;
        match keyword {
            Keyword::MinItems(min_items) => {
                count >= *min_items
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MinItems,
                        ValidationErrorKind::TooFewItems {
                            min_items: *min_items,
                            actual_count: count,
                        }
                    )
            }
            Keyword::MaxItems(max_items) => {
                count <= *max_items
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MaxItems,
                        ValidationErrorKind::TooManyItems {
                            max_items: *max_items,
                            actual_count: count,
                        }
                    )
            }
            Keyword::UniqueItems => match first_duplicate(items) {
                None => true,
                Some(duplicate) => fail!(
                    COLLECT,
                    self,
                    cx,
                    at.location,
                    Kw::UniqueItems,
                    ValidationErrorKind::DuplicateArrayItems {
                        duplicate_value: value_to_display_string(duplicate),
                    }
                ),
            },
            _ => true,
        }
    }

    /// Whether `name` is one `properties` or `patternProperties` covers.
    fn is_known(&self, known: &KnownProperties, name: &str) -> bool {
        known
            .names
            .is_some_and(|id| self.property_table(id).contains(name))
            || known
                .patterns
                .iter()
                .any(|id| self.pattern(*id).is_match(name))
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
            Keyword::Required(required) => self.required::<COLLECT>(required, object, at, cx),
            Keyword::MinProperties(min_properties) => {
                count >= *min_properties
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MinProperties,
                        ValidationErrorKind::TooFewProperties {
                            min_properties: *min_properties,
                            actual_count: count,
                        }
                    )
            }
            Keyword::MaxProperties(max_properties) => {
                count <= *max_properties
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        Kw::MaxProperties,
                        ValidationErrorKind::TooManyProperties {
                            max_properties: *max_properties,
                            actual_count: count,
                        }
                    )
            }
            Keyword::DependentRequired {
                dependencies,
                keyword,
            } => each!(
                COLLECT,
                dependencies
                    .iter()
                    .filter(|(name, _)| object.contains_key(&**name))
                    .flat_map(|(name, required)| required.iter().map(move |r| (name, r))),
                |(name, property)| object.contains_key(&**property)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        at.location,
                        *keyword,
                        ValidationErrorKind::MissingDependentProperty {
                            property: property.to_string(),
                            required_by: name.to_string(),
                        }
                    )
            ),
            Keyword::NoAdditionalProperties(known) => {
                each!(COLLECT, object.keys(), |key| self.is_known(known, key)
                    || fail!(
                        COLLECT,
                        self,
                        cx,
                        &at.location.key(key, Via::None),
                        Kw::AdditionalProperties,
                        ValidationErrorKind::DisallowedAdditionalProperty {
                            property: key.clone(),
                        }
                    ))
            }
            _ => true,
        }
    }

    /// `required`: decided against the object's sorted keys in one pass when the maps are
    /// sorted and the object is not much larger than the list; errors are built in schema order,
    /// by looking each name up.
    fn required<const COLLECT: bool>(
        &self,
        required: &Required,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
    ) -> bool {
        let Required { names, sorted } = required;
        if self.sorted_maps {
            // More distinct names than members: some name is missing.
            let decided: Option<bool> = if sorted.len() > object.len() {
                Some(false)
            } else if object.len() <= MERGE_RATIO * sorted.len() {
                Some(contains_all_names(object, sorted))
            } else {
                None
            };
            match decided {
                Some(true) => return true,
                Some(false) if !COLLECT => return false,
                Some(false) | None => {}
            }
        }
        each!(COLLECT, names.iter(), |name| object.contains_key(&**name)
            || fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::Required,
                ValidationErrorKind::MissingRequired {
                    property: name.to_string(),
                }
            ))
    }

    fn properties<const COLLECT: bool>(
        &self,
        table: &PropertyTable,
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let via: Via<'_> = Via::Member(Kw::Properties);
        // An object much larger than the list: look each name up in the object, unless
        // annotations need each member's position. Without sorted maps the names' order is not
        // the members', and errors follow the names whenever the object has at least as many
        // members as the list has names.
        let by_name: bool = evaluated.is_none()
            && if self.sorted_maps {
                object.len() > MERGE_RATIO * table.len()
            } else {
                table.len() <= object.len()
            };
        if by_name {
            return each!(
                COLLECT,
                table
                    .iter()
                    .filter_map(|(name, node)| object.get_key_value(name).map(|kv| (kv, node))),
                |((key, value), node)| match self.leaf_verdict(node, value, cx) {
                    Some(true) => true,
                    Some(false) if !COLLECT => false,
                    _ => self.eval::<COLLECT>(node, value, &at.location.key(key, via), cx, None),
                }
            );
        }
        each!(
            COLLECT,
            object
                .iter()
                .enumerate()
                .filter_map(|(i, (key, value))| table.get(key).map(|node| (i, key, value, node))),
            |(i, key, value, node)| {
                if let Some(evaluated) = evaluated.as_deref_mut() {
                    evaluated.mark(i);
                }
                match self.leaf_verdict(node, value, cx) {
                    Some(true) => true,
                    Some(false) if !COLLECT => false,
                    _ => self.eval::<COLLECT>(node, value, &at.location.key(key, via), cx, None),
                }
            }
        )
    }

    fn pattern_properties<const COLLECT: bool>(
        &self,
        patterns: &[(RegexId, NodeId)],
        object: &Map<String, Value>,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        each!(
            COLLECT,
            object.iter().enumerate().flat_map(|(i, kv)| patterns
                .iter()
                .filter(|(pattern, _)| self.pattern(*pattern).is_match(kv.0))
                .map(move |(pattern, node)| (i, kv, *pattern, node))),
            |(i, (key, value), pattern, node)| {
                if let Some(evaluated) = evaluated.as_deref_mut() {
                    evaluated.mark(i);
                }
                let child: Location<'_> = at.location.key(key, Via::Pattern(pattern));
                self.eval::<COLLECT>(*node, value, &child, cx, None)
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
        let via: Via<'_> = Via::Keyword(Kw::AdditionalProperties);
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
                match self.leaf_verdict(node, value, cx) {
                    Some(true) => true,
                    Some(false) if !COLLECT => false,
                    _ => self.eval::<COLLECT>(node, value, &at.location.key(key, via), cx, None),
                }
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
        // One buffer holds each name in turn, so names are not allocated one by one.
        let mut name: Value = Value::String(String::new());
        each!(COLLECT, object.keys(), |key| {
            if let Value::String(buffer) = &mut name {
                buffer.clear();
                buffer.push_str(key);
            }
            let child: Location<'_> = at.location.key(key, Via::Keyword(Kw::PropertyNames));
            self.eval::<false>(node, &name, &child, cx, None)
                || fail!(
                    COLLECT,
                    self,
                    cx,
                    &at.location.key(key, Via::None),
                    Kw::PropertyNames,
                    ValidationErrorKind::InvalidPropertyName {
                        property: key.clone(),
                    }
                )
        })
    }

    fn dependent_schemas<const COLLECT: bool>(
        &self,
        dependencies: &[(Box<str>, NodeId)],
        keyword: Kw,
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
            |(name, node)| {
                let branch: Location<'_> = at.location.in_place(Via::Name(keyword, name));
                self.in_place::<COLLECT>(*node, at.instance, &branch, cx, evaluated.as_deref_mut())
            }
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
                if reject {
                    fail!(
                        COLLECT,
                        self,
                        cx,
                        &at.location.key(key, Via::None),
                        Kw::UnevaluatedProperties,
                        ValidationErrorKind::DisallowedUnevaluatedProperty {
                            property: key.clone(),
                        }
                    )
                } else {
                    let child: Location<'_> = at
                        .location
                        .key(key, Via::Keyword(Kw::UnevaluatedProperties));
                    self.eval::<COLLECT>(node, value, &child, cx, None)
                }
            }
        );
        if let Some(evaluated) = evaluated {
            evaluated.mark_all();
        }
        valid
    }

    /// `prefixItems` / `items` (and their spellings in earlier drafts, `keywords` gives the
    /// prefix's and the rest's).
    #[expect(
        clippy::too_many_arguments,
        reason = "the tuple, its spellings, and the shared evaluation state"
    )]
    fn items<const COLLECT: bool>(
        &self,
        prefix: &[NodeId],
        rest: Option<NodeId>,
        keywords: (Kw, Kw),
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
        evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let (prefix_keyword, rest_keyword) = keywords;
        // Items past the prefix that a leaf checks are decided together, the leaf's keywords
        // each taken once; the general path below runs only when they fail in collect mode, to
        // build the errors, and for the prefix.
        let valid: bool = if let ([], Some(rest)) = (prefix, rest)
            && let Some(valid) = self.leaf_verdict_all(rest, items, cx)
            && (valid || !COLLECT)
        {
            valid
        } else {
            each!(
                COLLECT,
                items
                    .iter()
                    .enumerate()
                    .map_while(|(i, item)| match prefix.get(i) {
                        Some(node) => Some((i, item, *node, Via::Member(prefix_keyword))),
                        None => rest.map(|node| (i, item, node, Via::Keyword(rest_keyword))),
                    }),
                |(i, item, node, via)| match self.leaf_verdict(node, item, cx) {
                    Some(true) => true,
                    Some(false) if !COLLECT => false,
                    _ => self.eval::<COLLECT>(node, item, &at.location.index(i, via), cx, None),
                }
            )
        };
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
        contains: &Contains,
        items: &[Value],
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let Contains {
            node,
            min,
            max,
            annotates,
            min_keyword,
        } = contains;
        let annotating: bool = *annotates && evaluated.is_some();
        let mut match_count: u64 = 0;
        for (i, item) in items.iter().enumerate() {
            let child: Location<'_> = at.location.index(i, Via::Keyword(Kw::Contains));
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
            return fail!(
                COLLECT,
                self,
                cx,
                at.location,
                *min_keyword,
                ValidationErrorKind::TooFewContains {
                    min_contains: *min,
                    match_count,
                }
            );
        }
        match max {
            Some(max) if match_count > *max => fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::MaxContains,
                ValidationErrorKind::TooManyContains {
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
        let via: Via<'_> = Via::Keyword(Kw::UnevaluatedItems);
        let valid: bool = each!(
            COLLECT,
            items
                .iter()
                .enumerate()
                .filter(|(i, _)| !seen.is_some_and(|e| e.contains(*i))),
            |(i, item)| self.eval::<COLLECT>(node, item, &at.location.index(i, via), cx, None)
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
        each!(COLLECT, nodes.iter().enumerate(), |(i, node)| {
            let branch: Location<'_> = at.location.in_place(Via::Index(Kw::AllOf, index_u32(i)));
            self.in_place::<COLLECT>(*node, at.instance, &branch, cx, evaluated.as_deref_mut())
        })
    }

    /// Records that `union` chose `branch` for `instance`, when choices are recorded. The last
    /// evaluation wins: an outer one ends after any inner one at the same value.
    /// Only objects and arrays: a generated union reads a recorded choice for no other value
    /// (see `ValueRef`).
    #[inline]
    fn record_choice(union: &Union, instance: &Value, branch: usize, cx: &mut Context) {
        if let Some(choices) = cx.choices.as_mut()
            && matches!(instance, Value::Object(_) | Value::Array(_))
        {
            choices
                .0
                .push((choice_key(instance, union), index_u32(branch)));
        }
    }

    /// The one branch of `union` that `instance` can pass, when its discriminator decides it.
    fn pick(union: &Union, instance: &Value) -> Pick {
        match (&union.discriminator, instance) {
            (Some(discriminator), Value::Object(object)) => discriminator.pick(object),
            _ => Pick::Undecided,
        }
    }

    fn any_of<const COLLECT: bool>(
        &self,
        union: &Union,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let nodes: &[NodeId] = &union.nodes;
        // Every other branch fails, so the one the discriminator picks decides. A failure is
        // evaluated again in full below when its errors are collected.
        let pick: Pick = Self::pick(union, at.instance);
        if pick != Pick::Undecided {
            if let Pick::Branch(i) = pick {
                let branch: Location<'_> =
                    at.location.in_place(Via::Index(Kw::AnyOf, index_u32(i)));
                if self.in_place::<false>(
                    nodes[i],
                    at.instance,
                    &branch,
                    cx,
                    evaluated.as_deref_mut(),
                ) {
                    Self::record_choice(union, at.instance, i, cx);
                    return true;
                }
            }
            if !COLLECT {
                return false;
            }
        }
        let mut matched: bool = false;
        for (i, node) in nodes.iter().enumerate() {
            let branch: Location<'_> = at.location.in_place(Via::Index(Kw::AnyOf, index_u32(i)));
            if self.in_place::<false>(*node, at.instance, &branch, cx, evaluated.as_deref_mut()) {
                if !matched {
                    Self::record_choice(union, at.instance, i, cx);
                }
                matched = true;
                // Every passing branch annotates, so keep going only when annotations are
                // collected.
                if evaluated.is_none() {
                    break;
                }
            }
        }
        if !matched && COLLECT {
            self.no_match(Kw::AnyOf, nodes, at, cx);
        }
        matched
    }

    fn one_of<const COLLECT: bool>(
        &self,
        union: &Union,
        at: At<'_, '_>,
        cx: &mut Context,
        mut evaluated: Option<&mut Evaluated>,
    ) -> bool {
        let nodes: &[NodeId] = &union.nodes;
        // As in `any_of`: only the discriminated branch can pass, so it is the one match.
        let pick: Pick = Self::pick(union, at.instance);
        if pick != Pick::Undecided {
            if let Pick::Branch(i) = pick {
                let mut sub: Evaluated = Evaluated::default();
                let wants: Option<&mut Evaluated> = evaluated.is_some().then_some(&mut sub);
                let branch: Location<'_> =
                    at.location.in_place(Via::Index(Kw::OneOf, index_u32(i)));
                if self.eval::<false>(nodes[i], at.instance, &branch, cx, wants) {
                    if let Some(parent) = evaluated.as_deref_mut() {
                        parent.merge(&sub);
                    }
                    Self::record_choice(union, at.instance, i, cx);
                    return true;
                }
            }
            if !COLLECT {
                return false;
            }
        }
        let mut match_count: usize = 0;
        let mut first: Option<(usize, Evaluated)> = None;
        for (i, node) in nodes.iter().enumerate() {
            let mut sub: Evaluated = Evaluated::default();
            let wants: Option<&mut Evaluated> = evaluated.is_some().then_some(&mut sub);
            let branch: Location<'_> = at.location.in_place(Via::Index(Kw::OneOf, index_u32(i)));
            if self.eval::<false>(*node, at.instance, &branch, cx, wants) {
                match_count += 1;
                if first.is_none() {
                    first = Some((i, sub));
                }
                // A second match decides the verdict; the error counts every match.
                if match_count > 1 && !COLLECT {
                    break;
                }
            }
        }
        match match_count {
            1 => {
                if let Some((i, sub)) = first {
                    if let Some(parent) = evaluated {
                        parent.merge(&sub);
                    }
                    Self::record_choice(union, at.instance, i, cx);
                }
                true
            }
            0 => {
                if COLLECT {
                    self.no_match(Kw::OneOf, nodes, at, cx);
                }
                false
            }
            _ => fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::OneOf,
                ValidationErrorKind::MultipleSubschemasMatched {
                    subschema_count: nodes.len(),
                    match_count,
                }
            ),
        }
    }

    /// Records [`ValidationErrorKind::NoSubschemaMatched`] for an `anyOf` / `oneOf` none of
    /// whose branches passed, with its causes: the branches evaluated again in the collect mode,
    /// only to explain the failure (every error of every branch, in branch order).
    #[cold]
    #[inline(never)]
    fn no_match(&self, keyword: Kw, nodes: &[NodeId], at: At<'_, '_>, cx: &mut Context) {
        let outer: Vec<ValidationError> = std::mem::take(&mut cx.errors);
        for (i, node) in nodes.iter().enumerate() {
            let branch: Location<'_> = at.location.in_place(Via::Index(keyword, index_u32(i)));
            self.eval::<true>(*node, at.instance, &branch, cx, None);
        }
        let causes: Vec<ValidationError> = std::mem::replace(&mut cx.errors, outer);
        self.report(&mut cx.errors, at.location, Some(keyword), || {
            ValidationErrorKind::NoSubschemaMatched {
                subschema_count: nodes.len(),
                causes,
            }
        });
    }

    fn not<const COLLECT: bool>(&self, node: NodeId, at: At<'_, '_>, cx: &mut Context) -> bool {
        let branch: Location<'_> = at.location.in_place(Via::Keyword(Kw::Not));
        let matched: bool = self.eval::<false>(node, at.instance, &branch, cx, None);
        // A branch cut short by the depth guard did not fail: it must not make `not` pass.
        if cx.depth_exceeded.is_some() {
            return false;
        }
        !matched
            || fail!(
                COLLECT,
                self,
                cx,
                at.location,
                Kw::Not,
                ValidationErrorKind::NotSchemaMatched
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
        let branch: Location<'_> = at.location.in_place(Via::Keyword(Kw::If));
        let passed: bool = self.in_place::<false>(
            condition,
            at.instance,
            &branch,
            cx,
            evaluated.as_deref_mut(),
        );
        let (node, keyword): (Option<NodeId>, Kw) = if passed {
            (then, Kw::Then)
        } else {
            (otherwise, Kw::Else)
        };
        match node {
            Some(node) => {
                let branch: Location<'_> = at.location.in_place(Via::Keyword(keyword));
                self.in_place::<COLLECT>(node, at.instance, &branch, cx, evaluated)
            }
            None => true,
        }
    }
}

/// A position in a keyword's array of subschemas, for its keyword location.
fn index_u32(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
