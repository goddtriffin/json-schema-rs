//! The `Deserialize` impls of generated types that serde's derive would buffer.
//!
//! serde's derive reads a struct with a `#[serde(flatten)]` field and an `#[serde(untagged)]`
//! enum through its private `Content` buffer. With `serde_json/arbitrary_precision` on (which
//! Cargo feature unification turns on for the whole build when one crate enables it),
//! `serde_json` hands a number to that buffer as a map holding the number's text, so a typed
//! number behind a flattened map or inside an untagged enum fails to deserialize. Generated
//! code implements those two `Deserialize`s with the macros here instead:
//! [`deserialize_struct!`](crate::deserialize_struct) reads each known key straight into its
//! field and every other key into the struct's map of additional properties, and
//! [`deserialize_untagged!`](crate::deserialize_untagged) buffers the value once as a
//! [`serde_json::Value`] and tries each variant from it. Both accept and reject exactly what the
//! derive does, with its error messages, and neither buffers through `Content`.

/// The `Deserialize` of a generated struct whose last field holds the additional properties
/// (the keys its schema does not name), in place of serde's derive with `#[serde(flatten)]`.
///
/// It reads a JSON object (`deserialize_map`, expecting `struct Name`). A known key is read
/// straight into its field, through the field's `deserialize_with` function when it has one; a
/// repeated known key is a `duplicate field` error. Every other key goes into the additional
/// map (a repeated one keeps its last value). A missing key is the field's default with
/// `default`, a `missing field` error with `with`, and otherwise what the field's type makes of
/// absence: `None` for an `Option`, else a `missing field` error (serde's rule).
///
/// `json_schema_rs::deserialize_struct!(Name, additional: Map, { fields })`, where `Map` is a
/// `BTreeMap<String, V>` or `HashMap<String, V>` (`additional: Map = with(path)` reads each `V`
/// through `path`) and each field is one of:
/// - `"key" => field: Type`: read as `Type`.
/// - `"key" => field: Type = with(path)`: read through `path`; absence is an error.
/// - `"key" => field: Type = default(path)`: read through `path`; absence is
///   `Type::default()`.
///
/// ```
/// use std::collections::BTreeMap;
///
/// #[derive(Debug, PartialEq, serde::Serialize)]
/// pub struct Pet {
///     pub name: String,
///     #[serde(rename = "age-years", skip_serializing_if = "Option::is_none")]
///     pub age_years: Option<u8>,
///     #[serde(flatten)]
///     pub additional: BTreeMap<String, serde_json::Value>,
/// }
///
/// json_schema_rs::deserialize_struct!(Pet, additional: BTreeMap<String, serde_json::Value>, {
///     "name" => name: String,
///     "age-years" => age_years: Option<u8>,
/// });
///
/// let pet: Pet = serde_json::from_str(r#"{"name": "Rex", "weight": 1.5}"#).unwrap();
/// let expected: Pet = Pet {
///     name: "Rex".to_string(),
///     age_years: None,
///     additional: BTreeMap::from([("weight".to_string(), serde_json::json!(1.5))]),
/// };
/// assert_eq!(pet, expected);
/// assert!(serde_json::from_str::<Pet>(r#"{"age-years": 3}"#).is_err());
/// ```
#[macro_export]
macro_rules! deserialize_struct {
    (
        $name:ident, $additional:ident : $map:ty $( = with ( $value_with:path ) )?, {
            $( $key:literal => $field:ident : $ty:ty $( = $mode:ident ( $with:path ) )? ),* $(,)?
        }
    ) => {
        // A deprecated type or field is still read (as serde's derive does).
        #[allow(deprecated)]
        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<__D: ::serde::Deserializer<'de>>(deserializer: __D) -> Result<Self, __D::Error> {
                struct __Visitor;

                impl<'de> ::serde::de::Visitor<'de> for __Visitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        formatter.write_str(concat!("struct ", stringify!($name)))
                    }

                    fn visit_map<__A: ::serde::de::MapAccess<'de>>(self, mut map: __A) -> Result<$name, __A::Error> {
                        const KEYS: &[&str] = &[$($key),*];
                        $( let mut $field: Option<$ty> = None; )*
                        let mut additional: $map = Default::default();
                        while let Some(key) = map.next_key_seed($crate::generated_serde::__private::KeySeed(KEYS))? {
                            match key.as_str() {
                                $(
                                    $key => {
                                        if $field.is_some() {
                                            return Err(<__A::Error as ::serde::de::Error>::duplicate_field($key));
                                        }
                                        $field = Some($crate::deserialize_struct!(@read map, $ty $(, $with)?));
                                    }
                                )*
                                _ => {
                                    let value: <$map as $crate::generated_serde::__private::AdditionalProperties>::Value =
                                        $crate::deserialize_struct!(@read map, <$map as $crate::generated_serde::__private::AdditionalProperties>::Value $(, $value_with)?);
                                    $crate::generated_serde::__private::AdditionalProperties::insert(&mut additional, key.into_string(), value);
                                }
                            }
                        }
                        Ok($name {
                            $( $field: $crate::deserialize_struct!(@missing $field, $key $(, $mode)?), )*
                            $additional: additional,
                        })
                    }
                }

                deserializer.deserialize_map(__Visitor)
            }
        }
    };
    (@read $map:ident, $ty:ty) => {
        $map.next_value::<$ty>()?
    };
    (@read $map:ident, $ty:ty, $with:path) => {{
        struct __With($ty);

        impl<'de> ::serde::Deserialize<'de> for __With {
            fn deserialize<__D: ::serde::Deserializer<'de>>(deserializer: __D) -> Result<Self, __D::Error> {
                $with(deserializer).map(__With)
            }
        }

        $map.next_value::<__With>()?.0
    }};
    (@missing $field:ident, $key:literal) => {
        match $field {
            Some(value) => value,
            None => $crate::generated_serde::__private::missing_field::<_, __A::Error>($key)?,
        }
    };
    (@missing $field:ident, $key:literal, with) => {
        match $field {
            Some(value) => value,
            None => return Err(::serde::de::Error::missing_field($key)),
        }
    };
    (@missing $field:ident, $key:literal, default) => {
        $field.unwrap_or_default()
    };
}

/// The `Deserialize` of a generated untagged enum (one newtype variant per JSON shape), in
/// place of serde's derive with `#[serde(untagged)]`: the value is read once into a
/// [`serde_json::Value`], and the first variant (in declaration order) whose type deserializes
/// from it is the result. When none does, the error is serde's: `data did not match any variant
/// of untagged enum Name`.
///
/// `json_schema_rs::deserialize_untagged!(Name { Variant(Type), ... })`; `Variant(Type = with(path))`
/// reads that variant through `path` instead of `Type`'s `Deserialize`:
///
/// ```
/// #[derive(Debug, PartialEq, serde::Serialize)]
/// #[serde(untagged)]
/// pub enum Id {
///     Integer(i64),
///     String(String),
/// }
///
/// json_schema_rs::deserialize_untagged!(Id { Integer(i64), String(String) });
///
/// assert_eq!(serde_json::from_str::<Id>("7").ok(), Some(Id::Integer(7)));
/// assert_eq!(serde_json::from_str::<Id>(r#""a""#).ok(), Some(Id::String("a".to_string())));
/// assert!(serde_json::from_str::<Id>("1.5").is_err());
/// ```
#[macro_export]
macro_rules! deserialize_untagged {
    ($name:ident {}) => {
        // A deprecated type or field is still read (as serde's derive does).
        #[allow(deprecated)]
        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<__D: ::serde::Deserializer<'de>>(deserializer: __D) -> Result<Self, __D::Error> {
                <::serde_json::Value as ::serde::Deserialize>::deserialize(deserializer)?;
                Err(::serde::de::Error::custom(concat!(
                    "data did not match any variant of untagged enum ",
                    stringify!($name)
                )))
            }
        }
    };
    ($name:ident { $( $variant:ident ( $ty:ty $( = with ( $with:path ) )? ) ),+ $(,)? }) => {
        // A deprecated type or field is still read (as serde's derive does).
        #[allow(deprecated)]
        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<__D: ::serde::Deserializer<'de>>(deserializer: __D) -> Result<Self, __D::Error> {
                let value: ::serde_json::Value = <::serde_json::Value as ::serde::Deserialize>::deserialize(deserializer)?;
                $(
                    if let Ok(variant) = $crate::deserialize_untagged!(@read &value, $ty $(, $with)?) {
                        return Ok(Self::$variant(variant));
                    }
                )+
                Err(::serde::de::Error::custom(concat!(
                    "data did not match any variant of untagged enum ",
                    stringify!($name)
                )))
            }
        }
    };
    (@read $value:expr, $ty:ty) => {
        <$ty as ::serde::Deserialize>::deserialize($value)
    };
    (@read $value:expr, $ty:ty, $with:path) => {{
        let read: Result<$ty, ::serde_json::Error> = $with($value);
        read
    }};
}

/// Helpers of the generated-code macros; not public API.
#[doc(hidden)]
pub mod __private {
    use serde::de::{self, Deserialize, DeserializeSeed, Deserializer, Visitor};
    use std::collections::{BTreeMap, HashMap};
    use std::fmt;
    use std::hash::BuildHasher;
    use std::marker::PhantomData;

    /// A map of additional properties: the keys a struct's schema does not name.
    pub trait AdditionalProperties: Default {
        /// The type of each value.
        type Value;

        /// Inserts `value` under `key`, replacing an earlier value.
        fn insert(&mut self, key: String, value: Self::Value);
    }

    impl<V> AdditionalProperties for BTreeMap<String, V> {
        type Value = V;

        fn insert(&mut self, key: String, value: V) {
            Self::insert(self, key, value);
        }
    }

    impl<V, S: BuildHasher + Default> AdditionalProperties for HashMap<String, V, S> {
        type Value = V;

        fn insert(&mut self, key: String, value: V) {
            Self::insert(self, key, value);
        }
    }

    /// An object key: borrowed from the input when it can be, one of the struct's own keys when
    /// it is one, else owned. A known key is never allocated.
    pub enum Key<'de> {
        /// Borrowed from the input.
        Borrowed(&'de str),
        /// One of the struct's keys.
        Known(&'static str),
        /// Neither.
        Owned(String),
    }

    impl Key<'_> {
        /// The key's text.
        #[must_use]
        pub fn as_str(&self) -> &str {
            match self {
                Key::Borrowed(key) | Key::Known(key) => key,
                Key::Owned(key) => key,
            }
        }

        /// The key as an owned string.
        #[must_use]
        pub fn into_string(self) -> String {
            match self {
                Key::Borrowed(key) | Key::Known(key) => key.to_string(),
                Key::Owned(key) => key,
            }
        }
    }

    /// Reads an object key (as an identifier, like serde's derive), knowing the struct's keys.
    pub struct KeySeed(pub &'static [&'static str]);

    impl<'de> DeserializeSeed<'de> for KeySeed {
        type Value = Key<'de>;

        fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Key<'de>, D::Error> {
            deserializer.deserialize_identifier(self)
        }
    }

    impl<'de> Visitor<'de> for KeySeed {
        type Value = Key<'de>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("field identifier")
        }

        fn visit_borrowed_str<E: de::Error>(self, key: &'de str) -> Result<Key<'de>, E> {
            Ok(Key::Borrowed(key))
        }

        fn visit_str<E: de::Error>(self, key: &str) -> Result<Key<'de>, E> {
            Ok(self
                .0
                .iter()
                .copied()
                .find(|known: &&'static str| *known == key)
                .map_or_else(|| Key::Owned(key.to_string()), Key::Known))
        }

        fn visit_string<E: de::Error>(self, key: String) -> Result<Key<'de>, E> {
            Ok(Key::Owned(key))
        }

        fn visit_borrowed_bytes<E: de::Error>(self, key: &'de [u8]) -> Result<Key<'de>, E> {
            std::str::from_utf8(key)
                .map(Key::Borrowed)
                .map_err(|_| de::Error::invalid_value(de::Unexpected::Bytes(key), &self))
        }

        fn visit_bytes<E: de::Error>(self, key: &[u8]) -> Result<Key<'de>, E> {
            match std::str::from_utf8(key) {
                Ok(key) => self.visit_str(key),
                Err(_) => Err(de::Error::invalid_value(de::Unexpected::Bytes(key), &self)),
            }
        }
    }

    /// What the type of a field with no `default` and no `deserialize_with` makes of its key's
    /// absence: `None` for an `Option`, else a `missing field` error (serde's derive rule).
    ///
    /// # Errors
    ///
    /// `missing field` when `V` is not an `Option`.
    pub fn missing_field<'de, V: Deserialize<'de>, E: de::Error>(
        field: &'static str,
    ) -> Result<V, E> {
        V::deserialize(MissingField(field, PhantomData))
    }

    /// The deserializer of an absent field.
    struct MissingField<E>(&'static str, PhantomData<E>);

    impl<'de, E: de::Error> Deserializer<'de> for MissingField<E> {
        type Error = E;

        fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, E> {
            Err(de::Error::missing_field(self.0))
        }

        fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, E> {
            visitor.visit_none()
        }

        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
            bytes byte_buf unit unit_struct newtype_struct seq tuple
            tuple_struct map struct enum identifier ignored_any
        }
    }
}
