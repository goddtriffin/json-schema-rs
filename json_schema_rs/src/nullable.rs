//! [`Nullable`]: an optional property whose value may be `null`, with absent and `null` kept
//! apart.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The value of an optional property that also allows `null`: the key is absent, it is
/// `null`, or it holds a value. JSON Schema tells the three apart (`required` and `type: null`
/// are separate), and so does a round trip: an absent key stays absent, `null` stays `null`.
///
/// Generated structs use it for such properties, with `#[serde(default, skip_serializing_if =
/// "json_schema_rs::Nullable::is_absent")]`: an absent key reads as [`Nullable::Absent`] (the
/// default) and is not written back.
///
/// ```
/// use json_schema_rs::Nullable;
///
/// #[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
/// struct Pet {
///     #[serde(default, skip_serializing_if = "Nullable::is_absent")]
///     owner: Nullable<String>,
/// }
///
/// let absent: Pet = serde_json::from_str("{}").unwrap();
/// let null: Pet = serde_json::from_str(r#"{"owner": null}"#).unwrap();
/// let ada: Pet = serde_json::from_str(r#"{"owner": "Ada"}"#).unwrap();
/// assert_eq!(absent.owner, Nullable::Absent);
/// assert_eq!(null.owner, Nullable::Null);
/// assert_eq!(ada.owner, Nullable::Value("Ada".to_string()));
/// assert_eq!(serde_json::to_string(&absent).unwrap(), "{}");
/// assert_eq!(serde_json::to_string(&null).unwrap(), r#"{"owner":null}"#);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Nullable<T> {
    /// The key is absent.
    #[default]
    Absent,
    /// The key is present with the value `null`.
    Null,
    /// The key holds a value.
    Value(T),
}

impl<T> Nullable<T> {
    /// Whether the key is absent (for `skip_serializing_if`).
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        matches!(self, Nullable::Absent)
    }

    /// Whether the key is present with `null`.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Nullable::Null)
    }

    /// Whether the key holds a value.
    #[must_use]
    pub const fn is_value(&self) -> bool {
        matches!(self, Nullable::Value(_))
    }

    /// The value, if the key holds one (`None` when absent or `null`).
    #[must_use]
    pub const fn value(&self) -> Option<&T> {
        match self {
            Nullable::Value(value) => Some(value),
            Nullable::Absent | Nullable::Null => None,
        }
    }

    /// The value, if the key holds one, by value.
    #[must_use]
    pub fn into_value(self) -> Option<T> {
        match self {
            Nullable::Value(value) => Some(value),
            Nullable::Absent | Nullable::Null => None,
        }
    }

    /// `Nullable<&T>`, borrowing the value.
    #[must_use]
    pub const fn as_ref(&self) -> Nullable<&T> {
        match self {
            Nullable::Absent => Nullable::Absent,
            Nullable::Null => Nullable::Null,
            Nullable::Value(value) => Nullable::Value(value),
        }
    }

    /// `Nullable<&T::Target>`, dereferencing the value (`Nullable<&str>` from
    /// `Nullable<String>`).
    #[must_use]
    pub fn as_deref(&self) -> Nullable<&T::Target>
    where
        T: std::ops::Deref,
    {
        match self {
            Nullable::Absent => Nullable::Absent,
            Nullable::Null => Nullable::Null,
            Nullable::Value(value) => Nullable::Value(value),
        }
    }

    /// Maps the value, keeping absent and `null`.
    #[must_use]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Nullable<U> {
        match self {
            Nullable::Absent => Nullable::Absent,
            Nullable::Null => Nullable::Null,
            Nullable::Value(value) => Nullable::Value(f(value)),
        }
    }

    /// The value as an `Option` (`null` is `None`), or `absent` when the key is absent: what a
    /// schema `default` accessor returns (`absent` is `Some(default)`).
    #[must_use]
    pub fn or_absent(self, absent: Option<T>) -> Option<T> {
        match self {
            Nullable::Absent => absent,
            Nullable::Null => None,
            Nullable::Value(value) => Some(value),
        }
    }
}

impl<T> From<T> for Nullable<T> {
    fn from(value: T) -> Nullable<T> {
        Nullable::Value(value)
    }
}

impl<T> From<Option<T>> for Nullable<T> {
    /// `None` is `null` (a present key); use [`Nullable::Absent`] for an absent one.
    fn from(value: Option<T>) -> Nullable<T> {
        value.map_or(Nullable::Null, Nullable::Value)
    }
}

impl<T: Serialize> Serialize for Nullable<T> {
    /// A value as itself, `null` (and absent, when not skipped) as `null`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Nullable::Value(value) => value.serialize(serializer),
            Nullable::Absent | Nullable::Null => serializer.serialize_none(),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    /// `null` is [`Nullable::Null`], anything else a value; an absent key never reaches this
    /// (it takes the field's `#[serde(default)]`, [`Nullable::Absent`]).
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(Nullable::from)
    }
}

#[cfg(test)]
mod tests {
    use super::Nullable;
    use serde_json::{Value, json};

    #[test]
    fn deserializes_null_and_a_value() {
        let expected: (Nullable<u8>, Nullable<u8>) = (Nullable::Null, Nullable::Value(3));
        let actual: (Nullable<u8>, Nullable<u8>) = (
            serde_json::from_value(Value::Null).expect("null"),
            serde_json::from_value(json!(3)).expect("value"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn serializes_null_and_a_value() {
        let expected: (Value, Value) = (Value::Null, json!("a"));
        let actual: (Value, Value) = (
            serde_json::to_value(Nullable::<String>::Null).expect("null"),
            serde_json::to_value(Nullable::Value("a".to_string())).expect("value"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn or_absent_keeps_null_apart_from_absent() {
        let expected: [Option<u8>; 3] = [Some(7), None, Some(1)];
        let actual: [Option<u8>; 3] = [
            Nullable::Absent.or_absent(Some(7)),
            Nullable::Null.or_absent(Some(7)),
            Nullable::Value(1).or_absent(Some(7)),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_none_is_null_and_some_is_a_value() {
        let expected: (Nullable<u8>, Nullable<u8>) = (Nullable::Null, Nullable::Value(2));
        let actual: (Nullable<u8>, Nullable<u8>) = (None.into(), Some(2).into());
        assert_eq!(expected, actual);
    }
}
