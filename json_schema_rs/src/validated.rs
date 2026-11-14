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

#[cfg(test)]
mod tests {
    use super::Validated;
    use crate::{EmbeddedDocument, EmbeddedDocumentKind, TypeValidationError, TypeValidators};
    use serde_json::json;

    static VALIDATORS: TypeValidators<'static> = TypeValidators::new(
        None,
        &[EmbeddedDocument {
            uri: "json-schema:///",
            json: r#"{"properties":{"tags":{"type":"array","minItems":1,"uniqueItems":true}}}"#,
            kind: EmbeddedDocumentKind::Parsed(None),
        }],
        &[&["json-schema:///#"]],
    );

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Pet {
        tags: Vec<String>,
    }

    crate::self_validating!(Pet, VALIDATORS, 0);

    fn errors(result: Result<Validated<Pet>, serde_json::Error>) -> String {
        result.map_or_else(|error| error.to_string(), |_| String::new())
    }

    #[test]
    fn deserializes_a_valid_instance() {
        let expected: Validated<Pet> = Validated(Pet {
            tags: vec!["a".to_string()],
        });
        let actual: Validated<Pet> =
            serde_json::from_str(r#"{"tags": ["a"]}"#).expect("valid instance");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_what_plain_deserialize_accepts_with_every_error() {
        let expected: (bool, String) = (
            true,
            Pet::try_from(json!({"tags": []}))
                .map_or_else(|error| error.to_string(), |_| String::new()),
        );
        let actual: (bool, String) = (
            serde_json::from_str::<Pet>(r#"{"tags": []}"#).is_ok(),
            errors(serde_json::from_value(json!({"tags": []}))),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_a_duplicate_with_the_validator_error() {
        let expected: String = r#"/tags: array has duplicate items (value: "a")"#.to_string();
        let actual: String = errors(serde_json::from_value(json!({"tags": ["a", "a"]})));
        assert_eq!(expected, actual);
    }

    #[test]
    fn try_from_value_validates() {
        let expected: bool = true;
        let actual: bool = matches!(
            Validated::<Pet>::try_from(json!({"tags": []})),
            Err(TypeValidationError::Invalid(_))
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn derefs_to_the_value() {
        let expected: bool = true;
        let actual: bool = Validated(Pet {
            tags: vec!["a".to_string()],
        })
        .validate()
        .is_ok();
        assert_eq!(expected, actual);
    }

    #[test]
    fn serializes_as_the_value() {
        let expected: serde_json::Value = json!({"tags": ["a"]});
        let actual: serde_json::Value = serde_json::to_value(Validated(Pet {
            tags: vec!["a".to_string()],
        }))
        .expect("serializes");
        assert_eq!(expected, actual);
    }
}
