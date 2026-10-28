//! Exact arithmetic on JSON numbers: comparison, the integer rule, `multipleOf`, and
//! mathematical equality of JSON values (with a hash consistent with it).
//!
//! A [`serde_json::Number`] holds a `u64`, an `i64` or an `f64`. Integers compare as integers and
//! `f64`s only with `f64`s; an integer against an `f64` is compared exactly, never by converting
//! one side and accepting the rounding. Numbers beyond 64 bits are `f64`s as `serde_json` parses
//! them (see design.md, *Numbers*).

use crate::json_schema::IntegerSemantics;
use serde_json::{Number, Value};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// A JSON number lowered to the kind `serde_json` stored it as.
#[derive(Debug, Clone, Copy)]
enum Num {
    Int(i128),
    Float(f64),
}

impl From<&Number> for Num {
    fn from(n: &Number) -> Self {
        if let Some(u) = n.as_u64() {
            Num::Int(i128::from(u))
        } else if let Some(i) = n.as_i64() {
            Num::Int(i128::from(i))
        } else {
            // A `Number` that is neither `u64` nor `i64` is an `f64`; `as_f64` is `Some` for it.
            Num::Float(n.as_f64().unwrap_or(f64::NAN))
        }
    }
}

/// Compares two JSON numbers by mathematical value, exactly.
///
/// Integers compare as integers and two `f64`s as `f64`s. An integer `x` against an `f64` `f`
/// first compares `x` rounded to the nearest `f64`: when that differs from `f`, rounding cannot
/// have crossed `f` (the rounded value is the double nearest `x`), so the order stands; when it
/// equals `f`, `f` is a whole number within ±2^64 and the two compare as integers.
#[must_use]
pub(crate) fn cmp_num(a: &Number, b: &Number) -> Ordering {
    match (Num::from(a), Num::from(b)) {
        (Num::Int(x), Num::Int(y)) => x.cmp(&y),
        (Num::Float(x), Num::Float(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Num::Int(x), Num::Float(f)) => cmp_int_float(x, f),
        (Num::Float(f), Num::Int(x)) => cmp_int_float(x, f).reverse(),
    }
}

/// Exact comparison of an integer (within ±2^64) with an `f64`. See [`cmp_num`].
fn cmp_int_float(x: i128, f: f64) -> Ordering {
    #[expect(clippy::cast_precision_loss)]
    let rounded: f64 = x as f64;
    match rounded.partial_cmp(&f) {
        Some(Ordering::Equal) => {
            // `f` equals a rounded 64-bit integer, so it is a whole number within ±2^64 and
            // converts to `i128` exactly.
            #[expect(clippy::cast_possible_truncation)]
            let f_int: i128 = f as i128;
            x.cmp(&f_int)
        }
        Some(order) => order,
        None => Ordering::Equal,
    }
}

/// Whether `n` is an integer under `semantics`: [`IntegerSemantics::Lexical`] accepts numbers
/// `serde_json` read as `u64` / `i64` (written without a fraction or exponent and within 64
/// bits); [`IntegerSemantics::Numeric`] also accepts any `f64` with a zero fractional part
/// (`1.0`, `1e3`, and integers too large for 64 bits).
#[must_use]
pub(crate) fn is_integer(n: &Number, semantics: IntegerSemantics) -> bool {
    match (Num::from(n), semantics) {
        (Num::Int(_), _) => true,
        (Num::Float(_), IntegerSemantics::Lexical) => false,
        (Num::Float(f), IntegerSemantics::Numeric) => f.is_finite() && f.fract() == 0.0,
    }
}

/// Magnitude below which a whole `f64` converts to `i128` with room to add or subtract one.
const I128_SAFE_F64: f64 = 1_329_227_995_784_915_872_903_807_060_280_344_576.0; // 2^120

/// The least integer that satisfies the lower bound `n`: `⌈n⌉` when inclusive (`minimum`),
/// `⌊n⌋ + 1` when exclusive (`exclusiveMinimum`). `None` when `n` is an `f64` beyond ±2^120
/// (no Rust integer type reaches it).
#[must_use]
pub(crate) fn integer_lower_bound(n: &Number, exclusive: bool) -> Option<i128> {
    match Num::from(n) {
        Num::Int(i) => Some(if exclusive { i + 1 } else { i }),
        Num::Float(f) => {
            let bound: f64 = if exclusive { f.floor() + 1.0 } else { f.ceil() };
            float_to_i128(bound)
        }
    }
}

/// The greatest integer that satisfies the upper bound `n`: `⌊n⌋` when inclusive (`maximum`),
/// `⌈n⌉ - 1` when exclusive (`exclusiveMaximum`). `None` when `n` is an `f64` beyond ±2^120.
#[must_use]
pub(crate) fn integer_upper_bound(n: &Number, exclusive: bool) -> Option<i128> {
    match Num::from(n) {
        Num::Int(i) => Some(if exclusive { i - 1 } else { i }),
        Num::Float(f) => {
            let bound: f64 = if exclusive { f.ceil() - 1.0 } else { f.floor() };
            float_to_i128(bound)
        }
    }
}

/// A whole `f64` as `i128`, or `None` when it is not finite or beyond ±2^120.
fn float_to_i128(whole: f64) -> Option<i128> {
    if whole.is_finite() && whole.abs() < I128_SAFE_F64 {
        #[expect(clippy::cast_possible_truncation)]
        let i: i128 = whole as i128;
        Some(i)
    } else {
        None
    }
}

/// Whether `value` is an integer multiple of `divisor` (`value = k · divisor` for an integer
/// `k`), decided exactly with no epsilon.
///
/// Two integers use `%`. Otherwise both numbers are read as the decimal they were written as:
/// an `f64` is the shortest decimal that round-trips to it (`0.1` is `1 · 10^-1`, not the binary
/// value nearest it), which is what the schema author wrote. See [`decimal_is_multiple`].
/// Parsing rejects a `multipleOf` of zero or below (every draft requires it to be strictly
/// positive); for a hand-built model, only `0` is a multiple of `0` and signs do not matter.
#[must_use]
pub(crate) fn is_multiple_of(value: &Number, divisor: &Number) -> bool {
    match (Num::from(value), Num::from(divisor)) {
        (Num::Int(v), Num::Int(d)) => {
            if d == 0 {
                v == 0
            } else {
                v % d == 0
            }
        }
        (v, d) => match (decimal(v), decimal(d)) {
            (Some(v), Some(d)) => decimal_is_multiple(v, d),
            // Unreachable for JSON numbers: only non-finite `f64`s have no decimal form.
            _ => false,
        },
    }
}

/// A non-negative decimal `mantissa · 10^exponent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Decimal {
    mantissa: u128,
    exponent: i64,
}

/// The absolute value of `n` as a decimal: an integer as itself, an `f64` as the shortest decimal
/// that round-trips to it (Rust's `{:e}` formatting). `None` for a non-finite `f64`.
fn decimal(n: Num) -> Option<Decimal> {
    match n {
        Num::Int(i) => Some(Decimal {
            mantissa: i.unsigned_abs(),
            exponent: 0,
        }),
        Num::Float(f) => {
            if !f.is_finite() {
                return None;
            }
            // Shortest round-trip form, e.g. `7.5e-3`, `1e308`, `0e0`.
            let text: String = format!("{:e}", f.abs());
            let (digits, exponent) = text.split_once('e')?;
            let exponent: i64 = exponent.parse().ok()?;
            let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
            let mantissa: u128 = format!("{whole}{fraction}").parse().ok()?;
            let fraction_len: i64 = i64::try_from(fraction.len()).ok()?;
            Some(Decimal {
                mantissa,
                exponent: exponent - fraction_len,
            })
        }
    }
}

/// Splits `n` (non-zero) into `(twos, fives, rest)` with `n = 2^twos · 5^fives · rest` and
/// `rest` coprime to 10.
fn split_twos_and_fives(mut n: u128) -> (i64, i64, u128) {
    let mut twos: i64 = 0;
    let mut fives: i64 = 0;
    while n.is_multiple_of(2) {
        n /= 2;
        twos += 1;
    }
    while n.is_multiple_of(5) {
        n /= 5;
        fives += 1;
    }
    (twos, fives, n)
}

/// Whether `value = vm · 10^ve` is an integer multiple of `divisor = dm · 10^de`.
///
/// `value / divisor = (vm / dm) · 10^shift` with `shift = ve - de`. Split each mantissa into its
/// twos, fives and a rest coprime to 10 (`vm = 2^s · 5^t · u`, `dm = 2^p · 5^q · r`). The
/// quotient is an integer exactly when `r` divides `u` (no power of ten can cancel a factor
/// coprime to 10) and the divisor's twos and fives are covered: `p ≤ s + shift` and
/// `q ≤ t + shift`. Every step stays within `u128` / `i64`, so `1e308` against `0.123456789`
/// needs no big-integer arithmetic.
fn decimal_is_multiple(value: Decimal, divisor: Decimal) -> bool {
    if value.mantissa == 0 {
        return true;
    }
    if divisor.mantissa == 0 {
        return false;
    }
    let shift: i64 = value.exponent - divisor.exponent;
    let (value_twos, value_fives, value_rest) = split_twos_and_fives(value.mantissa);
    let (divisor_twos, divisor_fives, divisor_rest) = split_twos_and_fives(divisor.mantissa);
    value_rest.is_multiple_of(divisor_rest)
        && divisor_twos <= value_twos + shift
        && divisor_fives <= value_fives + shift
}

/// Whether two JSON values are equal as JSON Schema defines it (`const`, `enum`, `uniqueItems`):
/// numbers by mathematical value (`1 == 1.0`), objects regardless of key order, arrays element by
/// element. Iterative, so deep values cannot overflow the stack.
#[must_use]
pub(crate) fn json_equal(a: &Value, b: &Value) -> bool {
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
    Float,
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
/// agree), any other number by its `f64` bits; an object's entries are hashed in key order, so
/// insertion order does not matter. Iterative.
pub(crate) fn json_hash<H: Hasher>(value: &Value, state: &mut H) {
    let mut stack: Vec<HashItem<'_>> = vec![HashItem::Value(value)];
    while let Some(item) = stack.pop() {
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

/// Hashes a number so that numbers equal under [`cmp_num`] hash equally.
fn hash_number<H: Hasher>(n: &Number, state: &mut H) {
    match Num::from(n) {
        Num::Int(i) => {
            HashTag::Integer.hash(state);
            i.hash(state);
        }
        Num::Float(f) => {
            // 2^127: every whole `f64` below it in magnitude converts to `i128` exactly.
            const I128_LIMIT: f64 = 170_141_183_460_469_231_731_687_303_715_884_105_728.0;
            if f.fract() == 0.0 && f.abs() < I128_LIMIT {
                HashTag::Integer.hash(state);
                #[expect(clippy::cast_possible_truncation)]
                let whole: i128 = f as i128;
                whole.hash(state);
            } else {
                HashTag::Float.hash(state);
                f.to_bits().hash(state);
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
