//! A deserializer over a borrowed [`Value`] that tells a generated union which value it is
//! reading, so the union can look up the branch the validation that just accepted the instance
//! chose for it (#128).
//!
//! `serde_json`'s own `&Value` deserializer hands children out as plain `&Value`s, and a moved
//! (`from_value`) instance has no stable addresses at all. [`ValueRef`] wraps every value it
//! hands out, and answers a union's `deserialize_newtype_struct(`[`UNION_TOKEN`]`)` by noting the
//! value's address before visiting it. Any other deserializer visits the union's newtype as
//! usual, so the union takes its buffered path.

use serde::de::value::BorrowedStrDeserializer;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserializer, forward_to_deserialize_any};
use serde_json::{Error, Value};
use std::cell::Cell;

/// The newtype-struct name a generated union deserializes through.
pub(crate) const UNION_TOKEN: &str = "$json_schema_rs::private::Union";

thread_local! {
    /// The address of the value [`ValueRef`] is handing to a union, until the union takes it.
    static UNION_VALUE: Cell<usize> = const { Cell::new(0) };
}

/// The address of the value the union being deserialized reads, when [`ValueRef`] gave it; taken,
/// so a later union never sees it.
pub(crate) fn take_union_value() -> Option<usize> {
    let address: usize = UNION_VALUE.with(|cell: &Cell<usize>| cell.replace(0));
    (address != 0).then_some(address)
}

/// A deserializer over a borrowed [`Value`] that keeps every value's address (see the module
/// docs).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ValueRef<'de>(pub(crate) &'de Value);

impl<'de> Deserializer<'de> for ValueRef<'de> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Array(items) => visitor.visit_seq(SeqRef(items.iter())),
            Value::Object(map) => visitor.visit_map(MapRef {
                entries: map.iter(),
                value: None,
            }),
            other => other.deserialize_any(visitor),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.0 {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        // Only objects and arrays: a union of scalars is decided by the value's JSON type, and
        // its variant order (integers before numbers) is not its branch order.
        if name == UNION_TOKEN && matches!(self.0, Value::Object(_) | Value::Array(_)) {
            UNION_VALUE.with(|cell: &Cell<usize>| cell.set(std::ptr::from_ref(self.0) as usize));
        }
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.0.deserialize_enum(name, variants, visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit
        unit_struct seq tuple tuple_struct map struct identifier
    }
}

struct SeqRef<'de>(std::slice::Iter<'de, Value>);

impl<'de> SeqAccess<'de> for SeqRef<'de> {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        self.0
            .next()
            .map(|item: &'de Value| seed.deserialize(ValueRef(item)))
            .transpose()
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.0.len())
    }
}

struct MapRef<'de> {
    entries: serde_json::map::Iter<'de>,
    value: Option<&'de Value>,
}

impl<'de> MapAccess<'de> for MapRef<'de> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.entries.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(BorrowedStrDeserializer::new(key))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        match self.value.take() {
            Some(value) => seed.deserialize(ValueRef(value)),
            None => Err(de::Error::custom("value is missing")),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}
