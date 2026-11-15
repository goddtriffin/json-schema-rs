//! Building a [`Registry`] with an asynchronous retriever ([`RegistryBuilder::build_async`]).
//!
//! The build itself stays synchronous. It runs in rounds: each round builds the registry with a
//! retriever that serves the documents fetched so far and records every URI it is asked for
//! that it does not have yet. When a round asked for nothing new, its result is the registry
//! (or the build errors). Otherwise every new URI is fetched concurrently and the next round
//! starts. Each URI is fetched at most once, and a fetch that failed is served as that failure,
//! so the last round is exactly the synchronous build with those documents (see design.md,
//! *Asynchronous retrieval*).

use super::{
    Builder, DocumentRoot, JsonSchemaSettings, Registry, RegistryBuilder, RegistryErrors, Retrieve,
    RetrieveError, SchemaRef,
};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};

/// The future an [`AsyncRetrieve`] returns: the document, or why it could not be retrieved.
pub type RetrieveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Value, RetrieveError>> + Send + 'a>>;

/// Fetches schema documents that a registry needs and does not have, asynchronously: the
/// counterpart of [`Retrieve`] for [`RegistryBuilder::build_async`].
///
/// The registry asks only for absolute URIs without a fragment, at most once each, and only
/// while it is being built; documents found in the same round are fetched concurrently.
/// Embedded standard metaschemas and documents given to [`RegistryBuilder::document`] are never
/// retrieved.
///
/// ```
/// use json_schema_rs::{AsyncRetrieve, RetrieveError, RetrieveFuture};
/// use serde_json::{Value, json};
///
/// struct Inventory;
///
/// impl AsyncRetrieve for Inventory {
///     fn retrieve<'a>(&'a self, uri: &'a str) -> RetrieveFuture<'a> {
///         Box::pin(async move {
///             match uri {
///                 "https://example.com/item.json" => Ok(json!({ "type": "string" })),
///                 _ => Err(RetrieveError { reason: format!("no document at \"{uri}\"") }),
///             }
///         })
///     }
/// }
/// ```
pub trait AsyncRetrieve: Send + Sync {
    /// Returns the JSON document identified by `uri`.
    ///
    /// # Errors
    ///
    /// The future resolves to [`RetrieveError`] when the document is not available.
    fn retrieve<'a>(&'a self, uri: &'a str) -> RetrieveFuture<'a>;
}

impl<T: AsyncRetrieve + ?Sized> AsyncRetrieve for &T {
    fn retrieve<'a>(&'a self, uri: &'a str) -> RetrieveFuture<'a> {
        (**self).retrieve(uri)
    }
}

/// What the rounds of one asynchronous build share with each round's retriever.
#[derive(Default)]
struct Fetched {
    /// Every document fetched so far, or why it could not be.
    documents: HashMap<String, Result<Value, RetrieveError>>,
    /// URIs the current round asked for and `documents` does not hold, in the order asked.
    missing: Vec<String>,
}

/// The retriever of one round: serves what was fetched, records what was not.
struct RoundRetriever(Arc<Mutex<Fetched>>);

impl Retrieve for RoundRetriever {
    fn retrieve(&self, uri: &str) -> Result<Value, RetrieveError> {
        let mut fetched = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(document) = fetched.documents.get(uri) {
            return document.clone();
        }
        fetched.missing.push(uri.to_string());
        Err(RetrieveError {
            reason: format!("\"{uri}\" has not been retrieved yet"),
        })
    }
}

impl<'b> RegistryBuilder<'b> {
    /// The asynchronous retriever [`RegistryBuilder::build_async`] fetches referenced documents
    /// with, in place of [`RegistryBuilder::retriever`]. [`RegistryBuilder::build`] does not use
    /// it.
    #[must_use]
    pub fn async_retriever(mut self, retriever: impl AsyncRetrieve + 'b) -> Self {
        self.async_retriever = Some(Box::new(retriever));
        self
    }

    /// Builds the registry for `root`, fetching referenced documents with the
    /// [`RegistryBuilder::async_retriever`] (the documents one round of discovery finds are
    /// fetched concurrently). The future is `Send` when the schemas are. Without an asynchronous
    /// retriever it is [`RegistryBuilder::build`], run when this is called.
    ///
    /// # Errors
    ///
    /// See [`RegistryBuilder::build`]: the result is the one [`RegistryBuilder::build`] gives
    /// with a retriever that returns what the asynchronous one did.
    pub fn build_async<'a>(
        self,
        root: impl Into<SchemaRef<'a>>,
    ) -> impl Future<Output = Result<Registry<'a>, RegistryErrors>> + 'a
    where
        'b: 'a,
    {
        let root: SchemaRef<'a> = root.into();
        let RegistryBuilder {
            base_uri,
            documents,
            parsed_documents,
            retriever,
            async_retriever,
            settings,
            entries,
            vocabularies,
        } = self;
        // Only what is `Send` is moved into the future: the synchronous retriever is not.
        let plan: Plan<'a> = match async_retriever {
            Some(async_retriever) => Plan::Fetch(async_retriever),
            None => Plan::Built(
                RegistryBuilder {
                    base_uri: base_uri.clone(),
                    documents: documents.clone(),
                    parsed_documents: parsed_documents.clone(),
                    retriever,
                    async_retriever: None,
                    settings: settings.clone(),
                    entries: entries.clone(),
                    vocabularies: vocabularies.clone(),
                }
                .build(root),
            ),
        };
        let inputs: RoundInputs<'a> = RoundInputs {
            base_uri,
            documents,
            parsed_documents,
            settings,
            entries,
            vocabularies,
        };
        async move {
            let async_retriever: Box<dyn AsyncRetrieve + 'a> = match plan {
                Plan::Built(built) => return built,
                Plan::Fetch(async_retriever) => async_retriever,
            };
            let fetched: Arc<Mutex<Fetched>> = Arc::new(Mutex::new(Fetched::default()));
            loop {
                let built: Result<Registry<'a>, RegistryErrors> = inputs.build(root, &fetched);
                let missing: Vec<String> = std::mem::take(
                    &mut fetched
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .missing,
                );
                if missing.is_empty() {
                    return built;
                }
                drop(built);
                let results: Vec<Result<Value, RetrieveError>> = join_all(
                    missing
                        .iter()
                        .map(|uri| async_retriever.retrieve(uri))
                        .collect(),
                )
                .await;
                let mut guard = fetched.lock().unwrap_or_else(PoisonError::into_inner);
                for (uri, result) in missing.into_iter().zip(results) {
                    guard.documents.insert(uri, result);
                }
            }
        }
    }
}

/// What [`RegistryBuilder::build_async`] does once polled.
enum Plan<'a> {
    /// Nothing to fetch: the synchronous build's result.
    Built(Result<Registry<'a>, RegistryErrors>),
    /// Build in rounds, fetching with this retriever.
    Fetch(Box<dyn AsyncRetrieve + 'a>),
}

/// Everything of a [`RegistryBuilder`] each round builds from but its retrievers.
struct RoundInputs<'a> {
    base_uri: Option<String>,
    documents: HashMap<String, Value>,
    parsed_documents: Vec<(String, SchemaRef<'a>)>,
    settings: JsonSchemaSettings,
    entries: Vec<String>,
    vocabularies: Vec<String>,
}

impl<'a> RoundInputs<'a> {
    /// One round: the synchronous build with what was fetched so far.
    fn build(
        &self,
        root: SchemaRef<'a>,
        fetched: &Arc<Mutex<Fetched>>,
    ) -> Result<Registry<'a>, RegistryErrors> {
        let round: RegistryBuilder<'a> = RegistryBuilder {
            base_uri: self.base_uri.clone(),
            documents: self.documents.clone(),
            parsed_documents: self.parsed_documents.clone(),
            retriever: Box::new(RoundRetriever(Arc::clone(fetched))),
            async_retriever: None,
            settings: self.settings.clone(),
            entries: self.entries.clone(),
            vocabularies: self.vocabularies.clone(),
        };
        Builder::new(round)?.build(DocumentRoot::Borrowed(root))
    }
}

/// Polls every future until all are ready; their outputs in the same order.
async fn join_all(mut futures: Vec<RetrieveFuture<'_>>) -> Vec<Result<Value, RetrieveError>> {
    let mut outputs: Vec<Option<Result<Value, RetrieveError>>> = vec![None; futures.len()];
    std::future::poll_fn(|cx: &mut Context<'_>| {
        let mut pending: bool = false;
        for (future, output) in futures.iter_mut().zip(outputs.iter_mut()) {
            if output.is_some() {
                continue;
            }
            match future.as_mut().poll(cx) {
                Poll::Ready(result) => *output = Some(result),
                Poll::Pending => pending = true,
            }
        }
        if pending {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
    outputs
        .into_iter()
        .map(|output| output.expect("every future is ready"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AsyncRetrieve, RetrieveFuture};
    use crate::{
        CompiledSchema, JsonSchemaSettings, Registry, RegistryErrors, RetrieveError, Schema,
    };
    use serde_json::{Value, json};
    use std::collections::{BTreeMap, HashMap};
    use std::future::Future;
    use std::sync::Mutex;
    use std::task::{Context, Poll, Waker};

    fn schema(value: &Value) -> Schema {
        Schema::new_from_serde_value(value, &JsonSchemaSettings::default()).expect("schema")
    }

    /// Runs `future` to completion on the current thread.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx: Context<'_> = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
        }
    }

    /// Yields once, so the futures of one round interleave.
    async fn yield_once() {
        let mut yielded: bool = false;
        std::future::poll_fn(|_| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                Poll::Pending
            }
        })
        .await;
    }

    /// Serves `documents` after yielding once, logging when each fetch starts and ends.
    struct Documents {
        documents: BTreeMap<String, Value>,
        log: Mutex<Vec<String>>,
    }

    impl Documents {
        fn new(documents: &[(&str, Value)]) -> Self {
            Documents {
                documents: documents
                    .iter()
                    .map(|(uri, document)| ((*uri).to_string(), document.clone()))
                    .collect(),
                log: Mutex::new(Vec::new()),
            }
        }
    }

    impl AsyncRetrieve for Documents {
        fn retrieve<'a>(&'a self, uri: &'a str) -> RetrieveFuture<'a> {
            Box::pin(async move {
                self.log.lock().expect("log").push(format!("start {uri}"));
                yield_once().await;
                self.log.lock().expect("log").push(format!("end {uri}"));
                self.documents
                    .get(uri)
                    .cloned()
                    .ok_or_else(|| RetrieveError {
                        reason: format!("no document at \"{uri}\""),
                    })
            })
        }
    }

    /// The root refs `a` and `b`; `a` refs `c`, which refs `a` back.
    fn chain() -> (Schema, Vec<(&'static str, Value)>) {
        let root: Schema = schema(&json!({
            "properties": {
                "a": {"$ref": "https://example.com/a.json"},
                "b": {"$ref": "https://example.com/b.json"}
            }
        }));
        let documents: Vec<(&'static str, Value)> = vec![
            (
                "https://example.com/a.json",
                json!({"properties": {"c": {"$ref": "c.json"}}}),
            ),
            ("https://example.com/b.json", json!({"type": "integer"})),
            (
                "https://example.com/c.json",
                json!({"type": "object", "properties": {"a": {"$ref": "a.json"}}}),
            ),
        ];
        (root, documents)
    }

    #[test]
    fn build_async_gives_the_bundle_of_the_synchronous_build() {
        let (root, documents) = chain();
        let sync_documents: HashMap<String, Value> = documents
            .iter()
            .map(|(uri, document)| ((*uri).to_string(), document.clone()))
            .collect();
        let expected: Value = Registry::builder()
            .retriever(sync_documents)
            .build(&root)
            .expect("sync build")
            .bundle()
            .expect("bundle");
        let retriever: Documents = Documents::new(&documents);
        let actual: Value = block_on(
            Registry::builder()
                .async_retriever(&retriever)
                .build_async(&root),
        )
        .expect("async build")
        .bundle()
        .expect("bundle");
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_async_fetches_each_round_concurrently_and_each_uri_once() {
        let (root, documents) = chain();
        let retriever: Documents = Documents::new(&documents);
        block_on(
            Registry::builder()
                .async_retriever(&retriever)
                .build_async(&root),
        )
        .expect("async build");
        let expected: Vec<String> = vec![
            "start https://example.com/a.json".to_string(),
            "start https://example.com/b.json".to_string(),
            "end https://example.com/a.json".to_string(),
            "end https://example.com/b.json".to_string(),
            "start https://example.com/c.json".to_string(),
            "end https://example.com/c.json".to_string(),
        ];
        let actual: Vec<String> = retriever.log.into_inner().expect("log");
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_async_reports_a_failed_retrieval_at_the_ref() {
        let (root, documents) = chain();
        let retriever: Documents = Documents::new(&documents[..2]);
        let expected: String = "$ref \"c.json\" at /properties/c (\"https://example.com/c.json\") \
                                does not resolve: could not retrieve \
                                \"https://example.com/c.json\": no document at \
                                \"https://example.com/c.json\""
            .to_string();
        let errors: RegistryErrors = block_on(
            Registry::builder()
                .async_retriever(&retriever)
                .build_async(&root),
        )
        .expect_err("c.json is missing");
        let actual: String = errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join("\n");
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_async_without_an_async_retriever_uses_the_retriever() {
        let (root, documents) = chain();
        let sync_documents: HashMap<String, Value> = documents
            .iter()
            .map(|(uri, document)| ((*uri).to_string(), document.clone()))
            .collect();
        let registry: Registry<'_> = block_on(
            Registry::builder()
                .retriever(sync_documents)
                .build_async(&root),
        )
        .expect("build");
        let compiled: CompiledSchema = CompiledSchema::new(&registry).expect("compile");
        let expected: bool = false;
        let actual: bool = compiled.is_valid(&json!({"a": {"c": {"a": {"c": 1}}}}));
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_async_follows_a_custom_metaschema_it_retrieves() {
        // The metaschema leaves out the validation vocabulary, so `minimum` asserts nothing.
        let root: Schema = schema(&json!({
            "$schema": "https://example.com/meta.json",
            "minimum": 10
        }));
        let retriever: Documents = Documents::new(&[(
            "https://example.com/meta.json",
            json!({
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "$vocabulary": {"https://json-schema.org/draft/2020-12/vocab/core": true}
            }),
        )]);
        let registry: Registry<'_> = block_on(
            Registry::builder()
                .async_retriever(&retriever)
                .build_async(&root),
        )
        .expect("build");
        let compiled: CompiledSchema = CompiledSchema::new(&registry).expect("compile");
        let expected: bool = true;
        let actual: bool = compiled.is_valid(&json!(1));
        assert_eq!(expected, actual);
    }

    #[test]
    fn build_async_future_is_send() {
        fn send<T: Send>(_: &T) -> bool {
            true
        }
        let (root, documents) = chain();
        let retriever: Documents = Documents::new(&documents);
        let future = Registry::builder()
            .async_retriever(&retriever)
            .build_async(&root);
        let expected: bool = true;
        let actual: bool = send(&future);
        assert_eq!(expected, actual);
    }
}
