//! Deserializing the integer types of generated code by JSON Schema's integer rule.
//!
//! From draft 6 on, an integer is any JSON number with a zero fractional part, so `1.0` and
//! `1e3` are integers (draft 4 counts only numbers written without a fraction or exponent).
//! serde's integer `Deserialize` accepts only numbers written as integers, so a generated `i64`
//! field would reject a valid `1.0`. Generated code reads its integer types through
//! [`DeserializeInteger`] instead (`#[serde(deserialize_with = "json_schema_rs::integer::deserialize")]`
//! on a field, [`deserialize`] for a union variant): a whole number the type holds is accepted
//! exactly, whatever its spelling; a fraction, or a number the type cannot hold, is an error.
//! Containers of integers (`Option`, `Vec`, `Box`, maps) read each integer the same way.
//!
//! ```
//! #[derive(Debug, PartialEq, serde::Deserialize)]
//! struct Counts {
//!     #[serde(deserialize_with = "json_schema_rs::integer::deserialize")]
//!     total: u8,
//!     #[serde(default, deserialize_with = "json_schema_rs::integer::deserialize")]
//!     per_day: Option<Vec<i64>>,
//! }
//!
//! let counts: Counts = serde_json::from_str(r#"{"total": 3.0, "per_day": [1, 2e0]}"#).unwrap();
//! assert_eq!(counts, Counts { total: 3, per_day: Some(vec![1, 2]) });
//! assert!(serde_json::from_str::<Counts>(r#"{"total": 1.5}"#).is_err());
//! assert!(serde_json::from_str::<Counts>(r#"{"total": 256}"#).is_err());
//! ```
//!
//! A generated type alias (`pub type Root = Vec<i64>;`) is the std type itself, whose
//! `Deserialize` is serde's; read it through [`Integer`] to apply the rule:
//! `serde_json::from_str::<json_schema_rs::integer::Integer<Root>>(text)?.0`.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::BuildHasher;
use std::marker::PhantomData;

/// A type deserialized by JSON Schema's integer rule: an integer type, or a container of them.
pub trait DeserializeInteger<'de>: Sized {
    /// Deserializes `Self`, accepting each integer written with a zero fraction (`1.0`).
    ///
    /// # Errors
    ///
    /// The deserializer's error for anything `Self` does not hold: a fraction, a number out of
    /// the integer type's range, or a value of another kind.
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>;
}

/// `#[serde(deserialize_with = "json_schema_rs::integer::deserialize")]`: reads `T` by JSON
/// Schema's integer rule (see the [module](self)).
///
/// # Errors
///
/// See [`DeserializeInteger::deserialize_integer`].
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeInteger<'de>,
{
    T::deserialize_integer(deserializer)
}

/// [`deserialize`] for an optional field whose value can be `null`: a present value (`null`
/// included) is `Some`, and only an absent key is `None` (with `#[serde(default)]`).
///
/// # Errors
///
/// See [`DeserializeInteger::deserialize_integer`].
pub fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeInteger<'de>,
{
    T::deserialize_integer(deserializer).map(Some)
}

/// `T` read by JSON Schema's integer rule, as a `Deserialize` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Integer<T>(pub T);

impl<'de, T: DeserializeInteger<'de>> Deserialize<'de> for Integer<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize_integer(deserializer).map(Integer)
    }
}

/// Reads an integer type `T`, from an integer or from a float with a zero fraction.
struct IntegerVisitor<T>(PhantomData<T>);

macro_rules! integer {
    ($($ty:ty),*) => {$(
        impl<'de> Visitor<'de> for IntegerVisitor<$ty> {
            type Value = $ty;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!("an integer that fits ", stringify!($ty)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<$ty, E> {
                <$ty>::try_from(value)
                    .map_err(|_| E::invalid_value(de::Unexpected::Signed(value), &self))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<$ty, E> {
                <$ty>::try_from(value)
                    .map_err(|_| E::invalid_value(de::Unexpected::Unsigned(value), &self))
            }

            fn visit_i128<E: de::Error>(self, value: i128) -> Result<$ty, E> {
                <$ty>::try_from(value).map_err(|_| E::invalid_value(de::Unexpected::Other("integer"), &self))
            }

            fn visit_u128<E: de::Error>(self, value: u128) -> Result<$ty, E> {
                <$ty>::try_from(value).map_err(|_| E::invalid_value(de::Unexpected::Other("integer"), &self))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<$ty, E> {
                whole(value)
                    .and_then(|whole: i128| <$ty>::try_from(whole).ok())
                    .ok_or_else(|| E::invalid_value(de::Unexpected::Float(value), &self))
            }

            /// A number as `serde_json` hands it over with `arbitrary_precision`: a one-entry map
            /// holding the number's text, read exactly.
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<$ty, A::Error> {
                let key: Option<String> = map.next_key()?;
                if key.as_deref() != Some(NUMBER_TOKEN) {
                    return Err(de::Error::invalid_type(de::Unexpected::Map, &self));
                }
                let text: String = map.next_value()?;
                whole_text(&text)
                    .and_then(|whole: i128| <$ty>::try_from(whole).ok())
                    .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Other(&text), &self))
            }
        }

        impl<'de> DeserializeInteger<'de> for $ty {
            fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                deserializer.deserialize_any(IntegerVisitor::<$ty>(PhantomData))
            }
        }
    )*};
}

integer!(i8, i16, i32, i64, u8, u16, u32, u64);

/// The key under which `serde_json` with `arbitrary_precision` hands over a number's text.
const NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// The JSON number `text` as an integer when it is a whole number within `i128`, computed
/// exactly from its digits (`1.0`, `1e3`, `12.50e1` are whole; `1.5`, `1e-1` are not).
fn whole_text(text: &str) -> Option<i128> {
    let (negative, rest): (bool, &str) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (mantissa, exponent): (&str, i64) = match rest.find(['e', 'E']) {
        Some(at) => (&rest[..at], rest[at + 1..].parse().ok()?),
        None => (rest, 0),
    };
    let (int_part, fraction): (&str, &str) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits: String = format!("{int_part}{fraction}");
    let mut exponent: i64 = exponent.checked_sub(i64::try_from(fraction.len()).ok()?)?;
    // Trailing zeros move into the exponent, so a whole number has a non-negative one.
    while exponent < 0 && digits.ends_with('0') {
        digits.pop();
        exponent += 1;
    }
    let digits: &str = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some(0);
    }
    if exponent < 0 || !digits.bytes().all(|b: u8| b.is_ascii_digit()) {
        return None;
    }
    let mut value: i128 = digits.parse().ok()?;
    for _ in 0..exponent {
        value = value.checked_mul(10)?;
    }
    Some(if negative { -value } else { value })
}

/// `value` as an integer when it is a whole number within `i128` (every integer type of
/// generated code fits there), exactly.
fn whole(value: f64) -> Option<i128> {
    // 2^127: every whole `f64` below it in magnitude converts to `i128` exactly.
    const LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;
    if value.fract() != 0.0 || !value.is_finite() || value.abs() >= LIMIT {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a whole number below 2^127 in magnitude converts exactly"
    )]
    Some(value as i128)
}

impl<'de, T: DeserializeInteger<'de>> DeserializeInteger<'de> for Option<T> {
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<Integer<T>>::deserialize(deserializer).map(|value| value.map(|v| v.0))
    }
}

impl<'de, T: DeserializeInteger<'de>> DeserializeInteger<'de> for Box<T> {
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize_integer(deserializer).map(Box::new)
    }
}

impl<'de, T: DeserializeInteger<'de>> DeserializeInteger<'de> for Vec<T> {
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct VecVisitor<T>(PhantomData<T>);

        impl<'de, T: DeserializeInteger<'de>> Visitor<'de> for VecVisitor<T> {
            type Value = Vec<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
                let mut out: Vec<T> = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
                while let Some(Integer(item)) = seq.next_element::<Integer<T>>()? {
                    out.push(item);
                }
                Ok(out)
            }
        }

        deserializer.deserialize_seq(VecVisitor(PhantomData))
    }
}

/// A map from string keys, filled entry by entry (a repeated key keeps its last value, as
/// serde's map `Deserialize` does).
trait StringMap<T>: Default {
    fn insert_entry(&mut self, key: String, value: T);
}

impl<T> StringMap<T> for BTreeMap<String, T> {
    fn insert_entry(&mut self, key: String, value: T) {
        self.insert(key, value);
    }
}

impl<T, S: BuildHasher + Default> StringMap<T> for HashMap<String, T, S> {
    fn insert_entry(&mut self, key: String, value: T) {
        self.insert(key, value);
    }
}

struct MapVisitor<M, T>(PhantomData<(M, T)>);

impl<'de, M: StringMap<T>, T: DeserializeInteger<'de>> Visitor<'de> for MapVisitor<M, T> {
    type Value = M;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<M, A::Error> {
        let mut out: M = M::default();
        while let Some((key, Integer(value))) = map.next_entry::<String, Integer<T>>()? {
            out.insert_entry(key, value);
        }
        Ok(out)
    }
}

impl<'de, T: DeserializeInteger<'de>> DeserializeInteger<'de> for BTreeMap<String, T> {
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MapVisitor::<Self, T>(PhantomData))
    }
}

impl<'de, T, S> DeserializeInteger<'de> for HashMap<String, T, S>
where
    T: DeserializeInteger<'de>,
    S: BuildHasher + Default,
{
    fn deserialize_integer<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(MapVisitor::<Self, T>(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::Integer;
    use std::collections::BTreeMap;

    fn read<T: for<'de> super::DeserializeInteger<'de>>(text: &str) -> Result<T, String> {
        serde_json::from_str::<Integer<T>>(text)
            .map(|value: Integer<T>| value.0)
            .map_err(|error: serde_json::Error| error.to_string())
    }

    fn read_value<T: for<'de> super::DeserializeInteger<'de>>(
        value: &serde_json::Value,
    ) -> Result<T, String> {
        super::deserialize::<_, T>(value).map_err(|error: serde_json::Error| error.to_string())
    }

    #[test]
    fn accepts_integer_token() {
        let expected: Result<i64, String> = Ok(-7);
        let actual: Result<i64, String> = read("-7");
        assert_eq!(expected, actual);
    }

    #[test]
    fn accepts_zero_fraction_float() {
        let expected: Result<u8, String> = Ok(1);
        let actual: Result<u8, String> = read("1.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn accepts_exponent_whole_number() {
        let expected: Result<u16, String> = Ok(1000);
        let actual: Result<u16, String> = read("1e3");
        assert_eq!(expected, actual);
    }

    #[test]
    fn accepts_negative_zero_as_zero() {
        let expected: Result<u8, String> = Ok(0);
        let actual: Result<u8, String> = read("-0.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn accepts_zero_fraction_float_from_value() {
        let expected: Result<i32, String> = Ok(2);
        let actual: Result<i32, String> = read_value(&serde_json::json!(2.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_fraction() {
        // With `arbitrary_precision`, `serde_json` hands over the number's text.
        #[cfg(not(feature = "arbitrary-precision"))]
        let unexpected: &str = "floating point `1.5`";
        #[cfg(feature = "arbitrary-precision")]
        let unexpected: &str = "1.5";
        let expected: Result<i64, String> = Err(format!(
            "invalid value: {unexpected}, expected an integer that fits i64 at line 1 column 3"
        ));
        let actual: Result<i64, String> = read("1.5");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_integer_out_of_range() {
        let expected: Result<u8, String> = Err(
            "invalid value: integer `256`, expected an integer that fits u8 at line 1 column 3"
                .to_string(),
        );
        let actual: Result<u8, String> = read("256");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_whole_float_out_of_range() {
        #[cfg(not(feature = "arbitrary-precision"))]
        let unexpected: &str = "floating point `1e+19`";
        #[cfg(feature = "arbitrary-precision")]
        let unexpected: &str = "1e+19";
        let expected: Result<i64, String> = Err(format!(
            "invalid value: {unexpected}, expected an integer that fits i64 at line 1 column 4"
        ));
        let actual: Result<i64, String> = read("1e19");
        assert_eq!(expected, actual);
    }

    #[test]
    fn accepts_whole_float_within_u64() {
        let expected: Result<u64, String> = Ok(10_000_000_000_000_000_000);
        let actual: Result<u64, String> = read("1e19");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_negative_for_unsigned() {
        let expected: Result<u32, String> = Err(
            "invalid value: integer `-1`, expected an integer that fits u32 at line 1 column 2"
                .to_string(),
        );
        let actual: Result<u32, String> = read("-1");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rejects_string() {
        let expected: Result<i64, String> = Err(
            "invalid type: string \"1\", expected an integer that fits i64 at line 1 column 3"
                .to_string(),
        );
        let actual: Result<i64, String> = read("\"1\"");
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_reads_null_as_none() {
        let expected: Result<Option<i64>, String> = Ok(None);
        let actual: Result<Option<i64>, String> = read("null");
        assert_eq!(expected, actual);
    }

    #[test]
    fn option_reads_zero_fraction_float() {
        let expected: Result<Option<i64>, String> = Ok(Some(4));
        let actual: Result<Option<i64>, String> = read("4.0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn vec_reads_each_item() {
        let expected: Result<Vec<Option<i8>>, String> = Ok(vec![Some(1), None, Some(-2)]);
        let actual: Result<Vec<Option<i8>>, String> = read("[1.0, null, -2]");
        assert_eq!(expected, actual);
    }

    #[test]
    fn map_reads_each_value() {
        let expected: Result<BTreeMap<String, Vec<u64>>, String> = Ok(BTreeMap::from([
            ("a".to_string(), vec![1, 2]),
            ("b".to_string(), vec![]),
        ]));
        let actual: Result<BTreeMap<String, Vec<u64>>, String> =
            read(r#"{"a": [1.0, 2], "b": []}"#);
        assert_eq!(expected, actual);
    }

    #[test]
    fn boxed_reads_inner() {
        let expected: Result<Box<i16>, String> = Ok(Box::new(5));
        let actual: Result<Box<i16>, String> = read("5e0");
        assert_eq!(expected, actual);
    }

    #[test]
    fn whole_text_reads_each_spelling() {
        let expected: Vec<Option<i128>> = vec![
            Some(1),
            Some(1000),
            Some(125),
            Some(-40),
            Some(0),
            Some(0),
            None,
            None,
            Some(9_007_199_254_740_993),
            None,
        ];
        let actual: Vec<Option<i128>> = [
            "1.0",
            "1e3",
            "12.50e1",
            "-4E1",
            "-0.0",
            "0e-5",
            "1.5",
            "1e-1",
            "9007199254740993.000",
            "1e39",
        ]
        .into_iter()
        .map(super::whole_text)
        .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn deserialize_some_wraps_null() {
        let expected: Result<Option<Option<i64>>, String> = Ok(Some(None));
        let actual: Result<Option<Option<i64>>, String> =
            super::deserialize_some::<_, Option<i64>>(&serde_json::Value::Null)
                .map_err(|error: serde_json::Error| error.to_string());
        assert_eq!(expected, actual);
    }
}
