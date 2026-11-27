//! [`Validated`]: a generated type whose `Deserialize` validates the whole schema.

use crate::TypeValidationError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// A value of a self-validating generated type, read through its `TryFrom<serde_json::Value>`:
/// deserializing a `Validated<T>` checks the whole schema, where `T`'s own `Deserialize`
/// checks only what the Rust type and its constrained newtypes hold (not `minItems`,
/// `uniqueItems`, `not`, ...).
///
/// Use it where a framework deserializes for you: `axum::Json<Validated<Pet>>`,
/// `actix_web::web::Json<Validated<Pet>>`, `response.json::<Validated<Pet>>()`. An invalid
/// instance fails to deserialize with every error the validator reports, one per line.
///
/// ```
/// use json_schema_rs::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidators, Validated};
///
/// static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
///     None,
///     &[EmbeddedDocument {
///         uri: "json-schema:///",
///         json: r#"{"properties":{"tags":{"type":"array","minItems":1}}}"#,
///         kind: EmbeddedDocumentKind::Parsed(None),
///     }],
///     &[&["json-schema:///#"]],
/// );
///
/// #[derive(Debug, serde::Serialize, serde::Deserialize)]
/// pub struct Pet {
///     tags: Vec<String>,
/// }
///
/// json_schema_rs::self_validating!(Pet, VALIDATORS, 0);
///
/// let plain: Result<Pet, _> = serde_json::from_str(r#"{"tags": []}"#);
/// let validated: Result<Validated<Pet>, _> = serde_json::from_str(r#"{"tags": []}"#);
/// assert!(plain.is_ok());
/// assert!(validated.is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Validated<T>(T);

impl<T> Validated<T> {
    /// The validated value.
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Validated<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> AsRef<T> for Validated<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: TryFrom<Value, Error = TypeValidationError>> TryFrom<Value> for Validated<T> {
    type Error = TypeValidationError;

    /// Validates `value` against `T`'s schema, then deserializes it (`T::try_from`).
    fn try_from(value: Value) -> Result<Self, TypeValidationError> {
        T::try_from(value).map(Validated)
    }
}

impl<T: Serialize> Serialize for Validated<T> {
    /// As `T`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de, T: TryFrom<Value, Error = TypeValidationError>> Deserialize<'de> for Validated<T> {
    /// Reads a `serde_json::Value`, then `T::try_from` it; the error lists every validation
    /// error.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value: Value = Value::deserialize(deserializer)?;
        T::try_from(value)
            .map(Validated)
            .map_err(serde::de::Error::custom)
    }
}
