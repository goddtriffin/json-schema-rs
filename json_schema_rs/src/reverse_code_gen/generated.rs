//! The document the generator writes: the 2020-12 model plus its target-dialect form.

use super::error::ReverseCodeGenError;
use crate::json_schema::JsonSchema;
use serde::{Serialize, Serializer};
use serde_json::Value;

/// Result of generating a document: the [`GeneratedSchema`], or every problem found while
/// building it.
pub type GeneratedSchemaResult = Result<GeneratedSchema, Vec<ReverseCodeGenError>>;

/// A generated JSON Schema document.
///
/// The generator builds every document as 2020-12, so [`GeneratedSchema::schema`] is the exact
/// 2020-12 model: validating an instance against it in memory gives the verdict the derived
/// types define. For a target dialect other than 2020-12 (and `OpenAPI` 3.1), the document is
/// converted once, when it is generated (a keyword the target cannot express is reported as a
/// [`ReverseCodeGenError`](super::ReverseCodeGenError) then), and kept beside the model: the
/// converted form is what [`GeneratedSchema::to_value`] and the [`Serialize`] impl write. The
/// two differ where the dialects spell a keyword differently (draft-04's boolean
/// `exclusiveMinimum`, `id`, `definitions`, ...), which the typed model cannot hold, so the
/// converted document is JSON, not a second model.
///
/// ```
/// use json_schema_rs::{ReverseCodeGenSettings, SchemaGenerator, TargetDialect};
///
/// let settings = ReverseCodeGenSettings {
///     dialect: Some(TargetDialect::Draft04),
///     ..ReverseCodeGenSettings::default()
/// };
/// let generated = SchemaGenerator::new(settings).into_root_schema_for::<u8>()?;
/// // The model is 2020-12; the JSON is draft-04.
/// assert_eq!(generated.to_value()["$schema"], "http://json-schema.org/draft-04/schema#");
/// # Ok::<(), Vec<json_schema_rs::ReverseCodeGenError>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedSchema {
    /// The document as 2020-12.
    schema: JsonSchema,
    /// The document in the target dialect, when that is not the 2020-12 form of `schema`.
    converted: Option<Value>,
}

impl GeneratedSchema {
    /// A document whose target-dialect form is its 2020-12 model.
    pub(super) fn native(schema: JsonSchema) -> Self {
        Self {
            schema,
            converted: None,
        }
    }

    /// A document with its converted target-dialect form beside the 2020-12 model.
    pub(super) fn converted(schema: JsonSchema, converted: Value) -> Self {
        Self {
            schema,
            converted: Some(converted),
        }
    }

    /// The document as a 2020-12 model, which validates in memory exactly as the derived types
    /// define, whatever the target dialect is.
    #[must_use]
    pub fn schema(&self) -> &JsonSchema {
        &self.schema
    }

    /// [`GeneratedSchema::schema`], by value.
    #[must_use]
    pub fn into_schema(self) -> JsonSchema {
        self.schema
    }

    /// The document in the target dialect, as JSON.
    #[must_use]
    pub fn to_value(&self) -> Value {
        if let Some(value) = &self.converted {
            return value.clone();
        }
        let Ok(value) = serde_json::to_value(&self.schema) else {
            unreachable!("a schema serializes to JSON");
        };
        value
    }
}

impl Serialize for GeneratedSchema {
    /// Writes the document in the target dialect.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match &self.converted {
            Some(value) => value.serialize(serializer),
            None => self.schema.serialize(serializer),
        }
    }
}
