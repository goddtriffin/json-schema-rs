//! The runtime of the `Deserialize` impls of generated types.
//!
//! Generated structs derive `json_schema_rs_macro::Deserialize` instead of serde's derive. Its
//! expansion is a map visitor that reads each known key with one call to a shared generic reader
//! here (`__private::read_field`), so the code of a struct grows linearly with its fields and an
//! optimized build stays fast: serde's derive writes every field's read into one `visit_map`,
//! which LLVM optimizes in superlinear time (an 800-field struct took 100 s to build with
//! optimizations; through the shared reader it takes 2 s). The reader is not marked
//! `#[inline(never)]`: LLVM inlines it into small visitors, where that pays (nested small structs
//! read 20% slower without it). Keys are matched once, to a field index, by a non-generic
//! function the derive emits (`__private::KeySeed`), and a wide struct is built from values
//! taken out of their slots one by one, so a struct with thousands of fields still builds in
//! linear time (see `Slot`). The visitor also reads a `#[serde(flatten)]` map of additional properties key by key
//! (`__private::read_additional`), where serde's derive buffers the whole object through its
//! private `Content`.
//!
//! [`deserialize_untagged!`](crate::deserialize_untagged) is the `Deserialize` of a generated
//! `type`-array union: it buffers the value once as a [`serde_json::Value`] and tries each
//! variant from it, where serde's `#[serde(untagged)]` derive buffers through `Content`.
//!
//! serde's `Content` breaks numbers with `serde_json/arbitrary_precision` on (which Cargo
//! feature unification turns on for the whole build when one crate enables it): `serde_json`
//! hands a number to that buffer as a map holding the number's text, so a typed number behind a
//! flattened map or inside an untagged enum fails to deserialize. Neither path here buffers
//! through `Content`; both accept and reject what serde's derive does, with its error messages,
//! except that a struct reads only from a JSON object (serde's derive also reads one from an
//! array, positionally, which no object schema allows).

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

/// Helpers of generated code and its `Deserialize` derive; not public API.
#[doc(hidden)]
pub mod __private {
    use serde::de::{
        self, Deserialize, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, Visitor,
    };
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

    /// An object key: the index of the struct's field it names, else the key itself.
    pub enum Key<'de> {
        /// The struct's field at this index (in declaration order).
        Field(usize),
        /// A key that names no field.
        Unknown(UnknownKey<'de>),
    }

    /// A key that names no field: borrowed from the input when it can be, else owned.
    pub enum UnknownKey<'de> {
        /// Borrowed from the input.
        Borrowed(&'de str),
        /// Owned.
        Owned(String),
    }

    impl UnknownKey<'_> {
        /// The key's text.
        #[must_use]
        pub fn as_str(&self) -> &str {
            match self {
                UnknownKey::Borrowed(key) => key,
                UnknownKey::Owned(key) => key,
            }
        }

        /// The key as an owned string.
        #[must_use]
        pub fn into_string(self) -> String {
            match self {
                UnknownKey::Borrowed(key) => key.to_string(),
                UnknownKey::Owned(key) => key,
            }
        }
    }

    /// Reads an object key (as an identifier, like serde's derive) through the struct's
    /// `field_index`, which gives the index of the field a key names. The key's text is
    /// matched once, whether the input lends the key or hands over an owned one, and a field's
    /// key is never allocated: an owned one is dropped before the field's value is read (as
    /// serde's derive does), so the two are never held at once.
    ///
    /// Generic over the function, not a `fn` pointer: through a pointer, small structs read 4%
    /// slower.
    pub struct KeySeed<F>(pub F);

    impl<F: Fn(&str) -> Option<usize>> KeySeed<F> {
        /// `key` as the field it names, else as the unknown key `unknown` makes of it.
        fn key<'de, K: AsRef<str>>(
            &self,
            key: K,
            unknown: impl FnOnce(K) -> UnknownKey<'de>,
        ) -> Key<'de> {
            match (self.0)(key.as_ref()) {
                Some(index) => Key::Field(index),
                None => Key::Unknown(unknown(key)),
            }
        }
    }

    impl<'de, F: Fn(&str) -> Option<usize>> DeserializeSeed<'de> for KeySeed<F> {
        type Value = Key<'de>;

        fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Key<'de>, D::Error> {
            deserializer.deserialize_identifier(self)
        }
    }

    impl<'de, F: Fn(&str) -> Option<usize>> Visitor<'de> for KeySeed<F> {
        type Value = Key<'de>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("field identifier")
        }

        fn visit_borrowed_str<E: de::Error>(self, key: &'de str) -> Result<Key<'de>, E> {
            Ok(self.key(key, UnknownKey::Borrowed))
        }

        fn visit_str<E: de::Error>(self, key: &str) -> Result<Key<'de>, E> {
            Ok(self.key(key, |key: &str| UnknownKey::Owned(key.to_string())))
        }

        fn visit_string<E: de::Error>(self, key: String) -> Result<Key<'de>, E> {
            Ok(self.key(key, UnknownKey::Owned))
        }

        fn visit_borrowed_bytes<E: de::Error>(self, key: &'de [u8]) -> Result<Key<'de>, E> {
            match std::str::from_utf8(key) {
                Ok(key) => self.visit_borrowed_str(key),
                Err(_) => Err(de::Error::invalid_value(de::Unexpected::Bytes(key), &self)),
            }
        }

        fn visit_bytes<E: de::Error>(self, key: &[u8]) -> Result<Key<'de>, E> {
            match std::str::from_utf8(key) {
                Ok(key) => self.visit_str(key),
                Err(_) => Err(de::Error::invalid_value(de::Unexpected::Bytes(key), &self)),
            }
        }
    }

    /// The value of one field while its struct is read: empty until the field's key is read.
    ///
    /// The derive names each slot's type (`Slot<T>`): left to inference, 2,000 slots cost type
    /// checking 26 s, and an explicit `Option<T>` would spell `Option<Option<T>>` for an
    /// optional field (clippy's `option_option` lint in the user's crate).
    pub struct Slot<T>(pub Option<T>);

    impl<T> Slot<T> {
        /// An empty slot.
        #[must_use]
        pub const fn empty() -> Self {
            Self(None)
        }

        /// The value read, leaving the slot empty: how a wide struct's visitor takes its values,
        /// so its slots are never moved (see the derive's `field_tokens`).
        pub const fn take(&mut self) -> Option<T> {
            self.0.take()
        }
    }

    /// Reads the value of the known key `key` into its field's `slot`; a key read before is a
    /// `duplicate field` error.
    ///
    /// Every field of every struct reads through it (one copy per field type and input format),
    /// so a struct's `visit_map` is a linear list of calls that an optimizing build compiles in
    /// linear time; LLVM inlines it where the visitor is small.
    ///
    /// # Errors
    ///
    /// `duplicate field` when `slot` is already filled, and the value's own error.
    pub fn read_field<'de, A: MapAccess<'de>, T: Deserialize<'de>>(
        map: &mut A,
        slot: &mut Slot<T>,
        key: &'static str,
    ) -> Result<(), A::Error> {
        if slot.0.is_some() {
            return Err(de::Error::duplicate_field(key));
        }
        slot.0 = Some(map.next_value()?);
        Ok(())
    }

    /// Reads the value of a key that is not a field into the map of additional properties, as a
    /// `W` turned into the map's value by `value` (the identity, or the unwrapping of a
    /// `deserialize_with` wrapper); a repeated key keeps its last value.
    ///
    /// # Errors
    ///
    /// The value's error.
    pub fn read_additional<'de, A: MapAccess<'de>, M: AdditionalProperties, W: Deserialize<'de>>(
        map: &mut A,
        additional: &mut M,
        key: UnknownKey<'de>,
        value: fn(W) -> M::Value,
    ) -> Result<(), A::Error> {
        let read: W = map.next_value()?;
        additional.insert(key.into_string(), value(read));
        Ok(())
    }

    /// Skips the value of a key that is neither a field nor kept.
    ///
    /// # Errors
    ///
    /// The input's error.
    #[inline(never)]
    pub fn skip_value<'de, A: MapAccess<'de>>(map: &mut A) -> Result<(), A::Error> {
        map.next_value::<IgnoredAny>()?;
        Ok(())
    }

    /// The `unknown field` error for `key` (serde's message, listing the struct's `keys`).
    #[must_use]
    pub fn unknown_field<E: de::Error>(key: &UnknownKey<'_>, keys: &'static [&'static str]) -> E {
        E::unknown_field(key.as_str(), keys)
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

    /// The `missing field` error for `field`. Out of line and cold: a wide struct has one
    /// absent-field path per required field, and the error's formatting inlined into each made
    /// the visitor several times larger.
    #[cold]
    #[inline(never)]
    #[must_use]
    pub fn missing_field_error<E: de::Error>(field: &'static str) -> E {
        E::missing_field(field)
    }

    /// The deserializer of an absent field.
    struct MissingField<E>(&'static str, PhantomData<E>);

    impl<'de, E: de::Error> Deserializer<'de> for MissingField<E> {
        type Error = E;

        fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, E> {
            Err(missing_field_error(self.0))
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

#[cfg(test)]
mod tests {
    use super::__private::{Key, KeySeed, UnknownKey};
    use serde::de::DeserializeSeed;
    use serde::de::value::{BorrowedStrDeserializer, Error, StringDeserializer};

    /// The index of the field `"name"` names in a struct whose fields are `id` and `name`.
    fn field_index(key: &str) -> Option<usize> {
        match key {
            "id" => Some(0),
            "name" => Some(1),
            _ => None,
        }
    }

    /// The key `KeySeed` reads from an owned string.
    fn owned_key(text: &str) -> Result<Key<'static>, Error> {
        KeySeed(field_index).deserialize(StringDeserializer::<Error>::new(text.to_string()))
    }

    /// The key `KeySeed` reads from a borrowed string.
    fn borrowed_key(text: &str) -> Result<Key<'_>, Error> {
        KeySeed(field_index).deserialize(BorrowedStrDeserializer::<Error>::new(text))
    }

    #[test]
    fn key_seed_reads_a_known_owned_key_as_its_field_index() {
        let actual: Result<Key<'static>, Error> = owned_key("name");
        assert!(matches!(actual, Ok(Key::Field(1))));
    }

    #[test]
    fn key_seed_keeps_an_unknown_owned_key_owned() {
        let actual: Result<Key<'static>, Error> = owned_key("other");
        assert!(matches!(actual, Ok(Key::Unknown(UnknownKey::Owned(ref key))) if key == "other"));
    }

    #[test]
    fn key_seed_reads_a_known_borrowed_key_as_its_field_index() {
        let actual: Result<Key<'_>, Error> = borrowed_key("id");
        assert!(matches!(actual, Ok(Key::Field(0))));
    }

    #[test]
    fn key_seed_keeps_an_unknown_borrowed_key_borrowed() {
        let actual: Result<Key<'_>, Error> = borrowed_key("other");
        assert!(matches!(
            actual,
            Ok(Key::Unknown(UnknownKey::Borrowed("other")))
        ));
    }
}
