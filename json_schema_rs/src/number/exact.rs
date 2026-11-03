//! The `arbitrary-precision` build: `serde_json` keeps every [`Number`] as the text it was
//! written as, and every answer here is exact on that decimal.
//!
//! A number is lowered once per use to `Int(i128)` when its text is an integer within `i128`
//! (the common case, one `parse`), and otherwise to a normalised [`Decimal`]: a sign, the
//! significant digits (no leading or trailing zeros) borrowed from the text where they are
//! contiguous, and a power-of-ten exponent. Zero has no digits and no sign, so `-0`, `0.0` and
//! `0e5` are the one zero. Comparison, equality and hashing work on the digits directly (no
//! big-integer arithmetic); only a `multipleOf` divisor beyond `u64` uses [`BigUint`].
//!
//! **Exponent range.** The normalised exponent is held as an `i64` within ±2^62; an exponent
//! written beyond that (`1e99999999999999999999`) is read as ±2^62. Every number whose exponent
//! fits compares, hashes and divides exactly; two numbers whose exponents are both beyond the
//! range and differ only there compare equal. Nothing panics and no power of ten is ever built:
//! `1e100000000` against `1e-100000000` is decided from the exponents alone.

use super::HashTag;
use crate::json_schema::IntegerSemantics;
use num_bigint::BigUint;
use serde_json::Number;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

/// The largest normalised exponent magnitude held; beyond it an exponent saturates.
const EXPONENT_LIMIT: i128 = 1 << 62;

/// Room for the digits of any `i128` magnitude (at most 39).
const INT_DIGITS: usize = 40;

/// Integer bounds at or beyond 2^120 in magnitude are `None` (no Rust integer type reaches
/// them), as in the default build.
const INTEGER_BOUND_LIMIT: u128 = 1 << 120;

/// A JSON number lowered for arithmetic: an integer within `i128`, or any other decimal.
#[derive(Debug, Clone)]
enum Num<'a> {
    Int(i128),
    Decimal(Decimal<'a>),
}

impl<'a> Num<'a> {
    fn of(n: &'a Number) -> Self {
        let text: &'a str = n.as_str();
        if let Ok(i) = text.parse::<i128>() {
            return Num::Int(i);
        }
        Num::Decimal(Decimal::parse(text).unwrap_or_else(|| {
            // Only `Number::from_string_unchecked` can hold text that is not a JSON number.
            log::error!("serde_json::Number {text:?} is not a JSON number; it is read as 0");
            Decimal::ZERO
        }))
    }

    /// The number as a [`Decimal`]; an integer is written into `buffer`.
    fn into_decimal<'b>(self, buffer: &'b mut [u8; INT_DIGITS]) -> Decimal<'b>
    where
        'a: 'b,
    {
        match self {
            Num::Int(i) => Decimal::of_int(i, buffer),
            Num::Decimal(d) => d,
        }
    }
}

/// A decimal `±digits · 10^exponent`, normalised: `digits` are ASCII digits with no leading or
/// trailing zero, and zero is the empty digit string with `negative == false` and exponent `0`.
/// Two equal numbers have equal decimals.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Decimal<'a> {
    negative: bool,
    digits: Cow<'a, [u8]>,
    exponent: i64,
}

impl<'a> Decimal<'a> {
    const ZERO: Self = Decimal {
        negative: false,
        digits: Cow::Borrowed(&[]),
        exponent: 0,
    };

    /// Parses JSON number text (`-?digits(.digits)?([eE][+-]?digits)?`). `None` when `text`
    /// is not of that form.
    fn parse(text: &'a str) -> Option<Self> {
        let (negative, unsigned): (bool, &str) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (significand, written_exponent): (&str, i128) = match unsigned.find(['e', 'E']) {
            Some(at) => (&unsigned[..at], parse_exponent(&unsigned[at + 1..])?),
            None => (unsigned, 0),
        };
        let (whole, fraction): (&str, &str) = match significand.split_once('.') {
            Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
            Some(_) => return None,
            None => (significand, ""),
        };
        if whole.is_empty() || !all_digits(whole) || !all_digits(fraction) {
            return None;
        }
        let whole: &[u8] = whole.as_bytes();
        let fraction: &[u8] = fraction.as_bytes();
        let digit_at = |i: usize| -> u8 {
            if i < whole.len() {
                whole[i]
            } else {
                fraction[i - whole.len()]
            }
        };
        let total: usize = whole.len() + fraction.len();
        let Some(first) = (0..total).find(|i| digit_at(*i) != b'0') else {
            return Some(Self::ZERO);
        };
        // A non-zero digit exists, so the last one does too.
        let end: usize = (first..total)
            .rev()
            .find(|i| digit_at(*i) != b'0')
            .map_or(total, |last| last + 1);
        let digits: Cow<'a, [u8]> = if end <= whole.len() {
            Cow::Borrowed(&whole[first..end])
        } else if first >= whole.len() {
            Cow::Borrowed(&fraction[first - whole.len()..end - whole.len()])
        } else {
            let mut owned: Vec<u8> = Vec::with_capacity(end - first);
            owned.extend_from_slice(&whole[first..]);
            owned.extend_from_slice(&fraction[..end - whole.len()]);
            Cow::Owned(owned)
        };
        let exponent: i128 = written_exponent - len(fraction.len()) + len(total - end);
        Some(Decimal {
            negative,
            digits,
            exponent: saturate_exponent(exponent),
        })
    }

    /// `i` as a decimal, its digits written into `buffer`.
    fn of_int(i: i128, buffer: &mut [u8; INT_DIGITS]) -> Decimal<'_> {
        let mut magnitude: u128 = i.unsigned_abs();
        if magnitude == 0 {
            return Decimal::ZERO;
        }
        let mut exponent: i64 = 0;
        while magnitude.is_multiple_of(10) {
            magnitude /= 10;
            exponent += 1;
        }
        let mut start: usize = INT_DIGITS;
        while magnitude > 0 {
            start -= 1;
            let digit: u8 = (magnitude % 10) as u8;
            buffer[start] = b'0' + digit;
            magnitude /= 10;
        }
        Decimal {
            negative: i < 0,
            digits: Cow::Borrowed(&buffer[start..]),
            exponent,
        }
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// The power of ten just above the leading digit: `|self|` lies in
    /// `[10^(adjusted - 1), 10^adjusted)`.
    fn adjusted(&self) -> i128 {
        len(self.digits.len()) + i128::from(self.exponent)
    }

    /// `-1`, `0` or `1`.
    fn sign(&self) -> i8 {
        if self.is_zero() {
            0
        } else if self.negative {
            -1
        } else {
            1
        }
    }

    /// Orders by sign, then magnitude: the adjusted exponent first, then the digits, which for
    /// one adjusted exponent order lexicographically (`0.12 < 0.123`).
    fn cmp_value(&self, other: &Decimal<'_>) -> Ordering {
        match self.sign().cmp(&other.sign()) {
            Ordering::Equal => {}
            order => return order,
        }
        let magnitude: Ordering = self
            .adjusted()
            .cmp(&other.adjusted())
            .then_with(|| self.digits.as_ref().cmp(other.digits.as_ref()));
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    /// The value as an `i128` when it is a whole number within `i128`.
    fn whole_i128(&self) -> Option<i128> {
        let exponent: u32 = u32::try_from(self.exponent).ok()?;
        let magnitude: u128 =
            digits_u128(&self.digits)?.checked_mul(10_u128.checked_pow(exponent)?)?;
        signed(self.negative, magnitude)
    }

    /// `(⌊self⌋, ⌈self⌉)` when both fit `i128`.
    fn floor_and_ceil(&self) -> Option<(i128, i128)> {
        if self.exponent >= 0 {
            let whole: i128 = self.whole_i128()?;
            return Some((whole, whole));
        }
        // Normalised digits end in a non-zero digit, so a negative exponent leaves a non-zero
        // fraction: the whole part is the digits before the point.
        let whole_len: usize = usize::try_from(self.adjusted()).unwrap_or(0);
        let whole: i128 = i128::try_from(digits_u128(&self.digits[..whole_len])?).ok()?;
        if self.negative {
            Some((-whole - 1, -whole))
        } else {
            Some((whole, whole.checked_add(1)?))
        }
    }
}

/// Whether `text` is all ASCII digits (true for the empty string).
fn all_digits(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit())
}

/// A length as `i128` (every `usize` fits).
fn len(n: usize) -> i128 {
    i128::try_from(n).unwrap_or(i128::MAX)
}

/// The exponent text after `e` (`[+-]?digits`), its magnitude capped at 2^64 so later
/// arithmetic stays within `i128`. `None` when it is not of that form.
fn parse_exponent(text: &str) -> Option<i128> {
    const CAP: i128 = 1 << 64;
    let (negative, digits): (bool, &str) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() || !all_digits(digits) {
        return None;
    }
    let magnitude: i128 = digits
        .bytes()
        .fold(0, |acc: i128, b| (acc * 10 + i128::from(b - b'0')).min(CAP));
    Some(if negative { -magnitude } else { magnitude })
}

/// A normalised exponent clamped to ±2^62 (see the module docs).
fn saturate_exponent(exponent: i128) -> i64 {
    let clamped: i128 = exponent.clamp(-EXPONENT_LIMIT, EXPONENT_LIMIT);
    i64::try_from(clamped).unwrap_or(0)
}

/// ASCII digits as a `u128`, or `None` when they overflow it.
fn digits_u128(digits: &[u8]) -> Option<u128> {
    digits.iter().try_fold(0_u128, |acc: u128, b| {
        acc.checked_mul(10)?.checked_add(u128::from(b - b'0'))
    })
}

/// `±magnitude` as an `i128`, or `None` when it does not fit.
fn signed(negative: bool, magnitude: u128) -> Option<i128> {
    if negative {
        0_i128.checked_sub_unsigned(magnitude)
    } else {
        i128::try_from(magnitude).ok()
    }
}

/// Compares two JSON numbers by mathematical value, exactly: two integers within `i128` as
/// integers, anything else as decimals (sign, then the adjusted exponent, then the digits).
#[must_use]
pub(crate) fn cmp_num(a: &Number, b: &Number) -> Ordering {
    match (Num::of(a), Num::of(b)) {
        (Num::Int(x), Num::Int(y)) => x.cmp(&y),
        (x, y) => {
            let mut x_buffer: [u8; INT_DIGITS] = [0; INT_DIGITS];
            let mut y_buffer: [u8; INT_DIGITS] = [0; INT_DIGITS];
            x.into_decimal(&mut x_buffer)
                .cmp_value(&y.into_decimal(&mut y_buffer))
        }
    }
}

/// Whether `n` is an integer under `semantics`: [`IntegerSemantics::Lexical`] accepts a number
/// written without a fraction or exponent, of any size; [`IntegerSemantics::Numeric`] accepts
/// any number whose value has no fractional part (`1.0`, `1e3`, `1.5e1`).
#[must_use]
pub(crate) fn is_integer(n: &Number, semantics: IntegerSemantics) -> bool {
    match (Num::of(n), semantics) {
        (Num::Int(_), _) => true,
        (Num::Decimal(_), IntegerSemantics::Lexical) => !n.as_str().contains(['.', 'e', 'E']),
        // Zero has exponent 0; any other normalised decimal is whole iff its exponent is not
        // negative.
        (Num::Decimal(d), IntegerSemantics::Numeric) => d.exponent >= 0,
    }
}

/// A bound kept only below 2^120 in magnitude.
fn within_bound_limit(bound: i128) -> Option<i128> {
    (bound.unsigned_abs() < INTEGER_BOUND_LIMIT).then_some(bound)
}

/// The least integer that satisfies the lower bound `n`: `⌈n⌉` when inclusive (`minimum`),
/// `⌊n⌋ + 1` when exclusive (`exclusiveMinimum`), exactly. `None` when it is 2^120 or more in
/// magnitude (no Rust integer type reaches it).
#[must_use]
pub(crate) fn integer_lower_bound(n: &Number, exclusive: bool) -> Option<i128> {
    let bound: i128 = match Num::of(n) {
        Num::Int(i) if exclusive => i.checked_add(1)?,
        Num::Int(i) => i,
        Num::Decimal(d) => {
            let (floor, ceil): (i128, i128) = d.floor_and_ceil()?;
            if exclusive {
                floor.checked_add(1)?
            } else {
                ceil
            }
        }
    };
    within_bound_limit(bound)
}

/// The greatest integer that satisfies the upper bound `n`: `⌊n⌋` when inclusive (`maximum`),
/// `⌈n⌉ - 1` when exclusive (`exclusiveMaximum`), exactly. `None` when it is 2^120 or more in
/// magnitude.
#[must_use]
pub(crate) fn integer_upper_bound(n: &Number, exclusive: bool) -> Option<i128> {
    let bound: i128 = match Num::of(n) {
        Num::Int(i) if exclusive => i.checked_sub(1)?,
        Num::Int(i) => i,
        Num::Decimal(d) => {
            let (floor, ceil): (i128, i128) = d.floor_and_ceil()?;
            if exclusive {
                ceil.checked_sub(1)?
            } else {
                floor
            }
        }
    };
    within_bound_limit(bound)
}

/// The value of `n` as a `u64` when it is a non-negative integer by value (`2`, `2.0`, `2e0`,
/// `-0`). The count keywords (`minLength`, `maxItems`, ...) read their value with it.
#[must_use]
pub(crate) fn non_negative_integer(n: &Number) -> Option<u64> {
    match Num::of(n) {
        Num::Int(i) => u64::try_from(i).ok(),
        Num::Decimal(d) => {
            if d.negative {
                return None;
            }
            u64::try_from(d.whole_i128()?).ok()
        }
    }
}

/// Whether `n` is strictly greater than zero (the valid values of `multipleOf`).
#[must_use]
pub(crate) fn is_positive(n: &Number) -> bool {
    match Num::of(n) {
        Num::Int(i) => i > 0,
        Num::Decimal(d) => d.sign() > 0,
    }
}

/// Whether `value` is an integer multiple of `divisor` (`value = k · divisor` for an integer
/// `k`), decided exactly on the decimals as written. See [`decimal_is_multiple`]. Parsing
/// rejects a `multipleOf` of zero or below; for a hand-built model, only `0` is a multiple of
/// `0` and signs do not matter.
#[must_use]
pub(crate) fn is_multiple_of(value: &Number, divisor: &Number) -> bool {
    match (Num::of(value), Num::of(divisor)) {
        (Num::Int(v), Num::Int(d)) => {
            if d == 0 {
                v == 0
            } else {
                v % d == 0
            }
        }
        (v, d) => {
            let mut v_buffer: [u8; INT_DIGITS] = [0; INT_DIGITS];
            let mut d_buffer: [u8; INT_DIGITS] = [0; INT_DIGITS];
            decimal_is_multiple(
                &v.into_decimal(&mut v_buffer),
                &d.into_decimal(&mut d_buffer),
            )
        }
    }
}

/// Whether `value = vm · 10^ve` is an integer multiple of `divisor = dm · 10^de` (both
/// normalised, so neither mantissa is divisible by 10).
///
/// `value / divisor = (vm / dm) · 10^shift` with `shift = ve - de`. When `shift < 0` the
/// quotient is an integer only if `dm · 10^-shift` divides `vm`, which needs `10 | vm`: never,
/// for a normalised non-zero `vm`. Otherwise it is an integer exactly when
/// `(vm mod dm) · (10^shift mod dm) ≡ 0 (mod dm)`, computed by modular exponentiation, so a
/// huge `shift` costs `O(log shift)` steps and no power of ten is built. A divisor mantissa
/// within `u64` needs no big-integer arithmetic.
fn decimal_is_multiple(value: &Decimal<'_>, divisor: &Decimal<'_>) -> bool {
    if value.is_zero() {
        return true;
    }
    if divisor.is_zero() {
        return false;
    }
    let shift: i128 = i128::from(value.exponent) - i128::from(divisor.exponent);
    let Ok(shift) = u128::try_from(shift) else {
        return false;
    };
    if let Some(modulus) = digits_u128(&divisor.digits).and_then(|d| u64::try_from(d).ok()) {
        let modulus: u128 = u128::from(modulus);
        let remainder: u128 = value.digits.iter().fold(0, |acc: u128, b| {
            (acc * 10 + u128::from(b - b'0')) % modulus
        });
        let scale: u128 = pow_mod_10(shift, modulus);
        (remainder * scale).is_multiple_of(modulus)
    } else {
        // Both parse: normalised digits are non-empty ASCII digits.
        let (Some(vm), Some(dm)) = (
            BigUint::parse_bytes(&value.digits, 10),
            BigUint::parse_bytes(&divisor.digits, 10),
        ) else {
            return false;
        };
        let scale: BigUint = BigUint::from(10_u8).modpow(&BigUint::from(shift), &dm);
        ((vm % &dm) * scale % &dm) == BigUint::ZERO
    }
}

/// `10^exponent mod modulus` for a `modulus` within `u64` (so every product fits `u128`).
fn pow_mod_10(mut exponent: u128, modulus: u128) -> u128 {
    let mut result: u128 = 1 % modulus;
    let mut base: u128 = 10 % modulus;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result * base % modulus;
        }
        base = base * base % modulus;
        exponent >>= 1;
    }
    result
}

/// Hashes a number so that numbers equal under [`cmp_num`] hash equally: a whole number within
/// `i128` as that integer (as the default build does), any other number by its normalised
/// sign, digits and exponent.
pub(super) fn hash_number<H: Hasher>(n: &Number, state: &mut H) {
    match Num::of(n) {
        Num::Int(i) => {
            HashTag::Integer.hash(state);
            i.hash(state);
        }
        Num::Decimal(d) => {
            if let Some(i) = d.whole_i128() {
                HashTag::Integer.hash(state);
                i.hash(state);
            } else {
                HashTag::Decimal.hash(state);
                d.negative.hash(state);
                d.digits.hash(state);
                d.exponent.hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Decimal, cmp_num, integer_lower_bound, integer_upper_bound, is_integer, is_multiple_of,
        is_positive, non_negative_integer,
    };
    use crate::json_schema::IntegerSemantics;
    use crate::number::{JsonEqKey, json_equal, json_hash};
    use serde_json::{Number, Value};
    use std::borrow::Cow;
    use std::cmp::Ordering;
    use std::collections::HashSet;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    const TWO_POW_64: &str = "18446744073709551616";
    const TWO_POW_64_PLUS_ONE: &str = "18446744073709551617";

    fn num(text: &str) -> Number {
        serde_json::from_str(text).expect("valid JSON number")
    }

    fn value(text: &str) -> Value {
        serde_json::from_str(text).expect("valid JSON")
    }

    fn hash_of(value: &Value) -> u64 {
        let mut hasher: DefaultHasher = DefaultHasher::new();
        json_hash(value, &mut hasher);
        hasher.finish()
    }

    fn decimal(negative: bool, digits: &str, exponent: i64) -> Decimal<'_> {
        Decimal {
            negative,
            digits: Cow::Borrowed(digits.as_bytes()),
            exponent,
        }
    }

    // --- Parsing and normalisation ---

    #[test]
    fn decimal_parse_moves_trailing_zeros_into_exponent() {
        let expected: Option<Decimal<'_>> = Some(decimal(false, "12", 1));
        let actual: Option<Decimal<'_>> = Decimal::parse("120.000");
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_strips_leading_zeros_of_a_fraction() {
        let expected: Option<Decimal<'_>> = Some(decimal(true, "75", -4));
        let actual: Option<Decimal<'_>> = Decimal::parse("-0.0075");
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_joins_digits_across_the_point() {
        let expected: Option<Decimal<'_>> = Some(decimal(false, "1234", 3));
        let actual: Option<Decimal<'_>> = Decimal::parse("12.340e+5");
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_reads_upper_case_and_unsigned_exponents() {
        let expected: (Option<Decimal<'_>>, Option<Decimal<'_>>) =
            (Some(decimal(false, "15", 0)), Some(decimal(false, "15", 0)));
        let actual: (Option<Decimal<'_>>, Option<Decimal<'_>>) =
            (Decimal::parse("1.5E1"), Decimal::parse("1.5e1"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_has_one_zero() {
        let expected: [Option<Decimal<'_>>; 4] = [
            Some(Decimal::ZERO),
            Some(Decimal::ZERO),
            Some(Decimal::ZERO),
            Some(Decimal::ZERO),
        ];
        let actual: [Option<Decimal<'_>>; 4] = [
            Decimal::parse("-0"),
            Decimal::parse("0.000"),
            Decimal::parse("-0.0e-7"),
            Decimal::parse("0e99999999999999999999"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_rejects_text_that_is_not_a_number() {
        let expected: [Option<Decimal<'_>>; 6] = [None, None, None, None, None, None];
        let actual: [Option<Decimal<'_>>; 6] = [
            Decimal::parse(""),
            Decimal::parse("-"),
            Decimal::parse("1."),
            Decimal::parse(".5"),
            Decimal::parse("1e"),
            Decimal::parse("1x"),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn decimal_parse_saturates_an_exponent_beyond_the_limit() {
        let expected: (Option<Decimal<'_>>, Option<Decimal<'_>>) = (
            Some(decimal(false, "1", 1 << 62)),
            Some(decimal(false, "1", -(1 << 62))),
        );
        let actual: (Option<Decimal<'_>>, Option<Decimal<'_>>) = (
            Decimal::parse("1e99999999999999999999"),
            Decimal::parse("1e-99999999999999999999"),
        );
        assert_eq!(expected, actual);
    }

    // --- cmp_num and json_equal ---

    #[test]
    fn cmp_num_distinguishes_two_pow_64_and_its_successor() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num(TWO_POW_64), &num(TWO_POW_64_PLUS_ONE));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_bignums_beyond_i128() {
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(
            &num("12345678910111213141516171819202122232425262728293032"),
            &num("12345678910111213141516171819202122232425262728293031"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_number_beyond_f64_range_parses_and_compares_above_1e308() {
        let expected: Ordering = Ordering::Greater;
        let actual: Ordering = cmp_num(&num("1e400"), &num("1e308"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_decimals_beyond_f64_precision() {
        // The official suite's `exclusiveMaximum` bignum case.
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(
            &num("972783798187987123879878123.18878137"),
            &num("972783798187987123879878123.188781371"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_negative_bignums_order_by_magnitude_reversed() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num("-1e400"), &num("-1e308"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_integer_against_equal_decimal() {
        let expected: [Ordering; 3] = [Ordering::Equal; 3];
        let actual: [Ordering; 3] = [
            cmp_num(&num("100"), &num("1e2")),
            cmp_num(&num("-7"), &num("-70e-1")),
            cmp_num(&num("1"), &num("1.0")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_integer_against_nearby_decimals() {
        let expected: [Ordering; 3] = [Ordering::Less, Ordering::Greater, Ordering::Less];
        let actual: [Ordering; 3] = [
            cmp_num(&num("12"), &num("12.5")),
            cmp_num(&num("-12"), &num("-12.5")),
            cmp_num(&num("-1"), &num("0.5")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_digits_order_within_one_adjusted_exponent() {
        let expected: Ordering = Ordering::Less;
        let actual: Ordering = cmp_num(&num("0.12"), &num("0.123"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_i128_extremes_against_decimals() {
        let expected: [Ordering; 2] = [Ordering::Equal, Ordering::Equal];
        let actual: [Ordering; 2] = [
            cmp_num(
                &num("-170141183460469231731687303715884105728"),
                &num("-1.70141183460469231731687303715884105728e38"),
            ),
            cmp_num(
                &num("170141183460469231731687303715884105727"),
                &num("170141183460469231731687303715884105727.0"),
            ),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_negative_zero_equals_zero() {
        let expected: [Ordering; 2] = [Ordering::Equal, Ordering::Equal];
        let actual: [Ordering; 2] = [
            cmp_num(&num("-0"), &num("0")),
            cmp_num(&num("-0.0"), &num("0")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_tiny_and_huge_exponents_without_panicking() {
        let expected: [Ordering; 4] = [
            Ordering::Greater,
            Ordering::Less,
            Ordering::Greater,
            Ordering::Less,
        ];
        let actual: [Ordering; 4] = [
            cmp_num(&num("1e99999999999999999999"), &num(TWO_POW_64)),
            cmp_num(&num("1e-99999999999999999999"), &num("1e-400")),
            cmp_num(&num("1e-99999999999999999999"), &num("0")),
            cmp_num(&num("-1e99999999999999999999"), &num("-5")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn cmp_num_exponents_beyond_the_limit_compare_equal() {
        // The documented saturation: both exponents read as 2^62.
        let expected: Ordering = Ordering::Equal;
        let actual: Ordering = cmp_num(
            &num("1e99999999999999999999"),
            &num("1e99999999999999999998"),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_distinguishes_two_pow_64_and_its_successor() {
        let expected: bool = false;
        let actual: bool = json_equal(&value(TWO_POW_64), &value(TWO_POW_64_PLUS_ONE));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_equal_bignum_written_two_ways() {
        let expected: bool = true;
        let actual: bool = json_equal(
            &value("[18446744073709551616000]"),
            &value("[1.8446744073709551616e22]"),
        );
        assert_eq!(expected, actual);
    }

    // --- json_hash and JsonEqKey ---

    #[test]
    fn json_eq_key_keeps_two_pow_64_and_its_successor_apart() {
        let values: Vec<Value> = vec![value(TWO_POW_64), value(TWO_POW_64_PLUS_ONE)];
        let set: HashSet<JsonEqKey<'_>> = values.iter().map(JsonEqKey).collect();
        let expected: usize = 2;
        let actual: usize = set.len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_eq_key_merges_one_number_written_four_ways() {
        let values: Vec<Value> = ["1", "1.0", "1e0", "10e-1"].map(value).to_vec();
        let set: HashSet<JsonEqKey<'_>> = values.iter().map(JsonEqKey).collect();
        let expected: usize = 1;
        let actual: usize = set.len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_one_number_written_four_ways_agrees() {
        let expected: [u64; 4] = [hash_of(&value("1")); 4];
        let actual: [u64; 4] = ["1", "1.0", "1e0", "10e-1"].map(|text| hash_of(&value(text)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_zeros_agree() {
        let expected: [u64; 3] = [hash_of(&value("0")); 3];
        let actual: [u64; 3] = ["-0", "0.0", "-0e5"].map(|text| hash_of(&value(text)));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_i128_min_written_as_decimal_agrees() {
        let expected: u64 = hash_of(&value("-170141183460469231731687303715884105728"));
        let actual: u64 = hash_of(&value("-1.70141183460469231731687303715884105728e38"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_bignum_beyond_i128_written_two_ways_agrees() {
        let expected: u64 = hash_of(&value("1000000000000000000000000000000000000000000"));
        let actual: u64 = hash_of(&value("1e42"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn json_hash_fractions_written_two_ways_agree() {
        let expected: u64 = hash_of(&value("0.5"));
        let actual: u64 = hash_of(&value("50e-2"));
        assert_eq!(expected, actual);
    }

    // --- is_integer ---

    #[test]
    fn is_integer_lexical_accepts_bignum() {
        let expected: bool = true;
        let actual: bool = is_integer(
            &num("12345678910111213141516171819202122232425262728293031"),
            IntegerSemantics::Lexical,
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_lexical_rejects_fraction_and_exponent() {
        let expected: [bool; 3] = [false, false, false];
        let actual: [bool; 3] = [
            is_integer(&num("1.0"), IntegerSemantics::Lexical),
            is_integer(&num("1e3"), IntegerSemantics::Lexical),
            is_integer(&num("1E3"), IntegerSemantics::Lexical),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_accepts_zero_fraction_forms() {
        let expected: [bool; 5] = [true; 5];
        let actual: [bool; 5] = [
            is_integer(&num("1.0"), IntegerSemantics::Numeric),
            is_integer(&num("1e3"), IntegerSemantics::Numeric),
            is_integer(&num("1.5e1"), IntegerSemantics::Numeric),
            is_integer(&num("-0.0"), IntegerSemantics::Numeric),
            is_integer(&num("1e400"), IntegerSemantics::Numeric),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_rejects_fractions() {
        let expected: [bool; 3] = [false, false, false];
        let actual: [bool; 3] = [
            is_integer(&num("1.5"), IntegerSemantics::Numeric),
            is_integer(&num("1.05e1"), IntegerSemantics::Numeric),
            is_integer(&num("1e-400"), IntegerSemantics::Numeric),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_integer_numeric_rejects_fraction_beyond_f64_precision() {
        let expected: bool = false;
        let actual: bool = is_integer(&num("10000000000000000000.5"), IntegerSemantics::Numeric);
        assert_eq!(expected, actual);
    }

    // --- Integer bounds ---

    #[test]
    fn integer_lower_bound_of_fractions() {
        let expected: [Option<i128>; 4] = [Some(2), Some(-1), Some(2), Some(-1)];
        let actual: [Option<i128>; 4] = [
            integer_lower_bound(&num("1.5"), false),
            integer_lower_bound(&num("-1.5"), false),
            integer_lower_bound(&num("1.5"), true),
            integer_lower_bound(&num("-1.5"), true),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_upper_bound_of_fractions() {
        let expected: [Option<i128>; 4] = [Some(1), Some(-2), Some(1), Some(-2)];
        let actual: [Option<i128>; 4] = [
            integer_upper_bound(&num("1.5"), false),
            integer_upper_bound(&num("-1.5"), false),
            integer_upper_bound(&num("1.5"), true),
            integer_upper_bound(&num("-1.5"), true),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_bounds_of_whole_numbers() {
        let expected: [Option<i128>; 4] = [Some(100), Some(101), Some(100), Some(99)];
        let actual: [Option<i128>; 4] = [
            integer_lower_bound(&num("1e2"), false),
            integer_lower_bound(&num("1e2"), true),
            integer_upper_bound(&num("100.0"), false),
            integer_upper_bound(&num("100"), true),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_bounds_of_small_fractions() {
        let expected: [Option<i128>; 2] = [Some(1), Some(-1)];
        let actual: [Option<i128>; 2] = [
            integer_lower_bound(&num("1e-400"), false),
            integer_upper_bound(&num("-1e-400"), false),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_bounds_of_two_pow_64_successor_are_exact() {
        let expected: [Option<i128>; 2] = [
            Some(18_446_744_073_709_551_617),
            Some(18_446_744_073_709_551_618),
        ];
        let actual: [Option<i128>; 2] = [
            integer_lower_bound(&num(TWO_POW_64_PLUS_ONE), false),
            integer_lower_bound(&num(TWO_POW_64_PLUS_ONE), true),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_bounds_beyond_two_pow_120_are_none() {
        let expected: [Option<i128>; 4] = [None, None, None, None];
        let actual: [Option<i128>; 4] = [
            integer_lower_bound(&num("1e40"), false),
            integer_upper_bound(&num("-1e40"), false),
            integer_lower_bound(
                &num("12345678910111213141516171819202122232425262728293031"),
                true,
            ),
            // i128::MAX: in range for `i128`, beyond 2^120.
            integer_upper_bound(&num("170141183460469231731687303715884105727"), false),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn integer_bounds_of_huge_exponents_are_none() {
        let expected: [Option<i128>; 2] = [None, Some(0)];
        let actual: [Option<i128>; 2] = [
            integer_lower_bound(&num("1e99999999999999999999"), false),
            integer_upper_bound(&num("1e-99999999999999999999"), false),
        ];
        assert_eq!(expected, actual);
    }

    // --- Count keywords and multipleOf validity ---

    #[test]
    fn non_negative_integer_reads_every_whole_form() {
        let expected: [Option<u64>; 5] = [Some(2), Some(2), Some(2), Some(0), Some(u64::MAX)];
        let actual: [Option<u64>; 5] = [
            non_negative_integer(&num("2")),
            non_negative_integer(&num("2.0")),
            non_negative_integer(&num("2e0")),
            non_negative_integer(&num("-0.0")),
            non_negative_integer(&num("18446744073709551615.0")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_negative_integer_rejects_negative_fraction_and_beyond_u64() {
        let expected: [Option<u64>; 5] = [None, None, None, None, None];
        let actual: [Option<u64>; 5] = [
            non_negative_integer(&num("-1")),
            non_negative_integer(&num("-1.0")),
            non_negative_integer(&num("1e-400")),
            non_negative_integer(&num(TWO_POW_64)),
            non_negative_integer(&num("1e400")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn is_positive_is_exact() {
        let expected: [bool; 5] = [true, true, false, false, false];
        let actual: [bool; 5] = [
            is_positive(&num("1e-400")),
            is_positive(&num("1e400")),
            is_positive(&num("0.0")),
            is_positive(&num("-0")),
            is_positive(&num("-1e-400")),
        ];
        assert_eq!(expected, actual);
    }

    // --- multipleOf ---

    #[test]
    fn multiple_of_two_pow_64_and_its_successor_by_two() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num(TWO_POW_64), &num("2")),
            is_multiple_of(&num(TWO_POW_64_PLUS_ONE), &num("2")),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_bignum_beyond_i128_by_integer() {
        // A number is a multiple of 3 exactly when its digit sum is.
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(
                &num("1000000000000000000000000000000000000000002"),
                &num("3"),
            ),
            is_multiple_of(
                &num("1000000000000000000000000000000000000000001"),
                &num("3"),
            ),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_number_beyond_f64_range_by_half() {
        let expected: bool = true;
        let actual: bool = is_multiple_of(&num("1e400"), &num("0.5"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_huge_exponents_decide_without_building_powers_of_ten() {
        let expected: [bool; 4] = [true, true, false, false];
        let actual: [bool; 4] = [
            is_multiple_of(&num("1e100000000"), &num("1e-100000000")),
            is_multiple_of(&num("1e100000000"), &num("0.5")),
            // 10^k is never a multiple of 7.
            is_multiple_of(&num("1e99999999999999999999"), &num("7e-3")),
            is_multiple_of(&num("1e-100000000"), &num("1e100000000")),
        ];
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_irregular_decimal_beyond_f64_range() {
        let expected: bool = false;
        let actual: bool = is_multiple_of(&num("1e400"), &num("0.123456789"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_divisor_beyond_u64() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(
                &num("3.6893488147419103234e19"),
                &num("1.8446744073709551617e1"),
            ),
            is_multiple_of(
                &num("3.6893488147419103235e19"),
                &num("1.8446744073709551617e1"),
            ),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_divisor_beyond_u64_with_huge_shift() {
        // 2^64 + 1 divides 10^k exactly never (it is odd and not a power of 5), so
        // 1e100000000 / 18446744073709551617 is not an integer.
        let expected: bool = false;
        let actual: bool = is_multiple_of(&num("1e100000000"), &num("18446744073709551617.0"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_fraction_by_larger_fraction_is_false() {
        let expected: bool = false;
        let actual: bool = is_multiple_of(&num("0.5"), &num("0.75"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn multiple_of_decimal_by_decimal_beyond_f64_precision() {
        let expected: (bool, bool) = (true, false);
        let actual: (bool, bool) = (
            is_multiple_of(&num("0.00000000000000000000000000000003"), &num("1e-32")),
            is_multiple_of(&num("0.000000000000000000000000000000031"), &num("1e-32")),
        );
        assert_eq!(expected, actual);
    }
}
