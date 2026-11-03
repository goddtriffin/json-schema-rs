//! Exact arithmetic on JSON numbers: comparison, the integer rule, `multipleOf`, and
//! mathematical equality of JSON values (with a hash consistent with it).
//!
//! There is no epsilon anywhere; what "exact" covers depends on the build:
//!
//! - **Default build** (`float.rs`). A [`serde_json::Number`] holds a `u64`, an `i64` or an
//!   `f64`. Integers compare as integers and `f64`s only with `f64`s; an integer against an
//!   `f64` is compared exactly, never by converting one side and accepting the rounding.
//!   Numbers beyond 64 bits are the `f64`s `serde_json` parsed them as, so `2^64` and
//!   `2^64 + 1` are equal and `1e400` does not parse.
//! - **`arbitrary-precision` build** (`exact.rs`). `serde_json` keeps every number as the text
//!   it was written as, and each one is read as the exact decimal it denotes, of any size: an
//!   integer within `i128` as an integer, anything else as a normalised sign, digit string and
//!   power-of-ten exponent (see that module for the exponent range).
//!
//! Both builds share [`json_equal`], [`json_hash`] and [`JsonEqKey`] (see design.md,
//! *Numbers*).

#[cfg(feature = "arbitrary-precision")]
mod exact;
#[cfg(not(feature = "arbitrary-precision"))]
mod float;

#[cfg(feature = "arbitrary-precision")]
use exact::hash_number;
#[cfg(feature = "arbitrary-precision")]
pub(crate) use exact::{
    cmp_num, integer_lower_bound, integer_upper_bound, is_integer, is_multiple_of, is_positive,
    non_negative_integer,
};
#[cfg(not(feature = "arbitrary-precision"))]
use float::hash_number;
#[cfg(not(feature = "arbitrary-precision"))]
pub(crate) use float::{
    cmp_num, integer_lower_bound, integer_upper_bound, is_integer, is_multiple_of, is_positive,
    non_negative_integer,
};

use serde_json::Value;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// Whether two JSON values are equal as JSON Schema defines it (`const`, `enum`, `uniqueItems`):
/// numbers by mathematical value (`1 == 1.0`), objects regardless of key order, arrays element by
/// element. Iterative, so deep values cannot overflow the stack.
#[must_use]
pub fn json_equal(a: &Value, b: &Value) -> bool {
    // Scalars (the common `const` / `enum` case) compare directly, without the work stack.
    match (a, b) {
        (Value::Null, Value::Null) => return true,
        (Value::Bool(p), Value::Bool(q)) => return p == q,
        (Value::Number(p), Value::Number(q)) => return cmp_num(p, q) == Ordering::Equal,
        (Value::String(p), Value::String(q)) => return p == q,
        (Value::Array(_), Value::Array(_)) | (Value::Object(_), Value::Object(_)) => {}
        _ => return false,
    }
    let mut stack: Vec<(&Value, &Value)> = vec![(a, b)];
    while let Some((x, y)) = stack.pop() {
        match (x, y) {
            (Value::Null, Value::Null) => {}
            (Value::Bool(p), Value::Bool(q)) => {
                if p != q {
                    return false;
                }
            }
            (Value::Number(p), Value::Number(q)) => {
                if cmp_num(p, q) != Ordering::Equal {
                    return false;
                }
            }
            (Value::String(p), Value::String(q)) => {
                if p != q {
                    return false;
                }
            }
            (Value::Array(p), Value::Array(q)) => {
                if p.len() != q.len() {
                    return false;
                }
                stack.extend(p.iter().zip(q.iter()));
            }
            (Value::Object(p), Value::Object(q)) => {
                if p.len() != q.len() {
                    return false;
                }
                for (key, p_value) in p {
                    let Some(q_value) = q.get(key) else {
                        return false;
                    };
                    stack.push((p_value, q_value));
                }
            }
            _ => return false,
        }
    }
    true
}

/// Tags that keep the hashes of different JSON kinds apart.
#[derive(Hash)]
enum HashTag {
    Null,
    Bool,
    Integer,
    #[cfg(not(feature = "arbitrary-precision"))]
    Float,
    #[cfg(feature = "arbitrary-precision")]
    Decimal,
    String,
    Array,
    Object,
    Key,
}

/// One step of [`json_hash`]'s walk: a value to hash, or an object key.
enum HashItem<'a> {
    Value(&'a Value),
    Key(&'a str),
}

/// Feeds `value` into `state` consistently with [`json_equal`]: equal values hash equally.
///
/// A number that is a whole number within `i128` hashes as that integer (so `1` and `1.0`
/// agree), any other number by its `f64` bits (default build) or its normalised decimal
/// (`arbitrary-precision`); an object's entries are hashed in key order, so
/// insertion order does not matter. Iterative.
pub(crate) fn json_hash<H: Hasher>(value: &Value, state: &mut H) {
    // The root is hashed without the stack, so a scalar never allocates.
    let mut stack: Vec<HashItem<'_>> = Vec::new();
    let mut next: Option<HashItem<'_>> = Some(HashItem::Value(value));
    while let Some(item) = next.take().or_else(|| stack.pop()) {
        let value: &Value = match item {
            HashItem::Key(key) => {
                HashTag::Key.hash(state);
                key.hash(state);
                continue;
            }
            HashItem::Value(value) => value,
        };
        match value {
            Value::Null => HashTag::Null.hash(state),
            Value::Bool(b) => {
                HashTag::Bool.hash(state);
                b.hash(state);
            }
            Value::Number(n) => hash_number(n, state),
            Value::String(s) => {
                HashTag::String.hash(state);
                s.hash(state);
            }
            Value::Array(items) => {
                HashTag::Array.hash(state);
                items.len().hash(state);
                stack.extend(items.iter().rev().map(HashItem::Value));
            }
            Value::Object(map) => {
                HashTag::Object.hash(state);
                map.len().hash(state);
                let mut entries: Vec<(&String, &Value)> = map.iter().collect();
                entries.sort_unstable_by_key(|(key, _)| *key);
                for (key, child) in entries.into_iter().rev() {
                    stack.push(HashItem::Value(child));
                    stack.push(HashItem::Key(key));
                }
            }
        }
    }
}

/// A JSON value compared with [`json_equal`] and hashed with [`json_hash`], for hash sets keyed
/// by mathematical JSON equality (`uniqueItems`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct JsonEqKey<'a>(pub(crate) &'a Value);

impl PartialEq for JsonEqKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        json_equal(self.0, other.0)
    }
}

impl Eq for JsonEqKey<'_> {}

impl Hash for JsonEqKey<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        json_hash(self.0, state);
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonEqKey, cmp_num, is_integer, is_multiple_of, json_equal, json_hash};
    use crate::json_schema::IntegerSemantics;
    use serde_json::{Number, Value, json};
    use std::cmp::Ordering;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    fn num(text: &str) -> Number {
        serde_json::from_str(text).expect("valid JSON number")
    }

    fn hash_of(value: &Value) -> u64 {
        let mut hasher: DefaultHasher = DefaultHasher::new();
        json_hash(value, &mut hasher);
        hasher.finish()
    }

    #[test]
    fn cmp_num_integers_beyond_f64_precision() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num("9007199254740992"), &num("9007199254740993"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_u64_above_i64_max_with_negative() {
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(&num("18446744073709551615"), &num("-1"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_integer_and_equal_float() {
        let expected: Ordering = Ordering::Equal;
        let actual: Ordering = cmp_num(&num("1"), &num("1.0"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_integer_below_float() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num("-3"), &num("-2.0001"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_float_above_integer() {
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(&num("300.5"), &num("300"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_integer_that_rounds_onto_float_is_greater() {
        // 2^53 + 1 rounds to the double 2^53, but is greater than it.
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(&num("9007199254740993"), &num("9007199254740992.0"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_float_beyond_u64_above_u64_max() {
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(&num("1e20"), &num("18446744073709551615"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_floats() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num("0.6"), &num("1.1"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_equal_large_floats() {
        let expected: Ordering = Ordering::Equal;
        let actual: Ordering = cmp_num(&num("9.727837981879871e+26"), &num("9.727837981879871e26"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_accepts_zero_fraction_float() {
        let expected: bool = true;
        let actual: bool = is_integer(&num("1.0"), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_lexical_rejects_zero_fraction_float() {
        let expected: bool = false;
        let actual: bool = is_integer(&num("1.0"), IntegerSemantics::Lexical);
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_lexical_accepts_u64_above_i64_max() {
        let expected: bool = true;
        let actual: bool = is_integer(&num("18446744073709551615"), IntegerSemantics::Lexical);
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_accepts_bignum() {
        let expected: bool = true;
        let actual: bool = is_integer(
            &num("12345678910111213141516171819202122232425262728293031"),
            IntegerSemantics::Numeric,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_rejects_fraction() {
        let expected: bool = false;
        let actual: bool = is_integer(&num("1.5"), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_accepts_negative_i64() {
        let expected: bool = true;
        let actual: bool = is_integer(&num("-7"), IntegerSemantics::Lexical);
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_integers() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("10"), &num("2")),
            is_multiple_of(&num("7"), &num("2")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_negative_integers() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("-9"), &num("3"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_divisor_only_for_zero() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("0"), &num("0")),
            is_multiple_of(&num("1"), &num("0")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_float_divisor_only_for_zero() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("0.0"), &num("0.0")),
            is_multiple_of(&num("1.5"), &num("0.0")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_is_multiple_of_anything() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("0"), &num("1.5"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_float_by_float() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("4.5"), &num("1.5")),
            is_multiple_of(&num("35"), &num("1.5")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_small_decimal_is_exact() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("0.0075"), &num("0.0001")),
            is_multiple_of(&num("0.00751"), &num("0.0001")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_tenth_is_decimal_not_binary() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("0.3"), &num("0.1"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_huge_float_by_irregular_decimal_is_false() {
        let expected: bool = false;
        let actual: bool = is_multiple_of(&num("1e308"), &num("0.123456789"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_huge_float_by_half_is_true() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("1e308"), &num("0.5"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_integer_by_tiny_decimal() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("12391239123"), &num("1e-8"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_zero_fraction_float_by_integer() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("10.0"), &num("2")),
            is_multiple_of(&num("10.0"), &num("4")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_integer_by_larger_decimal_exponent() {
        // 2500 = 25 · 100 is a multiple of 2.5e2 = 250? 2500 / 250 = 10.
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("2500"), &num("2.5e2")),
            is_multiple_of(&num("2600"), &num("2.5e2")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_integer_and_float() {
        let expected: bool = true;
        let actual: bool = json_equal(&json!(1), &json!(1.0));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_nested_numbers() {
        let expected: bool = true;
        let actual: bool = json_equal(
            &json!({"a": [0, {"b": -2}]}),
            &json!({"a": [0.0, {"b": -2.0}]}),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_distinguishes_kinds() {
        let expected: (bool, bool, bool) = (false, false, false);
        let actual: (bool, bool, bool) = (
            json_equal(&json!(0), &json!(false)),
            json_equal(&json!(1), &json!(true)),
            json_equal(&json!(null), &json!(0)),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_different_lengths() {
        let expected: (bool, bool) = (false, false);
        let actual: (bool, bool) = (
            json_equal(&json!([1]), &json!([1, 1])),
            json_equal(&json!({"a": 1}), &json!({"a": 1, "b": 1})),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_scalars_of_one_kind() {
        let expected: [bool; 8] = [true, true, false, true, false, true, false, false];
        let actual: [bool; 8] = [
            json_equal(&json!(null), &json!(null)),
            json_equal(&json!(true), &json!(true)),
            json_equal(&json!(true), &json!(false)),
            json_equal(&json!(-3), &json!(-3.0)),
            json_equal(&json!(2), &json!(2.5)),
            json_equal(&json!("é"), &json!("é")),
            json_equal(&json!("a"), &json!("b")),
            json_equal(&json!([]), &json!({})),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_scalar_and_one_element_array_differ() {
        let expected: bool = false;
        let actual: bool = hash_of(&json!("a")) == hash_of(&json!(["a"]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_different_keys() {
        let expected: bool = false;
        let actual: bool = json_equal(&json!({"a": 1}), &json!({"b": 1}));
        assert_eq!(expected, actual);
    }

    /// `depth` nested single-element arrays around `1`, built without recursion.
    fn deep_array(depth: usize) -> Value {
        let mut deep: Value = json!(1);
        for _ in 0..depth {
            deep = Value::Array(vec![deep]);
        }
        deep
    }

    /// Drops a value without recursion (`Value`'s own `Drop` recurses).
    fn drop_iteratively(value: Value) {
        let mut stack: Vec<Value> = vec![value];
        while let Some(mut v) = stack.pop() {
            match &mut v {
                Value::Array(items) => stack.append(items),
                Value::Object(map) => stack.extend(std::mem::take(map).into_iter().map(|(_, v)| v)),
                _ => {}
            }
        }
    }

    #[test]
    fn json_equal_deep_nesting_does_not_overflow() {
        let a: Value = deep_array(100_000);
        let b: Value = deep_array(100_000);
        let expected: bool = true;
        let actual: bool = json_equal(&a, &b);
        drop_iteratively(a);
        drop_iteratively(b);
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_integer_and_float_agree() {
        let expected: u64 = hash_of(&json!([1, {"a": -2}]));
        let actual: u64 = hash_of(&json!([1.0, {"a": -2.0}]));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_large_whole_float_and_u64_agree() {
        let expected: u64 = hash_of(&json!(9_007_199_254_740_992_u64));
        let actual: u64 = hash_of(&json!(9_007_199_254_740_992.0_f64));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_eq_key_finds_duplicates_by_value() {
        let values: Vec<Value> = vec![json!({"a": 1, "b": 2}), json!({"b": 2.0, "a": 1})];
        let set: std::collections::HashSet<JsonEqKey<'_>> = values.iter().map(JsonEqKey).collect();
        let expected: usize = 1;
        let actual: usize = set.len();
        assert_eq!(expected, actual);
    }
}
