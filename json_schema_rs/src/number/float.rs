//! The default build: a [`Number`] is a `u64`, an `i64` or an `f64`, lowered to
//! `Int(i128) | Float(f64)`. Every answer is exact for those values without big-number
//! arithmetic; an integer beyond 64 bits is the `f64` `serde_json` rounded it to.

use super::HashTag;
use crate::json_schema::IntegerSemantics;
use serde_json::Number;
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

/// The value of `n` as a `u64` when it is a non-negative integer by value: an unsigned integer,
/// or a float with a zero fractional part below 2^64. The count keywords (`minLength`,
/// `maxItems`, ...) read their value with it.
#[must_use]
pub(crate) fn non_negative_integer(n: &Number) -> Option<u64> {
    // 2^64: every float with a zero fraction in [0, 2^64) converts to `u64` exactly.
    const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;
    if let Some(u) = n.as_u64() {
        return Some(u);
    }
    let f: f64 = n.as_f64()?;
    if (0.0..U64_LIMIT).contains(&f) && f.fract() == 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "f is a non-negative whole number below 2^64, so the cast is exact"
        )]
        let u: u64 = f as u64;
        Some(u)
    } else {
        None
    }
}

/// Whether `n` is strictly greater than zero (the valid values of `multipleOf`).
#[must_use]
pub(crate) fn is_positive(n: &Number) -> bool {
    n.as_f64().is_some_and(|f| f > 0.0)
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

/// Hashes a number so that numbers equal under [`cmp_num`] hash equally: a whole number below
/// 2^127 in magnitude as that integer, any other number by its `f64` bits.
pub(super) fn hash_number<H: Hasher>(n: &Number, state: &mut H) {
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

/// The 64-bit limit of the default build: numbers beyond `u64` / `i64` are the `f64`
/// `serde_json` rounded them to. The `arbitrary-precision` build answers each of these exactly
/// (see `exact.rs`).
#[cfg(test)]
mod tests {
    use super::{cmp_num, is_integer, is_multiple_of, non_negative_integer};
    use crate::json_schema::IntegerSemantics;
    use crate::number::{JsonEqKey, json_equal};
    use serde_json::{Number, Value};
    use std::cmp::Ordering;
    use std::collections::HashSet;

    fn num(text: &str) -> Number {
        serde_json::from_str(text).expect("valid JSON number")
    }

    fn value(text: &str) -> Value {
        serde_json::from_str(text).expect("valid JSON")
    }

    #[test]
    fn cmp_num_integers_beyond_u64_round_to_one_f64() {
        let expected: Ordering = Ordering::Equal;
        let actual: Ordering = cmp_num(&num("18446744073709551616"), &num("18446744073709551617"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_integers_beyond_u64_round_to_one_f64() {
        let expected: bool = true;
        let actual: bool = json_equal(
            &value("18446744073709551616"),
            &value("18446744073709551617"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_eq_key_integers_beyond_u64_round_to_one_f64() {
        let values: Vec<Value> = vec![value("18446744073709551616"), value("18446744073709551617")];
        let set: HashSet<JsonEqKey<'_>> = values.iter().map(JsonEqKey).collect();
        let expected: usize = 1;
        let actual: usize = set.len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_integer_beyond_u64_is_its_rounded_f64() {
        // 2^64 + 1 is odd, but its nearest `f64` is 2^64.
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("18446744073709551617"), &num("2"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_lexical_rejects_bignum() {
        let expected: bool = false;
        let actual: bool = is_integer(
            &num("12345678910111213141516171819202122232425262728293031"),
            IntegerSemantics::Lexical,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn number_beyond_f64_range_does_not_parse() {
        let expected: bool = true;
        let actual: bool = serde_json::from_str::<Number>("1e400").is_err();
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_negative_integer_reads_underflowing_exponent_as_zero() {
        // `1e-400` underflows to the `f64` `0.0`.
        let expected: Option<u64> = Some(0);
        let actual: Option<u64> = non_negative_integer(&num("1e-400"));
        assert_eq!(expected, actual);
    }
}
