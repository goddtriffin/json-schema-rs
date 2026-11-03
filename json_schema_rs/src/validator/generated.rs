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

use super::compiled::{CompiledEntries, LocatedEntries};
use super::{CompileErrors, CompiledSchema, SchemaError, ValidationError};
use crate::error_list::ErrorList;
use crate::json_schema::{
    DialectSettings, JsonSchemaParseError, JsonSchemaParseErrors, JsonSchemaSettings, Registry,
    RegistryBuilder, Schema,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::fmt;
use std::sync::{Arc, OnceLock};

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

/// Which variants of a union must match (see [`TypeValidators::union_candidates`]).
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
    /// to explain it (see [`TypeValidators::union_candidates`]).
    UnionMismatch {
        /// The union's keyword.
        kind: UnionMatch,
        /// How many variants matched: none, or (for `oneOf`) 2 when two or more did.
        matched: usize,
    },
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
        }
    }
}

impl std::error::Error for TypeValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TypeValidationError::Json(error) => Some(error),
            TypeValidationError::Schema(error) => Some(&**error),
            TypeValidationError::Invalid(_) | TypeValidationError::UnionMismatch { .. } => None,
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

    /// Validates `instance` as type `index`, then deserializes it.
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
        self.validate(index, &instance)?;
        Ok(serde_json::from_value(instance)?)
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
    pub fn union_candidates(
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

#[cfg(test)]
mod tests {
    use super::{
        EmbeddedDocument, EmbeddedDocumentKind, TypeValidationError, TypeValidators, UnionMatch,
    };
    use crate::json_pointer::JsonPointer;
    use crate::json_schema::{DialectSettings, FormatMode, Registry, Schema, SpecVersion};
    use crate::validator::{
        CompiledSchema, ValidationError, ValidationErrorKind, ValidationResult,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::{Value, json};

    /// A [`TypeValidationError`] in a comparable form.
    #[derive(Debug, PartialEq)]
    enum Outcome {
        Invalid(Vec<ValidationError>),
        Json(String),
        Schema(String),
        UnionMismatch { kind: UnionMatch, matched: usize },
    }

    fn outcome<T>(result: Result<T, TypeValidationError>) -> Result<T, Outcome> {
        result.map_err(|error| match error {
            TypeValidationError::Invalid(errors) => Outcome::Invalid(errors),
            TypeValidationError::Json(error) => Outcome::Json(error.to_string()),
            TypeValidationError::Schema(error) => Outcome::Schema(error.to_string()),
            TypeValidationError::UnionMismatch { kind, matched } => {
                Outcome::UnionMismatch { kind, matched }
            }
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
        let expected: Result<Point, Outcome> = Err(Outcome::Json(
            "invalid value: integer `10000000000000000000`, expected i64".to_string(),
        ));
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
}
