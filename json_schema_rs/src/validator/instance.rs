//! The instances the validator checks: any JSON representation that implements [`Instance`].
//!
//! [`serde_json::Value`] implements it, and every validation entry point takes `&impl Instance`,
//! so a document already held in another tree type (a `simd-json` value, a YAML or TOML tree,
//! a language binding's objects) is validated as it is, without converting it first.

use serde_json::{Map, Number, Value};
use std::borrow::Cow;

/// One JSON value of a representation the validator can check.
///
/// Implement [`Instance::view`] (what kind of value this is, and its contents) and
/// [`InstanceObject`] for the representation's objects; arrays are slices of the representation's
/// values. The validator reads instances only through these, so every keyword behaves exactly
/// as on the equal [`serde_json::Value`].
///
/// ```
/// use json_schema_rs::{
///     CompiledSchema, Instance, InstanceObject, InstanceRef, JsonSchemaSettings, Registry,
///     Schema,
/// };
/// use serde_json::{Number, json};
/// use std::borrow::Cow;
///
/// /// A small JSON tree with objects as lists of members.
/// enum Doc {
///     Int(i64),
///     Text(String),
///     List(Vec<Doc>),
///     Record(Members),
/// }
///
/// struct Members(Vec<(String, Doc)>);
///
/// impl Instance for Doc {
///     type Object = Members;
///
///     fn view(&self) -> InstanceRef<'_, Doc> {
///         match self {
///             Doc::Int(n) => InstanceRef::Number(Cow::Owned(Number::from(*n))),
///             Doc::Text(s) => InstanceRef::String(s),
///             Doc::List(items) => InstanceRef::Array(items),
///             Doc::Record(members) => InstanceRef::Object(members),
///         }
///     }
/// }
///
/// impl InstanceObject<Doc> for Members {
///     fn len(&self) -> usize {
///         self.0.len()
///     }
///
///     fn get(&self, name: &str) -> Option<&Doc> {
///         self.0.iter().find(|(key, _)| key == name).map(|(_, value)| value)
///     }
///
///     fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a str, &'a Doc)>
///     where
///         Doc: 'a,
///     {
///         self.0.iter().map(|(key, value)| (key.as_str(), value))
///     }
/// }
///
/// let settings: JsonSchemaSettings = JsonSchemaSettings::default();
/// let schema: Schema = Schema::new_from_serde_value(
///     &json!({"properties": {"tags": {"items": {"type": "string"}}}}),
///     &settings,
/// )
/// .unwrap();
/// let registry: Registry = Registry::new(&schema).unwrap();
/// let compiled: CompiledSchema = CompiledSchema::new(&registry).unwrap();
/// let doc: Doc = Doc::Record(Members(vec![(
///     "tags".to_string(),
///     Doc::List(vec![Doc::Text("a".to_string()), Doc::Int(1)]),
/// )]));
/// let errors: Vec<String> = compiled
///     .validate(&doc)
///     .unwrap_err()
///     .iter()
///     .map(ToString::to_string)
///     .collect();
/// assert_eq!(vec!["/tags/1: expected string, got number"], errors);
/// ```
pub trait Instance: Sized {
    /// The representation's objects.
    type Object: InstanceObject<Self> + ?Sized;

    /// What kind of value this is, with its contents.
    fn view(&self) -> InstanceRef<'_, Self>;

    /// This value as a [`serde_json::Value`], for what needs one: custom keywords
    /// ([`CustomKeyword`](super::CustomKeyword)), user formats that check whole values,
    /// error details that carry the value, and output-format annotations. None of these run on
    /// the path of a valid instance unless the schema uses them.
    ///
    /// The default builds the value from [`Instance::view`]; a representation that holds a
    /// `serde_json::Value` can lend it instead.
    fn to_value(&self) -> Cow<'_, Value> {
        Cow::Owned(build_value(self))
    }
}

/// One JSON value of an [`Instance`], seen through [`Instance::view`].
#[derive(Debug)]
pub enum InstanceRef<'a, I: Instance> {
    Null,
    Bool(bool),
    /// A number, compared mathematically as JSON numbers are: lend the representation's own
    /// [`serde_json::Number`] when it has one, or build one (`Number::from`, `Number::from_f64`).
    Number(Cow<'a, Number>),
    String(&'a str),
    Array(&'a [I]),
    Object(&'a I::Object),
}

/// The objects of an [`Instance`] representation `I`.
pub trait InstanceObject<I: Instance> {
    /// Whether [`InstanceObject::iter`] gives the members in ascending byte order of their
    /// names, which lets `required` and `properties` check an object in one merge walk. The
    /// default makes no such promise; claiming it falsely gives wrong verdicts.
    const SORTED_KEYS: bool = false;

    /// The number of members.
    fn len(&self) -> usize;

    /// Whether the object has no members.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The value of the member named `name`.
    fn get(&self, name: &str) -> Option<&I>;

    /// Whether the object has a member named `name`.
    fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The members, as name and value. Every call gives them in the same order: annotations
    /// (`unevaluatedProperties`) identify a member by its position in it.
    fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a str, &'a I)>
    where
        I: 'a;
}

impl Instance for Value {
    type Object = Map<String, Value>;

    #[inline]
    fn view(&self) -> InstanceRef<'_, Value> {
        match self {
            Value::Null => InstanceRef::Null,
            Value::Bool(b) => InstanceRef::Bool(*b),
            Value::Number(n) => InstanceRef::Number(Cow::Borrowed(n)),
            Value::String(s) => InstanceRef::String(s),
            Value::Array(items) => InstanceRef::Array(items),
            Value::Object(object) => InstanceRef::Object(object),
        }
    }

    #[inline]
    fn to_value(&self) -> Cow<'_, Value> {
        Cow::Borrowed(self)
    }
}

impl InstanceObject<Value> for Map<String, Value> {
    /// `serde_json`'s map is sorted unless its `preserve_order` feature is on; a compiled schema
    /// checks which (see `CompiledSchema::sorted_maps`), as no feature of this crate decides it.
    const SORTED_KEYS: bool = true;

    #[inline]
    fn len(&self) -> usize {
        Map::len(self)
    }

    #[inline]
    fn get(&self, name: &str) -> Option<&Value> {
        Map::get(self, name)
    }

    #[inline]
    fn contains_key(&self, name: &str) -> bool {
        Map::contains_key(self, name)
    }

    #[inline]
    fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a str, &'a Value)>
    where
        Value: 'a,
    {
        Map::iter(self).map(|(name, value)| (name.as_str(), value))
    }
}

/// The JSON type of `instance`, as the validator's error messages name it.
pub(crate) fn type_name<I: Instance>(instance: &I) -> &'static str {
    match instance.view() {
        InstanceRef::Null => "null",
        InstanceRef::Bool(_) => "boolean",
        InstanceRef::Number(_) => "number",
        InstanceRef::String(_) => "string",
        InstanceRef::Array(_) => "array",
        InstanceRef::Object(_) => "object",
    }
}

/// One container of [`build_value`]'s walk, its children converted so far.
enum Frame<'a, I: Instance> {
    Array {
        items: std::slice::Iter<'a, I>,
        built: Vec<Value>,
    },
    Object {
        members: Vec<(&'a str, &'a I)>,
        next: usize,
        built: Map<String, Value>,
    },
}

/// `instance` as a [`serde_json::Value`]. Iterative, so a deep instance cannot overflow the
/// stack.
fn build_value<I: Instance>(instance: &I) -> Value {
    let mut stack: Vec<Frame<'_, I>> = Vec::new();
    let mut next: Option<&I> = Some(instance);
    loop {
        // Descend into `next`, or finish the innermost container with the value built last.
        let mut done: Option<Value> = None;
        if let Some(value) = next.take() {
            match value.view() {
                InstanceRef::Null => done = Some(Value::Null),
                InstanceRef::Bool(b) => done = Some(Value::Bool(b)),
                InstanceRef::Number(n) => done = Some(Value::Number(n.into_owned())),
                InstanceRef::String(s) => done = Some(Value::String(s.to_string())),
                InstanceRef::Array(items) => stack.push(Frame::Array {
                    items: items.iter(),
                    built: Vec::with_capacity(items.len()),
                }),
                InstanceRef::Object(object) => stack.push(Frame::Object {
                    members: object.iter().collect(),
                    next: 0,
                    built: Map::new(),
                }),
            }
        }
        // Hand finished values to their containers until one has a child left to convert.
        loop {
            match (stack.last_mut(), done.take()) {
                (None, Some(value)) => return value,
                (Some(Frame::Array { built, .. }), Some(value)) => built.push(value),
                (
                    Some(Frame::Object {
                        members,
                        next,
                        built,
                    }),
                    Some(value),
                ) => {
                    built.insert(members[*next].0.to_string(), value);
                    *next += 1;
                }
                (_, None) => {}
            }
            let pending: Option<&I> = match stack.last_mut() {
                Some(Frame::Array { items, .. }) => items.next(),
                Some(Frame::Object { members, next, .. }) => {
                    members.get(*next).map(|(_, value)| *value)
                }
                None => None,
            };
            if let Some(value) = pending {
                next = Some(value);
                break;
            }
            done = match stack.pop() {
                Some(Frame::Array { built, .. }) => Some(Value::Array(built)),
                Some(Frame::Object { built, .. }) => Some(Value::Object(built)),
                None => unreachable!("a container is open while one is being finished"),
            };
        }
    }
}
