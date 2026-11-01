//! What codegen reads: the input schemas, where each was retrieved from, and how to retrieve
//! the documents their `$ref`s name.

use crate::json_schema::registry::parse_uri;
use crate::json_schema::{
    JsonSchemaSettings, OfflineRetriever, RegistryBuilder, Retrieve, RetrieveError, Schema,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;

/// The schemas to generate code for, with what their `$ref`s may reach.
///
/// Each schema gets one output buffer. A schema added with [`CodeGenInput::schema_at`] has a
/// retrieval URI: its relative `$ref`s resolve against it (unless its root `$id` says otherwise),
/// and the other input schemas reach it by that URI. Every input schema is also reachable by its
/// root `$id`. A `$ref` to any other document goes to the [`Retrieve`]r (default:
/// [`OfflineRetriever`], so it is an error naming the URI), and the types of a retrieved document
/// are generated in the buffer of the schema that reaches it (or the shared buffer, when several
/// do). See design.md, *$ref*.
///
/// A slice of schemas converts into an input with no URIs and no retrieval:
///
/// ```
/// use json_schema_rs::{CodeGenSettings, Schema, generate_rust};
///
/// let schema: Schema = Schema::try_from(r#"{"type": "string"}"#).unwrap();
/// let output = generate_rust(&[schema], &CodeGenSettings::default()).unwrap();
/// assert_eq!(1, output.per_schema.len());
/// ```
pub struct CodeGenInput<'a> {
    schemas: Vec<(&'a Schema, Option<String>)>,
    retriever: Box<dyn Retrieve + 'a>,
    settings: JsonSchemaSettings,
}

impl fmt::Debug for CodeGenInput<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodeGenInput")
            .field("schemas", &self.schemas)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl Default for CodeGenInput<'_> {
    fn default() -> Self {
        CodeGenInput {
            schemas: Vec::new(),
            retriever: Box::new(OfflineRetriever),
            settings: JsonSchemaSettings::default(),
        }
    }
}

impl<'a> CodeGenInput<'a> {
    /// An input with no schemas, no retrieval and default settings for retrieved documents.
    #[must_use]
    pub fn new() -> Self {
        CodeGenInput::default()
    }

    /// Adds a schema with no retrieval URI (inline text, stdin): its relative `$ref`s resolve
    /// against its root `$id`, if it has one.
    #[must_use]
    pub fn schema(mut self, schema: &'a Schema) -> Self {
        self.schemas.push((schema, None));
        self
    }

    /// Adds a schema retrieved from `uri` (an absolute URI, such as the `file:` URI of its path
    /// from [`file_uri`](crate::file_uri)).
    #[must_use]
    pub fn schema_at(mut self, uri: impl Into<String>, schema: &'a Schema) -> Self {
        self.schemas.push((schema, Some(uri.into())));
        self
    }

    /// The retriever for documents a `$ref` names that are neither input schemas nor embedded
    /// standard metaschemas, such as a [`FileRetriever`](crate::FileRetriever). Each document is
    /// retrieved at most once per generation.
    #[must_use]
    pub fn retriever(mut self, retriever: impl Retrieve + 'a) -> Self {
        self.retriever = Box::new(retriever);
        self
    }

    /// The settings retrieved documents are parsed with (input schemas are already parsed).
    #[must_use]
    pub fn settings(mut self, settings: JsonSchemaSettings) -> Self {
        self.settings = settings;
        self
    }

    /// The input schemas, in order.
    pub fn schemas(&self) -> impl ExactSizeIterator<Item = &'a Schema> + '_ {
        self.schemas.iter().map(|(schema, _)| *schema)
    }

    /// A registry builder for input schema `index`: its retrieval URI as the base, every other
    /// input schema as a parsed document (under its URI and, through the registry, its root
    /// `$id`; one without a URI only under an absolute root `$id`), and `retriever`.
    pub(crate) fn registry_builder<'r>(
        &'r self,
        index: usize,
        retriever: &'r CachingRetriever<'r>,
    ) -> RegistryBuilder<'r>
    where
        'a: 'r,
    {
        let mut builder: RegistryBuilder<'r> = RegistryBuilder::default()
            .retriever(retriever)
            .settings(self.settings.clone());
        if let Some(uri) = &self.schemas[index].1 {
            builder = builder.base_uri(uri.clone());
        }
        for (other, (schema, uri)) in self.schemas.iter().enumerate() {
            if other == index {
                continue;
            }
            // An invalid URI is reported by that schema's own registry (its base URI), so the
            // error carries its index; the other registries leave it out.
            let key: Option<&str> = uri
                .as_deref()
                .or(schema.as_object().and_then(|object| object.id.as_deref()))
                .filter(|key| parse_uri(key).is_ok());
            if let Some(key) = key {
                builder = builder.parsed_document(key, *schema);
            }
        }
        builder
    }

    /// The input's retriever behind a cache shared by every schema's registry.
    pub(crate) fn caching_retriever(&self) -> CachingRetriever<'_> {
        CachingRetriever {
            inner: self.retriever.as_ref(),
            cache: RefCell::new(HashMap::new()),
        }
    }
}

/// Retrieves each URI once for all of a generation's registries.
pub(crate) struct CachingRetriever<'r> {
    inner: &'r dyn Retrieve,
    cache: RefCell<HashMap<String, Result<Value, RetrieveError>>>,
}

impl Retrieve for CachingRetriever<'_> {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        if let Some(cached) = self.cache.borrow().get(uri) {
            return cached.clone();
        }
        let retrieved: Result<Value, RetrieveError> = self.inner.retrieve(uri);
        self.cache
            .borrow_mut()
            .insert(uri.to_string(), retrieved.clone());
        retrieved
    }
}

impl<'a> From<&'a [Schema]> for CodeGenInput<'a> {
    fn from(schemas: &'a [Schema]) -> Self {
        schemas
            .iter()
            .fold(CodeGenInput::new(), CodeGenInput::schema)
    }
}

impl<'a> From<&'a Vec<Schema>> for CodeGenInput<'a> {
    fn from(schemas: &'a Vec<Schema>) -> Self {
        CodeGenInput::from(schemas.as_slice())
    }
}

impl<'a, const N: usize> From<&'a [Schema; N]> for CodeGenInput<'a> {
    fn from(schemas: &'a [Schema; N]) -> Self {
        CodeGenInput::from(schemas.as_slice())
    }
}
