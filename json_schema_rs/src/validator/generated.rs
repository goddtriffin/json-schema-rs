//! The validation runtime of generated types: the schema documents embedded in generated code,
//! compiled once on first use, validating each generated type against the schemas it was
//! generated from.
//!
//! Generated code holds one [`TypeValidators`] in a `static`: the embedded documents (the root
//! first) and, per generated type, the absolute URIs of the schemas the type must satisfy. On
//! first use it parses the documents, builds one [`Registry`] with every type's URIs as entries,
//! and compiles one [`CompiledSchema`] that shares the nodes the types have in common; then each
//! type validates at its own nodes, with the errors [`CompiledSchema::validate`] gives for that
//! schema.
//!
//! # Trusted deserialization
//!
//! Each value is validated once per entry point (#86). Deserialization that runs right after its
//! value passed validation is *trusted*: the deserialization in [`TypeValidators::deserialize`]
//! (a type's `TryFrom<serde_json::Value>`, after the whole instance is validated) and a union
//! variant's in [`TypeValidators::deserialize_union`] (after the value satisfied the variant's
//! schema). Inside it, a constrained newtype's `Deserialize` skips its check
//! ([`TypeValidators::read_newtype`]) and a union takes the first variant its value satisfies:
//! a valid parent guarantees both, since codegen never gives a child type a narrower schema than
//! a valid parent allows at its position (#107). The scope is a thread-local depth, entered and
//! left around those deserializations only, so a plain `serde_json::from_str` of a generated type
//! still checks every newtype and union it reads.
//!
//! # Union choices
//!
//! The validation in [`TypeValidators::deserialize`] records which branch each `anyOf` / `oneOf`
//! chose for each object or array it was evaluated on, keyed by the value's address and the
//! union's (#128). The instance is then read from a [`ValueRef`] over it rather than moved, so
//! addresses stay valid: a union's `Deserialize` asks for its value's address
//! (`deserialize_newtype_struct` with a private name, which any other deserializer passes
//! through) and reads the recorded variant directly, without evaluating any variant again. A
//! value with no recorded choice, or any other deserializer, takes the buffered path.

use super::compiled::{CompiledEntries, Keyword, LocatedEntries, NodeId, Union};
use super::eval::{UnionChoices, union_address};
use super::value_ref::{UNION_TOKEN, ValueRef, take_union_value};
use super::{CompileErrors, CompiledSchema, SchemaError, ValidationError};
use crate::error_list::ErrorList;
use crate::json_schema::{
    DialectSettings, JsonSchemaParseError, JsonSchemaParseErrors, JsonSchemaSettings, Registry,
    RegistryBuilder, Schema,
};
use serde::de::{DeserializeOwned, Error as _, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::sync::{Arc, OnceLock};

thread_local! {
    /// How many trusted scopes the current thread is in (see the module docs, *Trusted
    /// deserialization*).
    static TRUSTED: Cell<usize> = const { Cell::new(0) };

    /// The union choices of the instance [`TypeValidators::deserialize`] is reading (see the
    /// module docs, *Union choices*).
    static CHOICES: RefCell<UnionChoices> = RefCell::new(UnionChoices::default());
}

/// Runs `read` with `choices` as the thread's union choices, restoring the previous ones after,
/// also when `read` panics.
fn with_choices<R>(choices: UnionChoices, read: impl FnOnce() -> R) -> R {
    /// Puts the previous choices back when dropped.
    struct Restore(Option<UnionChoices>);

    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                CHOICES.with(|cell: &RefCell<UnionChoices>| *cell.borrow_mut() = previous);
            }
        }
    }

    let previous: UnionChoices = CHOICES
        .with(|cell: &RefCell<UnionChoices>| std::mem::replace(&mut *cell.borrow_mut(), choices));
    let _restore: Restore = Restore(Some(previous));
    read()
}

/// Whether the current thread deserializes inside a trusted scope.
fn trusted() -> bool {
    TRUSTED.with(Cell::get) > 0
}

/// Runs `read` inside a trusted scope: the deserialization of a value that passed validation.
fn read_trusted<R>(read: impl FnOnce() -> R) -> R {
    /// Leaves the scope when dropped, also when `read` panics.
    struct Scope;

    impl Drop for Scope {
        fn drop(&mut self) {
            TRUSTED.with(|depth: &Cell<usize>| depth.set(depth.get() - 1));
        }
    }

    TRUSTED.with(|depth: &Cell<usize>| depth.set(depth.get() + 1));
    let _scope: Scope = Scope;
    read()
}

/// How an embedded document enters the registry of a [`TypeValidators`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedDocumentKind {
    /// A document the caller parsed (the root, other code generation inputs): parsed with these
    /// dialect settings (`None`: the defaults, following its `$schema`) and given to the registry
    /// as a parsed document (the first document is the registry's root).
    Parsed(Option<DialectSettings>),
    /// A document the registry loaded (retrieved or given as a raw document): given to the
    /// registry as a raw document ([`RegistryBuilder::document`]), so the registry parses it as
    /// it did when loading it (following its `$schema`, else the draft of the resource that
    /// references it).
    Loaded,
}

/// A schema document embedded in generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedDocument<'a> {
    /// The document's retrieval URI (absolute). The first document's is the registry's base URI.
    pub uri: &'a str,
    /// The document as JSON text.
    pub json: &'a str,
    /// How it enters the registry.
    pub kind: EmbeddedDocumentKind,
}

/// The embedded documents could not be parsed: the problems of one document.
#[derive(Debug)]
pub struct EmbeddedDocumentError {
    /// The document's URI.
    pub uri: String,
    /// Every problem parsing it (invalid JSON is the only problem when the JSON is invalid).
    pub errors: JsonSchemaParseErrors,
}

/// Prints every problem, one per line, each prefixed with the document's URI.
impl fmt::Display for EmbeddedDocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, error) in self.errors.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "\"{}\": {error}", self.uri)?;
        }
        Ok(())
    }
}

impl std::error::Error for EmbeddedDocumentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.errors)
    }
}

/// Why the schemas embedded in a [`TypeValidators`] cannot be used. Every problem found at the
/// step that failed is listed.
#[derive(Debug)]
pub enum EmbeddedSchemaError {
    /// Documents are not valid JSON or not valid schemas: one entry per such document, in
    /// document order.
    Documents(ErrorList<EmbeddedDocumentError>),
    /// The registry could not be built or the schemas did not compile.
    Schema(SchemaError),
}

/// Prints every problem, one per line.
impl fmt::Display for EmbeddedSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EmbeddedSchemaError::Documents(errors) => write!(f, "{errors}"),
            EmbeddedSchemaError::Schema(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for EmbeddedSchemaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EmbeddedSchemaError::Documents(errors) => Some(errors),
            EmbeddedSchemaError::Schema(error) => Some(error),
        }
    }
}

impl From<SchemaError> for EmbeddedSchemaError {
    fn from(error: SchemaError) -> Self {
        EmbeddedSchemaError::Schema(error)
    }
}

/// The variants of a generated `anyOf` / `oneOf` union, read by position in its `variants` list
/// (see [`TypeValidators::deserialize_union`]). Generated code implements it with
/// `json_schema_rs::deserialize_union!`.
pub trait UnionVariants: Sized {
    /// Deserializes the variant at `position` from `deserializer`.
    ///
    /// # Errors
    ///
    /// When the value is not that variant.
    fn read_variant<'de, D: Deserializer<'de>>(
        position: usize,
        deserializer: D,
    ) -> Result<Self, D::Error>;
}

/// Reads a union through [`TypeValidators::deserialize_union`]: the recorded variant when the
/// deserializer gave the value's address and a choice was recorded for it, else the buffered
/// path.
struct UnionVisitor<'v, 'a, T> {
    validators: &'v TypeValidators<'a>,
    name: &'static str,
    kind: UnionMatch,
    union: Option<usize>,
    variants: &'v [usize],
    union_type: std::marker::PhantomData<T>,
}

impl<'de, T: UnionVariants> Visitor<'de> for UnionVisitor<'_, '_, T> {
    type Value = T;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a {} variant", self.name)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(self, deserializer: D) -> Result<T, D::Error> {
        if let (Some(address), Some(union)) = (take_union_value(), self.union)
            && let Some(position) = self
                .validators
                .recorded_variant(address, union, self.variants)
        {
            return T::read_variant(position, deserializer);
        }
        let value: Value = Value::deserialize(deserializer)?;
        self.validators
            .read_union(
                self.kind,
                self.union,
                self.variants,
                &value,
                |position, value| T::read_variant(position, value),
            )
            .map_err(|error: Option<TypeValidationError>| match error {
                Some(error) => D::Error::custom(error),
                None => D::Error::custom(format!(
                    "{value} does not deserialize into a {} variant whose schema it satisfies",
                    self.name
                )),
            })
    }
}

/// Which variants of a union must match (see [`TypeValidators::deserialize_union`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionMatch {
    /// `anyOf`: at least one.
    AnyOf,
    /// `oneOf`: exactly one.
    OneOf,
}

impl fmt::Display for UnionMatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnionMatch::AnyOf => write!(f, "anyOf"),
            UnionMatch::OneOf => write!(f, "oneOf"),
        }
    }
}

/// Why a generated type rejected a value (its `validate`, `TryFrom` or `Deserialize`).
#[derive(Debug)]
pub enum TypeValidationError {
    /// The instance does not conform to the type's schemas: every error, as
    /// [`CompiledSchema::validate`] gives them.
    Invalid(Vec<ValidationError>),
    /// Serializing the value or deserializing the instance failed.
    Json(serde_json::Error),
    /// The embedded schemas cannot be used (generated code is checked when it is generated, so
    /// this means the embedded documents were changed).
    Schema(Arc<EmbeddedSchemaError>),
    /// A union's variants did not match as its keyword requires, and no union schema was given
    /// to explain it (see [`TypeValidators::deserialize_union`]).
    UnionMismatch {
        /// The union's keyword.
        kind: UnionMatch,
        /// How many variants matched: none, or (for `oneOf`) 2 when two or more did.
        matched: usize,
    },
    /// A constrained newtype was given a float that is not a JSON number (`NaN` or an
    /// infinity).
    NonFinite(f64),
}

/// Prints every problem, one per line.
impl fmt::Display for TypeValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeValidationError::Invalid(errors) => {
                for (index, error) in errors.iter().enumerate() {
                    if index > 0 {
                        writeln!(f)?;
                    }
                    write!(f, "{error}")?;
                }
                Ok(())
            }
            TypeValidationError::Json(error) => write!(f, "{error}"),
            TypeValidationError::Schema(error) => write!(f, "{error}"),
            TypeValidationError::UnionMismatch { kind, matched: 0 } => {
                write!(f, "no variant of the {kind} union matched")
            }
            TypeValidationError::UnionMismatch { kind, .. } => {
                write!(f, "more than one variant of the {kind} union matched")
            }
            TypeValidationError::NonFinite(value) => write!(f, "{value} is not a JSON number"),
        }
    }
}

impl std::error::Error for TypeValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TypeValidationError::Json(error) => Some(error),
            TypeValidationError::Schema(error) => Some(&**error),
            TypeValidationError::Invalid(_)
            | TypeValidationError::UnionMismatch { .. }
            | TypeValidationError::NonFinite(_) => None,
        }
    }
}

impl From<serde_json::Error> for TypeValidationError {
    fn from(error: serde_json::Error) -> Self {
        TypeValidationError::Json(error)
    }
}

impl From<Arc<EmbeddedSchemaError>> for TypeValidationError {
    fn from(error: Arc<EmbeddedSchemaError>) -> Self {
        TypeValidationError::Schema(error)
    }
}

/// A value a generated constrained newtype wraps (see
/// [`TypeValidators::validate_newtype`]): a string, boolean, integer or float, seen by
/// validation as the JSON value it serializes to.
pub trait NewtypeValue: Sized + sealed::Sealed {
    /// The JSON instance validation sees.
    ///
    /// # Errors
    ///
    /// [`TypeValidationError::NonFinite`] for a float that is not a JSON number.
    fn into_instance(self) -> Result<Value, TypeValidationError>;

    /// The value back from the instance [`NewtypeValue::into_instance`] gave.
    ///
    /// # Errors
    ///
    /// [`TypeValidationError::Json`] when `instance` is not of this type.
    fn from_instance(instance: Value) -> Result<Self, TypeValidationError>;
}

mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for String {}

impl NewtypeValue for String {
    fn into_instance(self) -> Result<Value, TypeValidationError> {
        Ok(Value::String(self))
    }

    fn from_instance(instance: Value) -> Result<Self, TypeValidationError> {
        match instance {
            Value::String(value) => Ok(value),
            other => Ok(serde_json::from_value(other)?),
        }
    }
}

/// [`NewtypeValue`] for types that convert into a [`Value`] without loss.
macro_rules! newtype_value {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl NewtypeValue for $ty {
            fn into_instance(self) -> Result<Value, TypeValidationError> {
                Ok(Value::from(self))
            }

            fn from_instance(instance: Value) -> Result<Self, TypeValidationError> {
                Ok(serde_json::from_value(instance)?)
            }
        }
    )*};
}

newtype_value!(bool, u8, u16, u32, u64, i8, i16, i32, i64);

/// [`NewtypeValue`] for floats: a non-finite value is not a JSON number.
macro_rules! newtype_float {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl NewtypeValue for $ty {
            fn into_instance(self) -> Result<Value, TypeValidationError> {
                serde_json::Number::from_f64(f64::from(self))
                    .map(Value::Number)
                    .ok_or(TypeValidationError::NonFinite(f64::from(self)))
            }

            fn from_instance(instance: Value) -> Result<Self, TypeValidationError> {
                Ok(serde_json::from_value(instance)?)
            }
        }
    )*};
}

newtype_float!(f32, f64);

/// The compiled schemas of every type, or why they cannot be used.
type Compiled = Result<CompiledEntries, Arc<EmbeddedSchemaError>>;

/// The validators of the types generated from one set of schema documents, compiled once on
/// first use. Built in a `static` by generated code:
///
/// ```
/// use json_schema_rs::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
/// use serde_json::json;
///
/// static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
///     None,
///     &[EmbeddedDocument {
///         uri: "json-schema:///",
///         json: r#"{"$defs":{"Name":{"type":"string","minLength":1}}}"#,
///         kind: EmbeddedDocumentKind::Parsed(None),
///     }],
///     &[&["json-schema:///#/$defs/Name"]],
/// );
///
/// assert!(VALIDATORS.is_valid(0, &json!("Ada")));
/// assert!(!VALIDATORS.is_valid(0, &json!("")));
/// ```
pub struct TypeValidators<'a> {
    settings: Option<DialectSettings>,
    documents: &'a [EmbeddedDocument<'a>],
    types: &'a [&'a [&'a str]],
    compiled: OnceLock<Compiled>,
}

impl fmt::Debug for TypeValidators<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypeValidators")
            .field("settings", &self.settings)
            .field("documents", &self.documents)
            .field("types", &self.types)
            .field("compiled", &self.compiled.get().map(Result::is_ok))
            .finish()
    }
}

impl<'a> TypeValidators<'a> {
    /// The validators of `types` over `documents`. `settings`: the dialect settings the registry
    /// parses loaded documents with (`None`: the defaults). `documents[0]` is the root (its `uri`
    /// is the registry's base URI); `types[i]` holds the absolute URIs of the schemas type `i`
    /// must satisfy, all of them. Nothing is parsed or compiled until first use.
    #[must_use]
    pub const fn new(
        settings: Option<DialectSettings>,
        documents: &'a [EmbeddedDocument<'a>],
        types: &'a [&'a [&'a str]],
    ) -> Self {
        TypeValidators {
            settings,
            documents,
            types,
            compiled: OnceLock::new(),
        }
    }

    /// Parses and compiles the embedded schemas now (the first validation does it otherwise).
    ///
    /// # Errors
    ///
    /// Returns why the schemas cannot be used, listing every problem: documents that are not
    /// valid JSON or not valid schemas, else a registry that could not be built (a URI that does
    /// not resolve, ...), else schemas that did not compile (an invalid `pattern`, ...).
    pub fn check(&self) -> Result<(), Arc<EmbeddedSchemaError>> {
        self.compiled().map(|_| ())
    }

    /// Whether `instance` conforms to type `index`. `false` when the embedded schemas cannot be
    /// used.
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    #[must_use]
    pub fn is_valid(&self, index: usize, instance: &Value) -> bool {
        self.compiled()
            .is_ok_and(|(schema, types)| schema.is_valid_at(&types[index], instance))
    }

    /// Validates `instance` as type `index`.
    ///
    /// # Errors
    ///
    /// [`TypeValidationError::Invalid`] with every error when `instance` does not conform;
    /// [`TypeValidationError::Schema`] when the embedded schemas cannot be used.
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn validate(&self, index: usize, instance: &Value) -> Result<(), TypeValidationError> {
        let (schema, types) = self.compiled()?;
        schema
            .validate_at(&types[index], instance)
            .map_err(TypeValidationError::Invalid)
    }

    /// Serializes `value` with `serde_json` and validates the result as type `index`.
    ///
    /// # Errors
    ///
    /// [`TypeValidationError::Json`] when serializing fails; otherwise see
    /// [`TypeValidators::validate`].
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn validate_serialized<T: Serialize + ?Sized>(
        &self,
        index: usize,
        value: &T,
    ) -> Result<(), TypeValidationError> {
        let instance: Value = serde_json::to_value(value)?;
        self.validate(index, &instance)
    }

    /// Validates `instance` as type `index`, then deserializes it trusted: the newtypes and
    /// unions it holds are not checked again (see the module docs, *Trusted deserialization*).
    ///
    /// # Errors
    ///
    /// See [`TypeValidators::validate`]; [`TypeValidationError::Json`] when deserializing fails.
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn deserialize<T: DeserializeOwned>(
        &self,
        index: usize,
        instance: Value,
    ) -> Result<T, TypeValidationError> {
        let (schema, types) = self.compiled()?;
        if !schema.has_unions {
            self.validate(index, &instance)?;
            return Ok(read_trusted(|| serde_json::from_value(instance))?);
        }
        let (result, choices) = schema.validate_recording_at(&types[index], &instance);
        result.map_err(TypeValidationError::Invalid)?;
        if choices.is_empty() {
            return Ok(read_trusted(|| serde_json::from_value(instance))?);
        }
        Ok(with_choices(choices, || {
            read_trusted(|| T::deserialize(ValueRef(&instance)))
        })?)
    }

    /// For a generated constrained newtype: validates `value` as type `index` and gives it back
    /// when it conforms. The value is moved into the instance and out again, never copied.
    ///
    /// ```
    /// use json_schema_rs::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
    ///
    /// static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
    ///     None,
    ///     &[EmbeddedDocument {
    ///         uri: "json-schema:///",
    ///         json: r#"{"type":"integer","multipleOf":5}"#,
    ///         kind: EmbeddedDocumentKind::Parsed(None),
    ///     }],
    ///     &[&["json-schema:///#"]],
    /// );
    ///
    /// assert_eq!(VALIDATORS.validate_newtype(0, 10_u8).ok(), Some(10));
    /// assert!(VALIDATORS.validate_newtype(0, 12_u8).is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// See [`TypeValidators::validate`]; [`TypeValidationError::NonFinite`] for a float that is
    /// not a JSON number.
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn validate_newtype<T: NewtypeValue>(
        &self,
        index: usize,
        value: T,
    ) -> Result<T, TypeValidationError> {
        self.validate_newtype_as(index, value)
    }

    /// [`TypeValidators::validate_newtype`] for a value read as another type: validates `value`
    /// as type `index`, then converts the instance to `T` (an `f32` newtype validates the `f64`
    /// it read, so a number just outside a bound cannot round into it).
    ///
    /// # Errors
    ///
    /// See [`TypeValidators::validate_newtype`]; [`TypeValidationError::Json`] when the instance
    /// is not a `T`.
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn validate_newtype_as<T: NewtypeValue, U: NewtypeValue>(
        &self,
        index: usize,
        value: U,
    ) -> Result<T, TypeValidationError> {
        let instance: Value = value.into_instance()?;
        self.validate(index, &instance)?;
        T::from_instance(instance)
    }

    /// For a generated constrained newtype's `Deserialize`: `value` when the deserialization is
    /// trusted (see the module docs, *Trusted deserialization*), else
    /// [`TypeValidators::validate_newtype`].
    ///
    /// # Errors
    ///
    /// See [`TypeValidators::validate_newtype`] (never when trusted).
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn read_newtype<T: NewtypeValue>(
        &self,
        index: usize,
        value: T,
    ) -> Result<T, TypeValidationError> {
        if trusted() {
            return Ok(value);
        }
        self.validate_newtype(index, value)
    }

    /// [`TypeValidators::read_newtype`] for a value read as another type: converts it to `T`
    /// when the deserialization is trusted, else [`TypeValidators::validate_newtype_as`].
    ///
    /// # Errors
    ///
    /// See [`TypeValidators::validate_newtype_as`].
    ///
    /// # Panics
    ///
    /// When `index` is not a type of these validators.
    pub fn read_newtype_as<T: NewtypeValue, U: NewtypeValue>(
        &self,
        index: usize,
        value: U,
    ) -> Result<T, TypeValidationError> {
        if trusted() {
            return T::from_instance(value.into_instance()?);
        }
        self.validate_newtype_as(index, value)
    }

    /// The `Deserialize` of a generated `anyOf` / `oneOf` union: reads the value once (buffered
    /// as a [`Value`]) and returns the first variant, in order, whose schema the value satisfies
    /// (for `oneOf`, the one) and that `read` deserializes; `read(position, value)` deserializes
    /// the variant at `position` in `variants` (type indexes). Each variant is read trusted, since
    /// its schema just accepted the value; a union read trusted takes the first variant that
    /// matches without checking that no other does (see the module docs, *Trusted
    /// deserialization*). An `anyOf` evaluates its variants in order and stops at the first that
    /// matches and reads.
    ///
    /// # Errors
    ///
    /// When the variants do not match as the keyword requires (none, or for `oneOf` more than
    /// one): the errors of validating the value as type `union` (the union's own schema, which
    /// reports why), or [`TypeValidationError::UnionMismatch`] when `union` is `None`; `<value> does not deserialize into a <name> variant whose schema it
    /// satisfies` when variants match but none deserializes.
    ///
    /// # Panics
    ///
    /// When `union` or an entry of `variants` is not a type of these validators.
    pub fn deserialize_union<'de, D: Deserializer<'de>, T: UnionVariants>(
        &self,
        deserializer: D,
        name: &'static str,
        kind: UnionMatch,
        union: Option<usize>,
        variants: &[usize],
    ) -> Result<T, D::Error> {
        deserializer.deserialize_newtype_struct(
            UNION_TOKEN,
            UnionVisitor {
                validators: self,
                name,
                kind,
                union,
                variants,
                union_type: std::marker::PhantomData,
            },
        )
    }

    /// The position in `variants` of the variant the recorded choice for the value at
    /// `address` names, for the union of type `union`: `None` when nothing was recorded, or the
    /// branch is not exactly one variant's.
    fn recorded_variant(&self, address: usize, union: usize, variants: &[usize]) -> Option<usize> {
        let (schema, types) = self.compiled().ok()?;
        let mut unions = types[union].iter().flat_map(|node: &NodeId| {
            schema
                .node(*node)
                .keywords
                .iter()
                .filter_map(|keyword| match keyword {
                    Keyword::AnyOf(union) | Keyword::OneOf(union) => Some(&**union),
                    _ => None,
                })
        });
        let union: &Union = unions.next()?;
        if unions.next().is_some() {
            return None;
        }
        let key: (usize, usize) = (address, union_address(union));
        let branch: u32 = CHOICES.with(|cell: &RefCell<UnionChoices>| cell.borrow().get(key))?;
        let mut node: NodeId = *union.nodes.get(branch as usize)?;
        let holds = |node: NodeId| {
            variants
                .iter()
                .filter(|v| types[**v].contains(&node))
                .count()
        };
        // A `$ref` branch is its target's variant. A chain of `$ref`s visits each node at most
        // once unless it loops, so the node count bounds it.
        for _ in 0..schema.nodes.len() {
            match &*schema.node(node).keywords {
                [Keyword::Ref { target, .. }] if holds(node) == 0 => node = *target,
                _ => break,
            }
        }
        let mut positions = variants
            .iter()
            .enumerate()
            .filter(|(_, variant)| types[**variant].contains(&node))
            .map(|(position, _)| position);
        let position: usize = positions.next()?;
        positions.next().is_none().then_some(position)
    }

    /// [`TypeValidators::deserialize_union`] on the buffered value. `Err(None)`: variants match
    /// but none reads.
    fn read_union<T>(
        &self,
        kind: UnionMatch,
        union: Option<usize>,
        variants: &[usize],
        value: &Value,
        read: impl Fn(usize, &Value) -> Result<T, serde_json::Error>,
    ) -> Result<T, Option<TypeValidationError>> {
        if kind == UnionMatch::OneOf && !trusted() {
            // Which variant is the one is only known once every variant is evaluated.
            let candidates: Vec<usize> = self
                .union_candidates(kind, union, variants, value)
                .map_err(Some)?;
            return candidates
                .into_iter()
                .find_map(|position: usize| read_trusted(|| read(position, value)).ok())
                .ok_or(None);
        }
        let (schema, types) = self
            .compiled()
            .map_err(|error| Some(TypeValidationError::from(error)))?;
        let mut matched: bool = false;
        for (position, variant) in variants.iter().enumerate() {
            if schema.is_valid_at(&types[*variant], value) {
                matched = true;
                if let Ok(variant) = read_trusted(|| read(position, value)) {
                    return Ok(variant);
                }
            }
        }
        if matched {
            return Err(None);
        }
        // No variant matched: explain why as the union's schema does.
        Err(Some(
            self.union_candidates(kind, union, variants, value)
                .err()
                .unwrap_or(TypeValidationError::UnionMismatch { kind, matched: 0 }),
        ))
    }

    /// For a generated `anyOf` / `oneOf` union: the positions in `variants` (type indexes) of
    /// the variants `instance` conforms to, in order. `AnyOf` returns every matching variant
    /// (at least one); `OneOf` returns the one matching variant, and stops evaluating at a
    /// second match.
    ///
    /// # Errors
    ///
    /// When no variant matches, or more than one matches under `OneOf`: the errors of
    /// validating `instance` as type `union` (the union's own schema, which reports why, as
    /// [`CompiledSchema::validate`] does), or [`TypeValidationError::UnionMismatch`] when
    /// `union` is `None` or, unexpectedly, accepts it. [`TypeValidationError::Schema`] when the
    /// embedded schemas cannot be used.
    ///
    /// # Panics
    ///
    /// When `union` or an entry of `variants` is not a type of these validators.
    fn union_candidates(
        &self,
        kind: UnionMatch,
        union: Option<usize>,
        variants: &[usize],
        instance: &Value,
    ) -> Result<Vec<usize>, TypeValidationError> {
        let (schema, types) = self.compiled()?;
        let mut matched: Vec<usize> = Vec::new();
        for (position, variant) in variants.iter().enumerate() {
            if schema.is_valid_at(&types[*variant], instance) {
                matched.push(position);
                if kind == UnionMatch::OneOf && matched.len() > 1 {
                    break;
                }
            }
        }
        let fits: bool = match kind {
            UnionMatch::AnyOf => !matched.is_empty(),
            UnionMatch::OneOf => matched.len() == 1,
        };
        if fits {
            return Ok(matched);
        }
        if let Some(union) = union
            && let Err(errors) = schema.validate_at(&types[union], instance)
        {
            return Err(TypeValidationError::Invalid(errors));
        }
        Err(TypeValidationError::UnionMismatch {
            kind,
            matched: matched.len(),
        })
    }

    /// The compiled schemas, compiling them on first use.
    fn compiled(&self) -> Result<&CompiledEntries, Arc<EmbeddedSchemaError>> {
        self.compiled
            .get_or_init(|| {
                self.compile().map_err(|error| {
                    log::debug!("json-schema-rs: the embedded schemas cannot be used: {error}");
                    Arc::new(error)
                })
            })
            .as_ref()
            .map_err(Arc::clone)
    }

    /// Parses the documents, builds the registry with every type's URIs as entries, and
    /// compiles them.
    pub(crate) fn compile(&self) -> Result<CompiledEntries, EmbeddedSchemaError> {
        self.compile_with(CompiledSchema::with_entries)
    }

    /// [`TypeValidators::compile`], also returning where every compiled schema sits (see
    /// [`CompiledSchema::with_entries_located`]): what codegen prunes the documents to.
    pub(crate) fn compile_located(&self) -> Result<LocatedEntries, EmbeddedSchemaError> {
        self.compile_with(CompiledSchema::with_entries_located)
    }

    /// Parses the documents, builds the registry with every type's URIs as entries, and
    /// compiles it with `compile`.
    fn compile_with<T>(
        &self,
        compile: impl FnOnce(&Registry<'_>, &[&[&str]]) -> Result<T, CompileErrors>,
    ) -> Result<T, EmbeddedSchemaError> {
        let registry_settings: JsonSchemaSettings =
            JsonSchemaSettings::from_dialect_settings(self.settings.unwrap_or_default());
        let mut errors: Vec<EmbeddedDocumentError> = Vec::new();
        // The root's schema, then each other document as a parsed schema or a raw value.
        let mut root: Option<Schema> = None;
        let mut parsed: Vec<(&str, Schema)> = Vec::new();
        let mut loaded: Vec<(&str, Value)> = Vec::new();
        for (index, document) in self.documents.iter().enumerate() {
            let result: Result<(), JsonSchemaParseErrors> = match document.kind {
                EmbeddedDocumentKind::Parsed(dialect) => {
                    let settings: JsonSchemaSettings =
                        JsonSchemaSettings::from_dialect_settings(dialect.unwrap_or_default());
                    Schema::new_from_str(document.json, &settings).map(|schema| {
                        if index == 0 {
                            root = Some(schema);
                        } else {
                            parsed.push((document.uri, schema));
                        }
                    })
                }
                EmbeddedDocumentKind::Loaded if index == 0 => {
                    Schema::new_from_str(document.json, &registry_settings).map(|schema| {
                        root = Some(schema);
                    })
                }
                EmbeddedDocumentKind::Loaded => serde_json::from_str::<Value>(document.json)
                    .map(|value| loaded.push((document.uri, value)))
                    .map_err(|error| ErrorList::new(JsonSchemaParseError::Serde(error))),
            };
            if let Err(parse_errors) = result {
                errors.push(EmbeddedDocumentError {
                    uri: document.uri.to_string(),
                    errors: parse_errors,
                });
            }
        }
        if let Some(errors) = ErrorList::from_vec(errors) {
            return Err(EmbeddedSchemaError::Documents(errors));
        }
        let root: Schema = root.unwrap_or(Schema::Bool(true));
        let mut builder: RegistryBuilder<'_> = Registry::builder().settings(registry_settings);
        if let Some(document) = self.documents.first() {
            builder = builder.base_uri(document.uri);
        }
        for (uri, schema) in &parsed {
            builder = builder.parsed_document(*uri, schema);
        }
        for (uri, value) in loaded {
            builder = builder.document(uri, value);
        }
        for uri in self.types.iter().flat_map(|uris| uris.iter()) {
            builder = builder.entry(*uri);
        }
        let registry: Registry<'_> = builder.build(&root).map_err(SchemaError::Registry)?;
        Ok(compile(&registry, self.types).map_err(SchemaError::Compile)?)
    }
}

/// The `Deserialize` of a generated `anyOf` / `oneOf` union, through
/// [`TypeValidators::deserialize_union`]: `name`, the validators static, the [`UnionMatch`]
/// variant, the union's type index (or `None`), the variants' type indexes, and one arm per
/// variant position reading it from the deserializer bound to `value`.
///
/// ```ignore
/// json_schema_rs::deserialize_union!(Shape, TYPE_VALIDATORS, OneOf, Some(4), [1, 3], |value| {
///     0 => <Circle as Deserialize>::deserialize(value).map(Self::Circle),
///     _ => <Square as Deserialize>::deserialize(value).map(Self::Square),
/// });
/// ```
#[macro_export]
macro_rules! deserialize_union {
    (
        $name:ident, $validators:expr, $kind:ident, $union:expr, [$($variant:expr),* $(,)?],
        |$value:ident| { $($position:pat => $read:expr),+ $(,)? }
    ) => {
        impl $crate::UnionVariants for $name {
            fn read_variant<'de, D: ::serde::Deserializer<'de>>(
                position: usize,
                $value: D,
            ) -> ::core::result::Result<Self, D::Error> {
                match position {
                    $($position => $read,)+
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                $validators.deserialize_union(
                    deserializer,
                    stringify!($name),
                    $crate::UnionMatch::$kind,
                    $union,
                    &[$($variant),*],
                )
            }
        }
    };
}

/// The self-validation of a struct, enum or union that generated code defines, through a
/// [`TypeValidators`] static: `validate(&self)` (the value as it serializes) and
/// `TryFrom<serde_json::Value>` (validates, then deserializes). Both report every error the
/// schema's validator reports. One implementation for every generated type, so the generated
/// code holds the type and one line.
///
/// `self_validating!(Point, VALIDATORS, 0)`: `VALIDATORS` is the static, `0` the type's entry.
///
/// ```
/// use json_schema_rs::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
///
/// static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
///     None,
///     &[EmbeddedDocument {
///         uri: "json-schema:///",
///         json: r#"{"type":"object","properties":{"x":{"minimum":0}}}"#,
///         kind: EmbeddedDocumentKind::Parsed(None),
///     }],
///     &[&["json-schema:///#"]],
/// );
///
/// #[derive(Debug, serde::Serialize, serde::Deserialize)]
/// pub struct Point {
///     x: i64,
/// }
///
/// json_schema_rs::self_validating!(Point, VALIDATORS, 0);
///
/// assert!(Point::try_from(serde_json::json!({"x": 1})).is_ok());
/// assert!(Point::try_from(serde_json::json!({"x": -1})).is_err());
/// assert!(Point { x: -1 }.validate().is_err());
/// ```
#[macro_export]
macro_rules! self_validating {
    ($name:ident, $validators:ident, $index:expr) => {
        impl $name {
            /// Validates this value against its schema.
            ///
            /// # Errors
            ///
            /// Every error the schema's validator reports, when this value does not satisfy the
            /// schema.
            pub fn validate(&self) -> Result<(), $crate::TypeValidationError> {
                $validators.validate_serialized($index, self)
            }
        }

        impl TryFrom<::serde_json::Value> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema, then deserializes it.
            // Not `Self::Error`: an enum with a variant named `Error` makes it ambiguous.
            fn try_from(value: ::serde_json::Value) -> Result<Self, $crate::TypeValidationError> {
                $validators.deserialize($index, value)
            }
        }
    };
}

/// The constructors and readers of a constrained newtype that generated code defines
/// (`pub struct Name(String);`), validated through a [`TypeValidators`] static: `into_inner`,
/// `TryFrom` the scalar (validates), `From<Name>` for the scalar, `Deref`, `AsRef`, `Display`
/// and a validating `Deserialize`. Strings also get `TryFrom<&str>`, `FromStr` and
/// `AsRef<str>`. One implementation for every generated newtype, so the generated code holds
/// only the type and one line.
///
/// Forms (`VALIDATORS` is the static, `0` the newtype's entry):
/// - `constrained_newtype!(string, Name, VALIDATORS, 0)`: wraps `String`.
/// - `constrained_newtype!(scalar, Age, u8, VALIDATORS, 0)`: wraps a `Copy` scalar (`bool`, an
///   integer, `f64`).
/// - `constrained_newtype!(integer, Age, u8, VALIDATORS, 0)`: wraps an integer read by JSON
///   Schema's integer rule (draft 6 on: `1.0` is the integer 1; see
///   [`json_schema_rs::integer`](crate::integer)).
/// - `constrained_newtype!(f32, Ratio, VALIDATORS, 0)`: wraps `f32`, deserialized as the `f64`
///   read and validated before it is narrowed.
/// - `constrained_newtype!(uuid, Id, VALIDATORS, 0)`: wraps `uuid::Uuid`, validated as the
///   string read (and, for `TryFrom<Uuid>`, the string it serializes to).
///
/// ```
/// use json_schema_rs::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators};
///
/// static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
///     None,
///     &[EmbeddedDocument {
///         uri: "json-schema:///",
///         json: r#"{"type":"string","minLength":2}"#,
///         kind: EmbeddedDocumentKind::Parsed(None),
///     }],
///     &[&["json-schema:///#"]],
/// );
///
/// #[derive(Debug, serde::Serialize)]
/// #[serde(transparent)]
/// pub struct Name(String);
///
/// json_schema_rs::constrained_newtype!(string, Name, VALIDATORS, 0);
///
/// assert_eq!(Name::try_from("Ada").map(Name::into_inner).ok(), Some("Ada".to_string()));
/// assert!("A".parse::<Name>().is_err());
/// assert!(serde_json::from_str::<Name>(r#""A""#).is_err());
/// ```
#[macro_export]
macro_rules! constrained_newtype {
    (string, $name:ident, $validators:ident, $index:expr) => {
        impl TryFrom<String> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn try_from(value: String) -> Result<Self, Self::Error> {
                $validators.validate_newtype($index, value).map(Self)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::try_from(value.to_string())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(value.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $validators
                    .read_newtype($index, <String as ::serde::Deserialize>::deserialize(deserializer)?)
                    .map(Self)
                    .map_err(::serde::de::Error::custom)
            }
        }

        $crate::constrained_newtype!(@readers, $name, String);
    };
    (scalar, $name:ident, $inner:ty, $validators:ident, $index:expr) => {
        impl TryFrom<$inner> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn try_from(value: $inner) -> Result<Self, Self::Error> {
                $validators.validate_newtype($index, value).map(Self)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $validators
                    .read_newtype($index, <$inner as ::serde::Deserialize>::deserialize(deserializer)?)
                    .map(Self)
                    .map_err(::serde::de::Error::custom)
            }
        }

        $crate::constrained_newtype!(@as_ref, $name, $inner);
        $crate::constrained_newtype!(@readers, $name, $inner);
    };
    (integer, $name:ident, $inner:ty, $validators:ident, $index:expr) => {
        impl TryFrom<$inner> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn try_from(value: $inner) -> Result<Self, Self::Error> {
                $validators.validate_newtype($index, value).map(Self)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                $validators
                    .read_newtype($index, <$inner as $crate::integer::DeserializeInteger>::deserialize_integer(deserializer)?)
                    .map(Self)
                    .map_err(::serde::de::Error::custom)
            }
        }

        $crate::constrained_newtype!(@as_ref, $name, $inner);
        $crate::constrained_newtype!(@readers, $name, $inner);
    };
    (f32, $name:ident, $validators:ident, $index:expr) => {
        impl TryFrom<f32> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` against the schema: every error the schema's validator reports.
            fn try_from(value: f32) -> Result<Self, Self::Error> {
                $validators.validate_newtype($index, value).map(Self)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                // The number as read, before it is narrowed: a value just outside a bound must
                // not round into it.
                $validators
                    .read_newtype_as($index, <f64 as ::serde::Deserialize>::deserialize(deserializer)?)
                    .map(Self)
                    .map_err(::serde::de::Error::custom)
            }
        }

        $crate::constrained_newtype!(@as_ref, $name, f32);
        $crate::constrained_newtype!(@readers, $name, f32);
    };
    (uuid, $name:ident, $validators:ident, $index:expr) => {
        impl TryFrom<::uuid::Uuid> for $name {
            type Error = $crate::TypeValidationError;

            /// Validates `value` (as the string it serializes to) against the schema: every error
            /// the schema's validator reports.
            fn try_from(value: ::uuid::Uuid) -> Result<Self, Self::Error> {
                $validators.validate_newtype($index, value.to_string())?;
                Ok(Self(value))
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                // The string as read, before it is parsed.
                let text: String = $validators
                    .read_newtype($index, <String as ::serde::Deserialize>::deserialize(deserializer)?)
                    .map_err(::serde::de::Error::custom)?;
                ::uuid::Uuid::parse_str(&text).map(Self).map_err(::serde::de::Error::custom)
            }
        }

        $crate::constrained_newtype!(@as_ref, $name, ::uuid::Uuid);
        $crate::constrained_newtype!(@readers, $name, ::uuid::Uuid);
    };
    (@as_ref, $name:ident, $inner:ty) => {
        impl AsRef<$inner> for $name {
            fn as_ref(&self) -> &$inner {
                &self.0
            }
        }
    };
    (@readers, $name:ident, $inner:ty) => {
        impl $name {
            /// The validated value.
            #[must_use]
            pub fn into_inner(self) -> $inner {
                self.0
            }
        }

        impl From<$name> for $inner {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl ::std::ops::Deref for $name {
            type Target = $inner;

            fn deref(&self) -> &$inner {
                &self.0
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::{
        EmbeddedDocument, EmbeddedDocumentKind, TypeValidationError, TypeValidators, UnionMatch,
        UnionVariants, read_trusted, trusted, with_choices,
    };
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::{
        ContentMode, DialectSettings, FormatMode, Registry, Schema, SpecVersion,
    };
    use crate::validator::{
        CompiledSchema, ValidationError, ValidationErrorKind, ValidationResult,
    };
    use serde::Deserializer;
    use serde::de::Error as _;
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};

    /// A [`TypeValidationError`] in a comparable form.
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Invalid(Vec<ValidationError>),
        Json(String),
        Schema(String),
        UnionMismatch { kind: UnionMatch, matched: usize },
        NonFinite(f64),
    }

    fn outcome<T>(result: Result<T, TypeValidationError>) -> Result<T, Outcome> {
        result.map_err(|error| match error {
            TypeValidationError::Invalid(errors) => Outcome::Invalid(errors),
            TypeValidationError::Json(error) => Outcome::Json(error.to_string()),
            TypeValidationError::Schema(error) => Outcome::Schema(error.to_string()),
            TypeValidationError::UnionMismatch { kind, matched } => {
                Outcome::UnionMismatch { kind, matched }
            }
            TypeValidationError::NonFinite(value) => Outcome::NonFinite(value),
        })
    }

    fn kl(pointer: &str) -> JsonPointer {
        JsonPointer::try_from(pointer).expect("valid JSON Pointer")
    }

    fn parsed(uri: &'static str, json: &'static str) -> EmbeddedDocument<'static> {
        EmbeddedDocument {
            uri,
            json,
            kind: EmbeddedDocumentKind::Parsed(None),
        }
    }

    /// The errors the library validator gives for `schema` (a whole document) and `instance`.
    fn library_errors(schema: &str, instance: &Value) -> ValidationResult {
        let schema: Schema = Schema::try_from(schema).expect("valid schema");
        let registry: Registry<'_> = Registry::new(&schema).expect("registry");
        CompiledSchema::new(&registry)
            .expect("compiles")
            .validate(instance)
    }

    const OBJECT_ROOT: &str = r#"{"type":"object","properties":{"a":{"type":"integer","minimum":3}},"required":["a","b"]}"#;

    #[test]
    fn root_type_gives_the_library_validator_errors() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed("json-schema:///", OBJECT_ROOT)];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let instance: Value = json!({"a": 1.5});
        let expected: Result<(), Outcome> =
            library_errors(OBJECT_ROOT, &instance).map_err(Outcome::Invalid);
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &instance));
        assert_eq!(expected, actual);
    }

    #[test]
    fn root_type_accepts_conforming_instance() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed("json-schema:///", OBJECT_ROOT)];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), Outcome> = Ok(());
        let actual: Result<(), Outcome> =
            outcome(validators.validate(0, &json!({"a": 3, "b": null})));
        assert_eq!(expected, actual);
    }

    /// The embedded document codegen emits for two newtype properties with the same `pattern`
    /// (`constrained_newtypes_with_equal_patterns_share_one_static_golden`).
    const EQUAL_PATTERNS_ROOT: &str = r#"{"properties":{"billing":{"pattern":"^[A-Z]{2}$","type":"string"},"shipping":{"pattern":"^[A-Z]{2}$","type":"string"}}}"#;

    #[test]
    fn newtypes_with_equal_patterns_share_one_compiled_regex() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed("json-schema:///", EQUAL_PATTERNS_ROOT)];
        let types: [&[&str]; 2] = [
            &["json-schema:///#/properties/billing"],
            &["json-schema:///#/properties/shipping"],
        ];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Vec<String> = vec!["^[A-Z]{2}$".to_string()];
        let actual: Vec<String> = validators
            .compiled()
            .expect("compiles")
            .0
            .patterns
            .iter()
            .map(|pattern| pattern.source.clone())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn newtypes_with_equal_patterns_both_reject_a_non_matching_value() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed("json-schema:///", EQUAL_PATTERNS_ROOT)];
        let types: [&[&str]; 2] = [
            &["json-schema:///#/properties/billing"],
            &["json-schema:///#/properties/shipping"],
        ];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: [bool; 4] = [true, false, true, false];
        let actual: [bool; 4] = [
            validators.is_valid(0, &json!("US")),
            validators.is_valid(0, &json!("usa")),
            validators.is_valid(1, &json!("GB")),
            validators.is_valid(1, &json!("gb")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn schemas_are_compiled_once_across_validations() {
        let validators: TypeValidators<'static> = defs_validators();
        let first: &CompiledSchema = &validators.compiled().expect("compiles").0;
        let _ = validators.is_valid(0, &json!("Ada"));
        let _ = validators.validate(1, &json!(3));
        let second: &CompiledSchema = &validators.compiled().expect("compiles").0;
        let expected: bool = true;
        let actual: bool = std::ptr::eq(first, second);
        assert_eq!(expected, actual);
    }

    const DEFS_ROOT: &str = r#"{"$defs":{"Name":{"type":"string","minLength":2},"Age":{"type":"integer"}},"type":"object"}"#;

    fn defs_validators() -> TypeValidators<'static> {
        static DOCUMENTS: [EmbeddedDocument<'static>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: DEFS_ROOT,
            kind: EmbeddedDocumentKind::Parsed(None),
        }];
        TypeValidators::new(
            None,
            &DOCUMENTS,
            &[
                &["json-schema:///#/$defs/Name"],
                &["json-schema:///#/$defs/Age"],
            ],
        )
    }

    #[test]
    fn defs_type_reports_errors_relative_to_its_schema() {
        let validators: TypeValidators<'_> = defs_validators();
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![ValidationError::new(
            ValidationErrorKind::TooShort {
                min_length: 2,
                actual_length: 1,
            },
            JsonPointer::root(),
            kl("/minLength"),
        )]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!("a")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn second_defs_type_validates_against_its_own_schema() {
        let validators: TypeValidators<'_> = defs_validators();
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            validators.is_valid(1, &json!(4)),
            validators.is_valid(1, &json!("4")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_in_document_with_real_uri_has_no_absolute_keyword_location_before_a_ref() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "https://example.com/root.json",
            r#"{"$defs":{"Name":{"type":"string"}}}"#,
        )];
        let types: [&[&str]; 1] = [&["https://example.com/root.json#/$defs/Name"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![ValidationError::new(
            ValidationErrorKind::ExpectedString {
                got: "number".to_string(),
            },
            JsonPointer::root(),
            kl("/type"),
        )]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!(1)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn absolute_keyword_location_follows_the_last_ref_under_a_real_uri() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "https://example.com/root.json",
            r##"{"$defs":{"Name":{"$ref":"#/$defs/Text"},"Text":{"type":"string"}}}"##,
        )];
        let types: [&[&str]; 1] = [&["https://example.com/root.json#/$defs/Name"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![
            ValidationError::new(
                ValidationErrorKind::ExpectedString {
                    got: "number".to_string(),
                },
                JsonPointer::root(),
                kl("/$ref/type"),
            )
            .with_absolute_keyword_location("https://example.com/root.json#/$defs/Text/type"),
        ]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!(1)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn conjunction_reports_errors_of_every_schema_in_order() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "json-schema:///",
            r#"{"$defs":{"A":{"required":["a"]},"B":{"required":["b"]}}}"#,
        )];
        let types: [&[&str]; 1] = [&["json-schema:///#/$defs/A", "json-schema:///#/$defs/B"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![
            ValidationError::new(
                ValidationErrorKind::MissingRequired {
                    property: "a".to_string(),
                },
                JsonPointer::root(),
                kl("/required"),
            ),
            ValidationError::new(
                ValidationErrorKind::MissingRequired {
                    property: "b".to_string(),
                },
                JsonPointer::root(),
                kl("/required"),
            ),
        ]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!({})));
        assert_eq!(expected, actual);
    }

    #[test]
    fn conjunction_is_valid_only_when_every_schema_accepts() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "json-schema:///",
            r#"{"$defs":{"A":{"required":["a"]},"B":{"required":["b"]}}}"#,
        )];
        let types: [&[&str]; 1] = [&["json-schema:///#/$defs/A", "json-schema:///#/$defs/B"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: (bool, bool) = (false, true);
        let actual: (bool, bool) = (
            validators.is_valid(0, &json!({"a": 1})),
            validators.is_valid(0, &json!({"a": 1, "b": 2})),
        );
        assert_eq!(expected, actual);
    }

    const RAW_KEYWORDS: &str = r#"{"not":{"type":"string","maxLength":1},"if":true,"then":{"properties":{"x":{"type":"integer"}}}}"#;

    fn raw_keywords_validators() -> TypeValidators<'static> {
        static DOCUMENTS: [EmbeddedDocument<'static>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: RAW_KEYWORDS,
            kind: EmbeddedDocumentKind::Parsed(None),
        }];
        TypeValidators::new(
            None,
            &DOCUMENTS,
            &[
                &["json-schema:///#/not"],
                &["json-schema:///#/then/properties/x"],
            ],
        )
    }

    #[test]
    fn type_under_not_validates_against_the_raw_schema() {
        let validators: TypeValidators<'_> = raw_keywords_validators();
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![ValidationError::new(
            ValidationErrorKind::TooLong {
                max_length: 1,
                actual_length: 2,
            },
            JsonPointer::root(),
            kl("/maxLength"),
        )]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!("ab")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn type_under_then_properties_validates_against_the_raw_schema() {
        let validators: TypeValidators<'_> = raw_keywords_validators();
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![ValidationError::new(
            ValidationErrorKind::ExpectedInteger {
                got: "string".to_string(),
            },
            JsonPointer::root(),
            kl("/type"),
        )]));
        let actual: Result<(), Outcome> = outcome(validators.validate(1, &json!("ab")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn loaded_document_reached_by_ref_validates() {
        let documents: [EmbeddedDocument<'_>; 2] = [
            parsed(
                "https://example.com/root.json",
                r#"{"$ref":"other.json#/$defs/Id"}"#,
            ),
            EmbeddedDocument {
                uri: "https://example.com/other.json",
                json: r#"{"$defs":{"Id":{"type":"integer"}}}"#,
                kind: EmbeddedDocumentKind::Loaded,
            },
        ];
        let types: [&[&str]; 1] = [&["https://example.com/root.json#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(vec![
            ValidationError::new(
                ValidationErrorKind::ExpectedInteger {
                    got: "string".to_string(),
                },
                JsonPointer::root(),
                kl("/$ref/type"),
            )
            .with_absolute_keyword_location("https://example.com/other.json#/$defs/Id/type"),
        ]));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!("x")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn loaded_document_is_a_type_entry() {
        let documents: [EmbeddedDocument<'_>; 2] = [
            parsed("https://example.com/root.json", "{}"),
            EmbeddedDocument {
                uri: "https://example.com/other.json",
                json: r#"{"$defs":{"Id":{"type":"integer"}}}"#,
                kind: EmbeddedDocumentKind::Loaded,
            },
        ];
        let types: [&[&str]; 1] = [&["https://example.com/other.json#/$defs/Id"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            validators.is_valid(0, &json!(1)),
            validators.is_valid(0, &json!("1")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn second_parsed_document_is_a_type_entry() {
        let documents: [EmbeddedDocument<'_>; 2] = [
            parsed("https://example.com/a.json", r#"{"type":"object"}"#),
            parsed("https://example.com/b.json", r#"{"type":"boolean"}"#),
        ];
        let types: [&[&str]; 2] = [
            &["https://example.com/a.json#"],
            &["https://example.com/b.json#"],
        ];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: (bool, bool, bool) = (true, true, false);
        let actual: (bool, bool, bool) = (
            validators.is_valid(0, &json!({})),
            validators.is_valid(1, &json!(true)),
            validators.is_valid(1, &json!({})),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn forced_draft_7_document_ignores_ref_siblings() {
        let dialect: DialectSettings = DialectSettings {
            forced_spec_version: Some(SpecVersion::Draft07),
            ..DialectSettings::default()
        };
        let documents: [EmbeddedDocument<'_>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"definitions":{"S":{"type":"string"}},"$ref":"#/definitions/S","maxLength":1}"##,
            kind: EmbeddedDocumentKind::Parsed(Some(dialect)),
        }];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = true;
        let actual: bool = validators.is_valid(0, &json!("long"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn default_document_evaluates_ref_siblings() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "json-schema:///",
            r##"{"$defs":{"S":{"type":"string"}},"$ref":"#/$defs/S","maxLength":1}"##,
        )];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("long"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_assertion_setting_asserts_format() {
        let dialect: DialectSettings = DialectSettings {
            format_mode: Some(FormatMode::Assertion),
            ..DialectSettings::default()
        };
        let documents: [EmbeddedDocument<'_>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"format":"ipv4"}"#,
            kind: EmbeddedDocumentKind::Parsed(Some(dialect)),
        }];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("not an address"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn format_is_an_annotation_by_default() {
        let documents: [EmbeddedDocument<'_>; 1] =
            [parsed("json-schema:///", r#"{"format":"ipv4"}"#)];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = true;
        let actual: bool = validators.is_valid(0, &json!("not an address"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn content_is_asserted_in_draft_7() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed(
            "json-schema:///",
            r#"{"$schema":"http://json-schema.org/draft-07/schema#","contentEncoding":"base64"}"#,
        )];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("not base64!"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn content_assertion_setting_asserts_content() {
        let dialect: DialectSettings = DialectSettings {
            content_mode: Some(ContentMode::Assertion),
            ..DialectSettings::default()
        };
        let documents: [EmbeddedDocument<'_>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"contentMediaType":"application/json"}"#,
            kind: EmbeddedDocumentKind::Parsed(Some(dialect)),
        }];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("{not json"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn registry_settings_apply_to_loaded_documents() {
        let dialect: DialectSettings = DialectSettings {
            format_mode: Some(FormatMode::Assertion),
            ..DialectSettings::default()
        };
        let documents: [EmbeddedDocument<'_>; 2] = [
            parsed("https://example.com/root.json", r#"{"$ref":"other.json"}"#),
            EmbeddedDocument {
                uri: "https://example.com/other.json",
                json: r#"{"format":"ipv4"}"#,
                kind: EmbeddedDocumentKind::Loaded,
            },
        ];
        let types: [&[&str]; 1] = [&["https://example.com/root.json#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(Some(dialect), &documents, &types);
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("not an address"));
        assert_eq!(expected, actual);
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Point {
        x: i64,
        y: i64,
    }

    const POINT: &str = r#"{"type":"object","properties":{"x":{"type":"integer","minimum":0},"y":{"type":"integer"}},"required":["x","y"]}"#;

    fn point_validators() -> TypeValidators<'static> {
        static DOCUMENTS: [EmbeddedDocument<'static>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: POINT,
            kind: EmbeddedDocumentKind::Parsed(None),
        }];
        TypeValidators::new(None, &DOCUMENTS, &[&["json-schema:///#"]])
    }

    #[test]
    fn deserialize_returns_the_value_of_a_conforming_instance() {
        let validators: TypeValidators<'_> = point_validators();
        let expected: Result<Point, Outcome> = Ok(Point { x: 1, y: -2 });
        let actual: Result<Point, Outcome> =
            outcome(validators.deserialize(0, json!({"x": 1, "y": -2})));
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_rejects_a_nonconforming_instance_with_its_errors() {
        let validators: TypeValidators<'_> = point_validators();
        let instance: Value = json!({"x": -1, "y": -2});
        let expected: Result<Point, Outcome> = Err(Outcome::Invalid(
            library_errors(POINT, &instance).unwrap_err(),
        ));
        let actual: Result<Point, Outcome> = outcome(validators.deserialize(0, instance));
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_reports_a_conforming_instance_serde_rejects() {
        let validators: TypeValidators<'_> = point_validators();
        // `serde_json` words the out-of-range error differently when it keeps numbers as text.
        #[cfg(not(feature = "arbitrary-precision"))]
        let message: &str = "invalid value: integer `10000000000000000000`, expected i64";
        #[cfg(feature = "arbitrary-precision")]
        let message: &str = "invalid number";
        let expected: Result<Point, Outcome> = Err(Outcome::Json(message.to_string()));
        let actual: Result<Point, Outcome> = outcome(
            validators.deserialize(0, json!({"x": 10_000_000_000_000_000_000_u64, "y": 0})),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_serialized_rejects_a_nonconforming_value() {
        let validators: TypeValidators<'_> = point_validators();
        let expected: Result<(), Outcome> = Err(Outcome::Invalid(
            library_errors(POINT, &json!({"x": -1, "y": 0})).unwrap_err(),
        ));
        let actual: Result<(), Outcome> =
            outcome(validators.validate_serialized(0, &Point { x: -1, y: 0 }));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_serialized_accepts_a_conforming_value() {
        let validators: TypeValidators<'_> = point_validators();
        let expected: Result<(), Outcome> = Ok(());
        let actual: Result<(), Outcome> =
            outcome(validators.validate_serialized(0, &Point { x: 1, y: 0 }));
        assert_eq!(expected, actual);
    }

    const UNION: &str = r#"{"$defs":{"Any":{"anyOf":[{"type":"integer"},{"type":"number"},{"type":"string"}]},"One":{"oneOf":[{"type":"integer"},{"type":"number"},{"type":"string"}]}}}"#;

    fn union_validators() -> TypeValidators<'static> {
        static DOCUMENTS: [EmbeddedDocument<'static>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: UNION,
            kind: EmbeddedDocumentKind::Parsed(None),
        }];
        TypeValidators::new(
            None,
            &DOCUMENTS,
            &[
                &["json-schema:///#/$defs/Any"],
                &["json-schema:///#/$defs/One"],
                &["json-schema:///#/$defs/Any/anyOf/0"],
                &["json-schema:///#/$defs/Any/anyOf/1"],
                &["json-schema:///#/$defs/Any/anyOf/2"],
            ],
        )
    }

    /// The errors the library validator gives for the `$defs` entry `name` of [`UNION`].
    fn union_errors(name: &str, instance: &Value) -> Vec<ValidationError> {
        let schema: Value =
            serde_json::from_str::<Value>(UNION).expect("JSON")["$defs"][name].clone();
        library_errors(&schema.to_string(), instance).unwrap_err()
    }

    #[test]
    fn any_of_candidates_are_every_matching_variant_in_order() {
        let validators: TypeValidators<'_> = union_validators();
        let expected: Result<Vec<usize>, Outcome> = Ok(vec![0, 1]);
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::AnyOf, Some(0), &[2, 3, 4], &json!(1)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn any_of_without_match_reports_the_union_schema_errors() {
        let validators: TypeValidators<'_> = union_validators();
        let instance: Value = json!(null);
        let expected: Result<Vec<usize>, Outcome> =
            Err(Outcome::Invalid(union_errors("Any", &instance)));
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::AnyOf, Some(0), &[2, 3, 4], &instance));
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_candidate_is_the_single_matching_variant() {
        let validators: TypeValidators<'_> = union_validators();
        let expected: Result<Vec<usize>, Outcome> = Ok(vec![1]);
        let actual: Result<Vec<usize>, Outcome> = outcome(validators.union_candidates(
            UnionMatch::OneOf,
            Some(1),
            &[2, 3, 4],
            &json!(1.5),
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_with_two_matches_reports_multiple_subschemas_matched() {
        let validators: TypeValidators<'_> = union_validators();
        let instance: Value = json!(1);
        let expected: Result<Vec<usize>, Outcome> =
            Err(Outcome::Invalid(union_errors("One", &instance)));
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::OneOf, Some(1), &[2, 3, 4], &instance));
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_without_match_reports_no_subschema_matched() {
        let validators: TypeValidators<'_> = union_validators();
        let instance: Value = json!([]);
        let expected: Result<Vec<usize>, Outcome> =
            Err(Outcome::Invalid(union_errors("One", &instance)));
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::OneOf, Some(1), &[2, 3, 4], &instance));
        assert_eq!(expected, actual);
    }

    #[test]
    fn one_of_with_two_matches_and_no_union_schema_is_a_union_mismatch() {
        let validators: TypeValidators<'_> = union_validators();
        let expected: Result<Vec<usize>, Outcome> = Err(Outcome::UnionMismatch {
            kind: UnionMatch::OneOf,
            matched: 2,
        });
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::OneOf, None, &[2, 3, 4], &json!(1)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn any_of_without_match_and_no_union_schema_is_a_union_mismatch() {
        let validators: TypeValidators<'_> = union_validators();
        let expected: Result<Vec<usize>, Outcome> = Err(Outcome::UnionMismatch {
            kind: UnionMatch::AnyOf,
            matched: 0,
        });
        let actual: Result<Vec<usize>, Outcome> =
            outcome(validators.union_candidates(UnionMatch::AnyOf, None, &[2, 3, 4], &json!(null)));
        assert_eq!(expected, actual);
    }

    /// A union that reads every variant as its position.
    #[derive(Debug, PartialEq)]
    struct Position(usize);

    impl UnionVariants for Position {
        fn read_variant<'de, D: Deserializer<'de>>(
            position: usize,
            deserializer: D,
        ) -> Result<Self, D::Error> {
            serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(Position(position))
        }
    }

    /// A union whose first variant never reads; the others read a number.
    #[derive(Debug, PartialEq)]
    struct NotFirst(usize);

    impl UnionVariants for NotFirst {
        fn read_variant<'de, D: Deserializer<'de>>(
            position: usize,
            deserializer: D,
        ) -> Result<Self, D::Error> {
            if position == 0 {
                return Err(D::Error::custom("not this one"));
            }
            f64::deserialize(deserializer).map(|_| NotFirst(position))
        }
    }

    /// A union none of whose variants reads.
    #[derive(Debug, PartialEq)]
    struct NeverReads;

    impl UnionVariants for NeverReads {
        fn read_variant<'de, D: Deserializer<'de>>(_: usize, _: D) -> Result<Self, D::Error> {
            Err(D::Error::custom("no"))
        }
    }

    /// A union that reads whether its variant is read trusted.
    #[derive(Debug, PartialEq)]
    struct ReadTrusted(bool);

    impl UnionVariants for ReadTrusted {
        fn read_variant<'de, D: Deserializer<'de>>(
            _: usize,
            deserializer: D,
        ) -> Result<Self, D::Error> {
            serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(ReadTrusted(trusted()))
        }
    }

    /// [`TypeValidators::deserialize_union`] over the `anyOf` (`kind` `AnyOf`, union type 0) or
    /// `oneOf` (type 1) of [`UNION`], reading each variant as its position.
    fn union_position(kind: UnionMatch, instance: &Value) -> Result<usize, String> {
        let union: usize = match kind {
            UnionMatch::AnyOf => 0,
            UnionMatch::OneOf => 1,
        };
        union_validators()
            .deserialize_union::<_, Position>(instance, "Number", kind, Some(union), &[2, 3, 4])
            .map(|Position(position)| position)
            .map_err(|error: serde_json::Error| error.to_string())
    }

    #[test]
    fn deserialize_union_any_of_takes_the_first_matching_variant() {
        let expected: Result<usize, String> = Ok(0);
        let actual: Result<usize, String> = union_position(UnionMatch::AnyOf, &json!(1));
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_any_of_tries_the_next_matching_variant_when_one_does_not_read() {
        let expected: Result<NotFirst, String> = Ok(NotFirst(1));
        let actual: Result<NotFirst, String> = union_validators()
            .deserialize_union(&json!(1), "Number", UnionMatch::AnyOf, Some(0), &[2, 3, 4])
            .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_one_of_with_two_matches_gives_the_union_errors() {
        let instance: Value = json!(1);
        let expected: Result<usize, String> =
            Err(TypeValidationError::Invalid(union_errors("One", &instance)).to_string());
        let actual: Result<usize, String> = union_position(UnionMatch::OneOf, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_one_of_read_trusted_takes_the_first_matching_variant() {
        let expected: Result<usize, String> = Ok(0);
        let actual: Result<usize, String> =
            read_trusted(|| union_position(UnionMatch::OneOf, &json!(1)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_without_match_gives_the_union_errors() {
        let instance: Value = json!(null);
        let expected: Result<usize, String> =
            Err(TypeValidationError::Invalid(union_errors("Any", &instance)).to_string());
        let actual: Result<usize, String> = union_position(UnionMatch::AnyOf, &instance);
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_whose_matching_variants_do_not_read_says_so() {
        let expected: Result<NeverReads, String> = Err(
            "1 does not deserialize into a Number variant whose schema it satisfies".to_string(),
        );
        let actual: Result<NeverReads, String> = union_validators()
            .deserialize_union(&json!(1), "Number", UnionMatch::AnyOf, Some(0), &[2, 3, 4])
            .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_union_reads_the_variant_trusted() {
        let expected: Result<ReadTrusted, String> = Ok(ReadTrusted(true));
        let actual: Result<ReadTrusted, String> = union_validators()
            .deserialize_union(&json!(1), "Number", UnionMatch::OneOf, Some(1), &[3, 4])
            .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }

    /// A `oneOf` of two object shapes (type 0), each branch also a type (1, 2).
    const SHAPES: &str = r#"{"oneOf":[{"type":"object","properties":{"r":{"type":"number"}},"required":["r"]},{"type":"object","properties":{"side":{"type":"number"}},"required":["side"]}]}"#;

    static SHAPE_VALIDATORS: TypeValidators<'static> = TypeValidators::new(
        None,
        &[EmbeddedDocument {
            uri: "json-schema:///",
            json: SHAPES,
            kind: EmbeddedDocumentKind::Parsed(None),
        }],
        &[
            &["json-schema:///#"],
            &["json-schema:///#/oneOf/0"],
            &["json-schema:///#/oneOf/1"],
        ],
    );

    #[derive(Debug, PartialEq, Deserialize)]
    struct Circle {
        r: f64,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Square {
        side: f64,
    }

    #[derive(Debug, PartialEq)]
    enum Shape {
        Circle(Circle),
        Square(Square),
    }

    crate::deserialize_union!(Shape, SHAPE_VALIDATORS, OneOf, Some(0), [1, 2], |value| {
        0 => Circle::deserialize(value).map(Self::Circle),
        _ => Square::deserialize(value).map(Self::Square),
    });

    #[test]
    fn validation_records_the_branch_a_union_chose() {
        let instance: Value = json!({"side": 2});
        let (schema, types) = SHAPE_VALIDATORS.compiled().expect("compiles");
        let (_, choices) = schema.validate_recording_at(&types[0], &instance);
        let expected: Option<usize> = Some(1);
        let actual: Option<usize> = with_choices(choices, || {
            SHAPE_VALIDATORS.recorded_variant(std::ptr::from_ref(&instance) as usize, 0, &[1, 2])
        });
        assert_eq!(expected, actual);
    }

    /// [`SHAPES`] with each branch a `$ref` to a `$defs` entry, which is the variant's type.
    static REF_SHAPE_VALIDATORS: TypeValidators<'static> = TypeValidators::new(
        None,
        &[EmbeddedDocument {
            uri: "json-schema:///",
            json: r##"{"oneOf":[{"$ref":"#/$defs/Circle"},{"$ref":"#/$defs/Square"}],"$defs":{"Circle":{"type":"object","properties":{"r":{"type":"number"}},"required":["r"]},"Square":{"type":"object","properties":{"side":{"type":"number"}},"required":["side"]}}}"##,
            kind: EmbeddedDocumentKind::Parsed(None),
        }],
        &[
            &["json-schema:///#"],
            &["json-schema:///#/$defs/Circle"],
            &["json-schema:///#/$defs/Square"],
        ],
    );

    #[test]
    fn recorded_choice_of_a_ref_branch_names_its_target_variant() {
        let instance: Value = json!({"side": 2});
        let (schema, types) = REF_SHAPE_VALIDATORS.compiled().expect("compiles");
        let (_, choices) = schema.validate_recording_at(&types[0], &instance);
        let expected: Option<usize> = Some(1);
        let actual: Option<usize> = with_choices(choices, || {
            REF_SHAPE_VALIDATORS.recorded_variant(
                std::ptr::from_ref(&instance) as usize,
                0,
                &[1, 2],
            )
        });
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_reads_the_recorded_variant() {
        let expected: Result<Shape, String> = Ok(Shape::Square(Square { side: 2.0 }));
        let actual: Result<Shape, String> = SHAPE_VALIDATORS
            .deserialize(0, json!({"side": 2}))
            .map_err(|error: TypeValidationError| error.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn plain_deserialization_of_a_union_takes_the_buffered_path() {
        let expected: Result<Shape, String> = Ok(Shape::Circle(Circle { r: 1.0 }));
        let actual: Result<Shape, String> = serde_json::from_str::<Shape>(r#"{"r": 1}"#)
            .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }

    /// A parent whose schema allows any string name, and a `Name` newtype that needs five
    /// characters: narrower than its position, which codegen never generates, so trusting the
    /// parent's validation is observable.
    const TRUST: &str = r#"{"type":"object","properties":{"name":{"type":"string"}},"$defs":{"Name":{"type":"string","minLength":5}}}"#;

    static TRUST_VALIDATORS: TypeValidators<'static> = TypeValidators::new(
        None,
        &[EmbeddedDocument {
            uri: "json-schema:///",
            json: TRUST,
            kind: EmbeddedDocumentKind::Parsed(None),
        }],
        &[&["json-schema:///#"], &["json-schema:///#/$defs/Name"]],
    );

    #[derive(Debug, PartialEq, Serialize)]
    #[serde(transparent)]
    struct Name(String);

    crate::constrained_newtype!(string, Name, TRUST_VALIDATORS, 1);

    #[derive(Debug, PartialEq, Deserialize)]
    struct Named {
        name: Name,
    }

    #[test]
    fn deserialize_reads_newtypes_trusted_after_validating_the_instance() {
        let expected: Result<String, Outcome> = Ok("ab".to_string());
        let actual: Result<String, Outcome> = outcome(
            TRUST_VALIDATORS
                .deserialize::<Named>(0, json!({"name": "ab"}))
                .map(|named: Named| named.name.into_inner()),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn plain_deserialize_checks_each_newtype() {
        let name_errors: Vec<ValidationError> =
            library_errors(r#"{"type":"string","minLength":5}"#, &json!("ab")).unwrap_err();
        let expected: Result<Named, String> =
            Err(TypeValidationError::Invalid(name_errors).to_string());
        let actual: Result<Named, String> = serde_json::from_value::<Named>(json!({"name": "ab"}))
            .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn trusted_scope_ends_with_its_deserialization() {
        let _ = TRUST_VALIDATORS.deserialize::<Named>(0, json!({"name": "ab"}));
        let expected: bool = false;
        let actual: bool = trusted();
        assert_eq!(expected, actual);
    }

    #[test]
    fn union_mismatch_display_names_the_union() {
        let expected: String = "more than one variant of the oneOf union matched".to_string();
        let actual: String = TypeValidationError::UnionMismatch {
            kind: UnionMatch::OneOf,
            matched: 2,
        }
        .to_string();
        assert_eq!(expected, actual);
    }

    const BAD_PATTERN: &str = r#"{"pattern":"("}"#;

    fn bad_pattern_validators() -> TypeValidators<'static> {
        static DOCUMENTS: [EmbeddedDocument<'static>; 1] = [EmbeddedDocument {
            uri: "json-schema:///",
            json: BAD_PATTERN,
            kind: EmbeddedDocumentKind::Parsed(None),
        }];
        TypeValidators::new(None, &DOCUMENTS, &[&["json-schema:///#"]])
    }

    /// The compile error of [`BAD_PATTERN`], as displayed.
    fn bad_pattern_error() -> String {
        let schema: Schema = Schema::try_from(BAD_PATTERN).expect("valid schema");
        let registry: Registry<'_> = Registry::new(&schema).expect("registry");
        CompiledSchema::new(&registry)
            .expect_err("invalid pattern")
            .to_string()
    }

    #[test]
    fn schema_that_does_not_compile_fails_check() {
        let validators: TypeValidators<'_> = bad_pattern_validators();
        let expected: Result<(), String> = Err(bad_pattern_error());
        let actual: Result<(), String> = validators.check().map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_that_does_not_compile_makes_validate_a_schema_error() {
        let validators: TypeValidators<'_> = bad_pattern_validators();
        let expected: Result<(), Outcome> = Err(Outcome::Schema(bad_pattern_error()));
        let actual: Result<(), Outcome> = outcome(validators.validate(0, &json!("x")));
        assert_eq!(expected, actual);
    }

    #[test]
    fn schema_that_does_not_compile_makes_is_valid_false() {
        let validators: TypeValidators<'_> = bad_pattern_validators();
        let expected: bool = false;
        let actual: bool = validators.is_valid(0, &json!("x"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn unresolvable_type_uri_fails_check() {
        let documents: [EmbeddedDocument<'_>; 1] = [parsed("json-schema:///", "{}")];
        let types: [&[&str]; 1] = [&["json-schema:///#/$defs/Missing"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), String> = Err(
            "$ref \"json-schema:///#/$defs/Missing\" at root (\"json-schema:///#/$defs/Missing\") does not resolve: no such location"
                .to_string(),
        );
        let actual: Result<(), String> = validators.check().map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn documents_with_invalid_json_fail_check_each() {
        let documents: [EmbeddedDocument<'_>; 3] = [
            parsed("json-schema:///", "{"),
            parsed("https://example.com/ok.json", "{}"),
            EmbeddedDocument {
                uri: "https://example.com/bad.json",
                json: "[",
                kind: EmbeddedDocumentKind::Loaded,
            },
        ];
        let types: [&[&str]; 1] = [&["json-schema:///#"]];
        let validators: TypeValidators<'_> = TypeValidators::new(None, &documents, &types);
        let expected: Result<(), String> = Err(
            "\"json-schema:///\": invalid JSON Schema: EOF while parsing an object at line 1 column 1\n\"https://example.com/bad.json\": invalid JSON Schema: EOF while parsing a list at line 1 column 1"
                .to_string(),
        );
        let actual: Result<(), String> = validators.check().map_err(|e| e.to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn validators_are_usable_from_a_static() {
        static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
            None,
            &[EmbeddedDocument {
                uri: "json-schema:///",
                json: r#"{"type":"string"}"#,
                kind: EmbeddedDocumentKind::Parsed(None),
            }],
            &[&["json-schema:///#"]],
        );
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            VALIDATORS.is_valid(0, &json!("s")),
            VALIDATORS.is_valid(0, &json!(1)),
        );
        assert_eq!(expected, actual);
    }

    const NEWTYPES: &str = r#"{"$defs":{"name":{"type":"string","minLength":2},"even":{"type":"integer","multipleOf":2},"ratio":{"type":"number","maximum":1}}}"#;

    /// Validators over [`NEWTYPES`]: `name` (0), `even` (1), `ratio` (2).
    fn newtype_validators<'a>(documents: &'a [EmbeddedDocument<'static>]) -> TypeValidators<'a> {
        static TYPES: [&[&str]; 3] = [
            &["json-schema:///#/$defs/name"],
            &["json-schema:///#/$defs/even"],
            &["json-schema:///#/$defs/ratio"],
        ];
        TypeValidators::new(None, documents, &TYPES)
    }

    #[test]
    fn validate_newtype_gives_back_a_conforming_string() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<String, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(0, "Ada".to_string()));
        let expected: Result<String, Outcome> = Ok("Ada".to_string());
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_gives_the_library_validator_errors_for_a_string() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<String, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(0, "A".to_string()));
        let expected: Result<String, Outcome> = Err(Outcome::Invalid(
            library_errors(r#"{"type":"string","minLength":2}"#, &json!("A")).expect_err("invalid"),
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_gives_back_a_conforming_integer() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<u64, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(1, u64::MAX - 1));
        let expected: Result<u64, Outcome> = Ok(u64::MAX - 1);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_rejects_a_non_conforming_integer() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<i8, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(1, -3_i8));
        let expected: Result<i8, Outcome> = Err(Outcome::Invalid(
            library_errors(r#"{"type":"integer","multipleOf":2}"#, &json!(-3))
                .expect_err("invalid"),
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_gives_back_a_conforming_f32() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<f32, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(2, 0.1_f32));
        let expected: Result<f32, Outcome> = Ok(0.1_f32);
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_rejects_a_non_conforming_float() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<f64, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(2, 1.5_f64));
        let expected: Result<f64, Outcome> = Err(Outcome::Invalid(
            library_errors(r#"{"type":"number","maximum":1}"#, &json!(1.5)).expect_err("invalid"),
        ));
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_rejects_a_non_finite_float() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let actual: Result<f64, Outcome> =
            outcome(newtype_validators(&documents).validate_newtype(2, f64::NEG_INFINITY));
        let expected: Result<f64, Outcome> = Err(Outcome::NonFinite(f64::NEG_INFINITY));
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_finite_error_names_the_value() {
        let actual: String = TypeValidationError::NonFinite(f64::NAN).to_string();
        let expected: &str = "NaN is not a JSON number";
        assert_eq!(expected, actual);
    }

    #[test]
    fn validate_newtype_as_validates_the_value_read_before_converting() {
        let documents: [EmbeddedDocument<'static>; 1] = [parsed("json-schema:///", NEWTYPES)];
        let validators: TypeValidators<'_> = newtype_validators(&documents);
        let actual: Result<f32, Outcome> =
            outcome(validators.validate_newtype_as(2, 1.000_000_000_1_f64));
        let expected: Result<f32, Outcome> = Err(Outcome::Invalid(
            library_errors(r#"{"type":"number","maximum":1}"#, &json!(1.000_000_000_1))
                .expect_err("invalid"),
        ));
        assert_eq!(expected, actual);
    }
}
